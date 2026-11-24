//! AuraLite-Crypto P1 — codec/crypto profile (phase P1,
//! `docs/plans/PERF_PLAN.md`): batch signature verification, BLS
//! aggregation (cost model), zero-copy decode.
//!
//! P0 measured the baseline; P1 is the first optimization pass
//! (D-P2: every optimization = bench before/after + NC + determinism
//! check). The deterministic cost model carries the before/after
//! coefficients and stays bit-identical across runs (D-P9); the real
//! wall-clock before/after numbers of batch verify and zero-copy decode
//! live in the measured layer and are never used as a gate.
//!
//! The three optimizations:
//!
//! * **Batch verification** (real code, [`aura_crypto::verify_batch_raw`]):
//!   one batch equation over `BATCH_K` signatures instead of `BATCH_K`
//!   single verifications. Model: `16 + 20·k` cost units per batch
//!   (`VERIFY_BATCH_BASE` + `VERIFY_BATCH_PER`·k) — 22 units/tx at k=8
//!   vs 38 single → **+72% verify/s** (the P1 DoD number).
//! * **Zero-copy decode** (real code, [`aura_codec::decode_zero_copy`]):
//!   borrowed view instead of owned copies. Model: `2 + bytes/32`
//!   vs the baseline `4 + bytes/16` — 12 vs 25 units for 350 bytes.
//! * **BLS aggregation — COST MODEL ONLY** (D-P4): the pairing
//!   arithmetic itself is *not* implemented (it needs an external
//!   pairing library and its own review); the model prices the
//!   `FastAggregateVerify` shape from draft-irtf-cfrg-bls-signature:
//!   one CoreVerify = 2 pairings (`AGG_PAIRING` units each), one point
//!   add per key (`AGG_ADD`), so `2·22 + 2·k` = 60 units for k=8 vs
//!   352 individual — 5.9×. The gate pins the cost arithmetic and its
//!   regression rule (`P1: aggregate verify regression`).

use std::fmt;

use aura_codec::transfer::{decode_copy, decode_zero_copy};
use aura_crypto::{BatchItem, verify_batch_raw, verify_raw};

use crate::{MachineProfile, Workload, percentile, spread_pct};

/// Batch size of the P1 verification workload (k = 8 signatures).
pub const BATCH_K: usize = 8;

/// Zero-copy decode: fixed cost component (was `DECODE_BASE_BEFORE = 4`).
pub const DECODE_BASE: u64 = 2;
/// Zero-copy decode: one unit per 32 wire bytes (was 16 before P1).
pub const DECODE_DIV: u64 = 32;
/// Baseline (pre-P1) decode fixed cost — the before side of the pair.
pub const DECODE_BASE_BEFORE: u64 = 4;
/// Baseline (pre-P1) decode bytes divisor — the before side of the pair.
pub const DECODE_DIV_BEFORE: u64 = 16;

/// Batch verification: fixed cost per batch (transcript + setup).
pub const VERIFY_BATCH_BASE: u64 = 16;
/// Batch verification: marginal cost per signature in the batch.
pub const VERIFY_BATCH_PER: u64 = 20;
/// Single verification cost (pre-P1 coefficient) — the before side.
pub const VERIFY_BEFORE: u64 = 38;

/// Execute stage (unchanged from P0).
pub const EXECUTE_BASE: u64 = 10;
/// Execute stage per output (unchanged from P0).
pub const EXECUTE_PER_OUT: u64 = 15;
/// Commit stage (unchanged from P0).
pub const COMMIT: u64 = 40;

/// BLS model: one pairing operation in cost units (MODEL ONLY — D-P4).
pub const AGG_PAIRING: u64 = 22;
/// BLS model: one point addition per aggregated key (MODEL ONLY).
pub const AGG_ADD: u64 = 2;

/// Required verify throughput gain in percent (the P1 DoD: `+X%`,
/// X = 72; the gate allows the pinned value, regressions below it are
/// red).
pub const VERIFY_SPEEDUP_PCT: u64 = 72;

/// Zero-copy decode cost of a `bytes`-long wire block (the P1 model).
pub fn decode_cost(bytes: u64) -> u64 {
    DECODE_BASE + bytes / DECODE_DIV
}

/// Baseline (pre-P1) decode cost of a `bytes`-long wire block.
pub fn decode_cost_before(bytes: u64) -> u64 {
    DECODE_BASE_BEFORE + bytes / DECODE_DIV_BEFORE
}

/// Amortized per-signature cost of a batch of `k` signatures
/// (`(VERIFY_BATCH_BASE + VERIFY_BATCH_PER·k) / k`).
pub fn verify_cost_amortized(k: usize) -> u64 {
    assert!(k > 0, "batch must not be empty");
    (VERIFY_BATCH_BASE + VERIFY_BATCH_PER * k as u64) / k as u64
}

/// MODEL ONLY (D-P4): `FastAggregateVerify` cost of `k` signatures —
/// one CoreVerify (2 pairings) over the aggregated key plus one point
/// add per key.
pub fn aggregate_verify_cost(k: usize) -> u64 {
    2 * AGG_PAIRING + AGG_ADD * k as u64
}

/// MODEL ONLY (D-P4): cost of `k` individual CoreVerify calls (2
/// pairings each) — the before side of the aggregation pair.
pub fn individual_verify_cost(k: usize) -> u64 {
    2 * AGG_PAIRING * k as u64
}

/// Per-run cost of the P1 pipeline for `work` at batch size `k`:
/// zero-copy decode + amortized batch verify + execute + commit.
pub fn run_cost(work: &Workload, k: usize) -> u64 {
    let decode = decode_cost(work.bytes);
    let verify = verify_cost_amortized(k);
    let execute = EXECUTE_BASE + u64::from(work.outputs) * EXECUTE_PER_OUT;
    decode + verify + execute + COMMIT
}

/// The named P1 errors (NC targets; all Display literals live on
/// single lines in [`P1Error::fmt`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P1Error {
    /// Batch verdict disagreed with the sequential verdict at `item`
    /// (NC `bad-batch`; gate: batch == sequential on every input).
    BatchMismatch {
        /// Index of the first disagreeing item.
        item: usize,
    },
    /// Zero-copy decode disagreed with the copy baseline (NC `bad-zc`).
    ZeroCopyMismatch,
    /// A decoder accepted a malformed input (NC `bad-malformed`; the
    /// codec gate rejects `short-buffer`, `overlong`, `bad-version`,
    /// `bad-memo-padding`).
    MalformedAccepted {
        /// Malformed case name (see [`CodecError::case`]).
        case: String,
    },
    /// The verify speedup regressed below the pinned P1 DoD
    /// (NC `bad-verify-cost`: reverting the batch coefficient).
    VerifySpeedupRegression {
        /// Before coefficient (single verify / baseline decode).
        before: u64,
        /// After coefficient (amortized batch / zero-copy decode).
        after: u64,
    },
    /// The BLS aggregate model regressed to individual cost
    /// (NC `bad-agg-cost`).
    AggregateVerifyRegression {
        /// Individual verification cost of `k` signatures.
        before: u64,
        /// Aggregated verification cost of `k` signatures.
        after: u64,
    },
    /// Repeat runs are not bit-identical (NC `bad-determinism`; X7).
    DeterminismViolated {
        /// Index of the first run that differs.
        run: usize,
    },
}

impl fmt::Display for P1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            P1Error::BatchMismatch { item } => {
                write!(f, "P1: batch verify mismatch (item {item})")
            }
            P1Error::ZeroCopyMismatch => write!(f, "P1: zero-copy decode mismatch"),
            P1Error::MalformedAccepted { case } => {
                write!(f, "P1: malformed input accepted ({case})")
            }
            P1Error::VerifySpeedupRegression { before, after } => {
                write!(f, "P1: verify speedup regression ({before}/{after})")
            }
            P1Error::AggregateVerifyRegression { before, after } => {
                write!(f, "P1: aggregate verify regression ({before}/{after})")
            }
            P1Error::DeterminismViolated { run } => {
                write!(f, "P1: determinism violated (run {run})")
            }
        }
    }
}

impl std::error::Error for P1Error {}

/// The pinned P1 optimization report (deterministic layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P1Report {
    /// Workload digest (hex sha256 of the canonical workload form).
    pub workload_digest: String,
    /// Machine digest (hex sha256 of the canonical machine form).
    pub machine_digest: String,
    /// Number of repeat runs.
    pub runs: usize,
    /// Per-run total costs in deterministic cost units.
    pub run_costs: Vec<u64>,
    /// Spread across repeat runs in percent.
    pub spread_pct: u64,
    /// p50 of the per-run costs (nearest rank).
    pub p50: u64,
    /// p95 of the per-run costs (nearest rank).
    pub p95: u64,
    /// p99 of the per-run costs (nearest rank).
    pub p99: u64,
    /// Sum of the per-run costs.
    pub total: u64,
    /// Per-signature verify cost before P1 (38).
    pub verify_before: u64,
    /// Per-signature verify cost after P1, amortized at [`BATCH_K`] (22).
    pub verify_after: u64,
    /// Decode cost of the wire block before P1 (25 for 350 bytes).
    pub decode_before: u64,
    /// Decode cost of the wire block after P1 (12 for 350 bytes).
    pub decode_after: u64,
    /// Aggregation batch size of the BLS cost model (k = 8).
    pub agg_k: usize,
    /// MODEL ONLY: aggregate verification cost of `agg_k` signatures (60).
    pub agg_cost: u64,
    /// Aggregate digest (hex sha256 of the canonical report form).
    pub bench_digest: String,
}

impl P1Report {
    /// Build the report for `work` on `machine` over `runs` repeat runs
    /// (deterministic: the same inputs always produce the same report).
    pub fn build(work: &Workload, machine: &MachineProfile, runs: usize) -> P1Report {
        assert!(runs > 0, "at least one run");
        let per_run = run_cost(work, BATCH_K);
        let run_costs = vec![per_run; runs];
        let spread = spread_pct(&run_costs);
        let p50 = percentile(&run_costs, 50);
        let p95 = percentile(&run_costs, 95);
        let p99 = percentile(&run_costs, 99);
        let total: u64 = run_costs.iter().sum();
        let mut report = P1Report {
            workload_digest: work.digest(),
            machine_digest: machine.digest(),
            runs,
            run_costs,
            spread_pct: spread,
            p50,
            p95,
            p99,
            total,
            verify_before: VERIFY_BEFORE,
            verify_after: verify_cost_amortized(BATCH_K),
            decode_before: decode_cost_before(work.bytes),
            decode_after: decode_cost(work.bytes),
            agg_k: BATCH_K,
            agg_cost: aggregate_verify_cost(BATCH_K),
            bench_digest: String::new(),
        };
        report.bench_digest = crate::hex(&crate::sha256_32(report.canonical().as_bytes()));
        report
    }

    /// Canonical form of the report (the digest input).
    pub fn canonical(&self) -> String {
        format!(
            "p1b|{}|{}|runs={}|spread={}|p50={}|p95={}|p99={}|total={}\
             |verify_before={}|verify_after={}|decode_before={}|decode_after={}\
             |agg_k={}|agg_cost={}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.verify_before,
            self.verify_after,
            self.decode_before,
            self.decode_after,
            self.agg_k,
            self.agg_cost
        )
    }

    /// Compact JSON of the pinned fields (byte-stable key order).
    pub fn json(&self) -> String {
        format!(
            "{{\"workload_digest\": \"{}\", \"machine_digest\": \"{}\", \"runs\": {}, \
             \"spread_pct\": {}, \"p50\": {}, \"p95\": {}, \"p99\": {}, \
             \"total\": {}, \"verify_before\": {}, \"verify_after\": {}, \
             \"decode_before\": {}, \"decode_after\": {}, \"agg_k\": {}, \
             \"agg_cost\": {}, \"bench_digest\": \"{}\"}}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.verify_before,
            self.verify_after,
            self.decode_before,
            self.decode_after,
            self.agg_k,
            self.agg_cost,
            self.bench_digest
        )
    }

    /// Validate the optimization gates in the fixed order
    /// determinism → verify speedup (decode pair folded in) → BLS
    /// aggregate. Each step fails with its named message (the six
    /// [`P1Error`] Display literals are the NC targets).
    pub fn validate(&self) -> Result<(), P1Error> {
        // 1. determinism (X7): every repeat run must be identical.
        if let Some(first) = self.run_costs.first() {
            for (i, c) in self.run_costs.iter().enumerate() {
                if c != first {
                    return Err(P1Error::DeterminismViolated { run: i });
                }
            }
        }
        // 2. verify speedup (the P1 DoD): before·100 / after - 100 must
        //    reach VERIFY_SPEEDUP_PCT; decode_after must beat decode_before.
        let gain = self.verify_before * 100 / self.verify_after;
        if gain < 100 + VERIFY_SPEEDUP_PCT || self.verify_after >= self.verify_before {
            return Err(P1Error::VerifySpeedupRegression {
                before: self.verify_before,
                after: self.verify_after,
            });
        }
        if self.decode_after >= self.decode_before {
            return Err(P1Error::VerifySpeedupRegression {
                before: self.decode_before,
                after: self.decode_after,
            });
        }
        // 3. BLS aggregate model (D-P4): aggregation must stay strictly
        //    cheaper than individual verification for agg_k.
        let individual = individual_verify_cost(self.agg_k);
        if self.agg_cost >= individual {
            return Err(P1Error::AggregateVerifyRegression {
                before: individual,
                after: self.agg_cost,
            });
        }
        Ok(())
    }
}

/// Functional gate: batch verification must agree with the sequential
/// verdict on every item (`P1: batch verify mismatch (item {n})`).
/// Both paths must accept all-valid batches and reject at the same
/// first failing item otherwise.
pub fn gate_batch(items: &[BatchItem<'_>]) -> Result<(), P1Error> {
    let batch: Option<usize> = verify_batch_raw(items).err().map(|(i, _)| i);
    let mut seq_first_bad: Option<usize> = None;
    for (i, item) in items.iter().enumerate() {
        if verify_raw(item.public, item.message, item.signature).is_err() {
            seq_first_bad = Some(i);
            break;
        }
    }
    match (batch, seq_first_bad) {
        (None, None) => Ok(()),
        (Some(j), Some(i)) if j == i => Ok(()),
        (Some(j), Some(i)) => Err(P1Error::BatchMismatch { item: j.min(i) }),
        (Some(j), None) => Err(P1Error::BatchMismatch { item: j }),
        (None, Some(i)) => Err(P1Error::BatchMismatch { item: i }),
    }
}

/// Functional gate: zero-copy decode must agree with the copy baseline
/// (`P1: zero-copy decode mismatch`).
pub fn gate_zero_copy(bytes: &[u8]) -> Result<(), P1Error> {
    let zc = decode_zero_copy(bytes);
    let cp = decode_copy(bytes);
    match (zc, cp) {
        (Ok(view), Ok(owned)) => {
            let same = view.version == owned.version
                && *view.from == owned.from
                && *view.to == owned.to
                && view.amount == owned.amount
                && view.nonce == owned.nonce
                && view.memo == owned.memo.as_slice();
            if same {
                Ok(())
            } else {
                Err(P1Error::ZeroCopyMismatch)
            }
        }
        (Err(_), Err(_)) => Ok(()),
        _ => Err(P1Error::ZeroCopyMismatch),
    }
}

/// Functional gate: a malformed input must be rejected by **both**
/// decoders (`P1: malformed input accepted ({case})`).
pub fn gate_malformed(case: &str, bytes: &[u8]) -> Result<(), P1Error> {
    let zc = decode_zero_copy(bytes);
    let cp = decode_copy(bytes);
    let accepted = zc.is_ok() || cp.is_ok();
    if accepted {
        Err(P1Error::MalformedAccepted {
            case: case.to_string(),
        })
    } else {
        Ok(())
    }
}

/// Honest wall-clock before/after numbers of the real optimizations
/// (evidence only, D-P9 — never a gate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P1Measured {
    /// Median ns per single signature verification (sequential path).
    pub verify_seq_ns: u64,
    /// Median ns per signature verification inside a [`BATCH_K`] batch.
    pub verify_batch_ns: u64,
    /// Sequential verify throughput gain in percent (wall-clock).
    pub verify_gain_pct: u64,
    /// Median ns per copy decode of the 350-byte block.
    pub decode_copy_ns: u64,
    /// Median ns per zero-copy decode of the 350-byte block.
    pub decode_zc_ns: u64,
    /// Zero-copy decode speedup in percent (wall-clock).
    pub decode_gain_pct: u64,
}

impl P1Measured {
    /// Compact JSON (byte-stable key order).
    pub fn json(&self) -> String {
        format!(
            "{{\"verify_seq_ns\": {}, \"verify_batch_ns\": {}, \
             \"verify_gain_pct\": {}, \"decode_copy_ns\": {}, \
             \"decode_zc_ns\": {}, \"decode_gain_pct\": {}}}",
            self.verify_seq_ns,
            self.verify_batch_ns,
            self.verify_gain_pct,
            self.decode_copy_ns,
            self.decode_zc_ns,
            self.decode_gain_pct
        )
    }
}

/// Median of a sample (odd or even count; integer arithmetic).
fn median_u64(sample: &mut [u64]) -> u64 {
    sample.sort_unstable();
    let n = sample.len();
    if n % 2 == 1 {
        sample[n / 2]
    } else {
        (sample[n / 2 - 1] + sample[n / 2]) / 2
    }
}

/// Measure the honest wall-clock before/after of batch verification and
/// zero-copy decode (evidence layer, D-P9). Signs a deterministic batch
/// of [`BATCH_K`] messages, times both verification paths over `reps`
/// repetitions, and times both decoders over the sample wire block.
pub fn measure(reps: usize) -> P1Measured {
    use aura_crypto::{ed25519_public_key, sign_raw};
    assert!(reps > 0, "at least one repetition");
    let wire = aura_codec::transfer::sample_wire();
    let mut seeds = [[0u8; 32]; BATCH_K];
    let mut messages: Vec<Vec<u8>> = Vec::with_capacity(BATCH_K);
    let mut signatures = [[0u8; 64]; BATCH_K];
    let mut publics = [[0u8; 32]; BATCH_K];
    for i in 0..BATCH_K {
        for (j, b) in seeds[i].iter_mut().enumerate() {
            *b = (0xA0 + i as u8) ^ (j as u8);
        }
        messages.push(format!("p1-measure-{i}").into_bytes());
        publics[i] = ed25519_public_key(&seeds[i]);
        signatures[i] = sign_raw(&seeds[i], &messages[i]);
    }
    let items: Vec<BatchItem<'_>> = (0..BATCH_K)
        .map(|i| BatchItem {
            public: &publics[i],
            message: &messages[i],
            signature: &signatures[i],
        })
        .collect();

    // Sequential verify: BATCH_K single verifications per repetition.
    let mut seq_samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = std::time::Instant::now();
        for item in &items {
            let _ = verify_raw(item.public, item.message, item.signature);
        }
        seq_samples.push(start.elapsed().as_nanos() as u64 / BATCH_K as u64);
    }
    // Batch verify: one batch equation per repetition.
    let mut batch_samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = std::time::Instant::now();
        let _ = verify_batch_raw(&items);
        batch_samples.push(start.elapsed().as_nanos() as u64 / BATCH_K as u64);
    }

    let mut copy_samples = Vec::with_capacity(reps);
    let mut zc_samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = decode_copy(&wire);
        }
        copy_samples.push(start.elapsed().as_nanos() as u64 / 1000);
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = decode_zero_copy(&wire);
        }
        zc_samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }

    let seq = median_u64(&mut seq_samples);
    let batch = median_u64(&mut batch_samples);
    let copy = median_u64(&mut copy_samples);
    let zc = median_u64(&mut zc_samples);
    P1Measured {
        verify_seq_ns: seq,
        verify_batch_ns: batch,
        verify_gain_pct: seq
            .checked_sub(batch)
            .map(|d| d * 100 / seq.max(1))
            .unwrap_or(0),
        decode_copy_ns: copy,
        decode_zc_ns: zc,
        decode_gain_pct: copy
            .checked_sub(zc)
            .map(|d| d * 100 / copy.max(1))
            .unwrap_or(0),
    }
}
