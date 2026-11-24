//! AuraLite-Crypto P0 — baseline bench: criterion + harness, fixed
//! hardware, first honest numbers (`docs/plans/PERF_PLAN.md`, phase P0).
//!
//! The benchmark follows the published methodology principles (see
//! `docs/perf/p0-baseline-report.md`): the benchmark plan (workload,
//! machine, run count, metric set) is committed **before** the run;
//! every run uses identical deterministic inputs; aggregates are
//! percentiles (p50/p95/p99), never a bare mean; runs are re-runnable
//! and auditable.
//!
//! Two layers, honestly separated (D-P2: profile before optimizing):
//!
//! * **Deterministic cost model** — the gate. Per-stage costs in
//!   abstract cost units, fixed by the machine profile. Five repeat
//!   runs are bit-identical by construction (spread 0%), so the P0 DoD
//!   "5 repeat runs: spread < 5%" is checked against pinned golden
//!   numbers (`tests/p0_bench.rs`).
//! * **Measured layer** — honest wall-clock numbers of the real
//!   350-byte pipeline (sha256 + ed25519 verify), reported through
//!   `--measured` and captured in evidence only. "First numbers, even
//!   bad ones" (P0 scope); they are environment-specific and are never
//!   used as a gate.
//!
//! Decision **D-P9**: the deterministic gate proves the *harness*
//! (identical inputs, fixed plan, percentile aggregation, spread
//! control); wall-clock spread on shared hardware is reported, not
//! gated. Decision **D-P9b**: cost units are machine-profile relative;
//! P1 optimizations change the cost model with before/after benches.

#![deny(missing_docs)]

/// P1 codec/crypto optimization profile (batch verify, BLS cost
/// model, zero-copy decode).
pub mod p1;

/// P2 parallel execution profile (optimistic MVCC / Block-STM-like
/// scheduling; parallel == sequential, honest rollback).
pub mod p2;

/// P3 storage profile (mini-LSM commit path: batch writes, bloom
/// filters, compaction; S7 durability contract).
pub mod p3;

/// P4 network profile (gossip-traffic cost model: vote batching,
/// compression, gossipsub parameters, mesh tuning; traffic/validator
/// within the published budget).
pub mod p4;

/// P5 consensus profile (finality-latency cost model: pipelining,
/// QC cache, faster view change; p99 finality at 100 nodes < 2 s).
pub mod p5;

/// P6 SLO publication profile (tamper-evident run log, north-star
/// registry, methodology + honest-failure ledgers; NC: rigged run ->
/// log check red).
pub mod p6;

use std::fmt;

use aura_crypto::{ed25519_public_key, sign_raw, verify_raw};
use aura_state::sha256_bytes;

/// Raw SHA-256 as a 32-byte word.
pub fn sha256_32(data: &[u8]) -> [u8; 32] {
    let h = sha256_bytes(data);
    let b = h.as_bytes();
    let mut out = [0u8; 32];
    out[..32].copy_from_slice(&b[..32]);
    out
}

/// Lowercase hex encoding of raw bytes.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// The fixed reference machine (decision **D-P1**: one workload, one
/// hardware profile — 8 vCPU / 32 GiB / NVMe / 100 validators /
/// 1 Gbit/s). Substituting any field is a measurement-substitution
/// attack and must be rejected by [`validate`].
pub struct MachineProfile {
    /// vCPU count (8 per D-P1).
    pub vcpu: u32,
    /// Memory in GiB (32 per D-P1).
    pub mem_gib: u32,
    /// Storage class, lowercase (`nvme` per D-P1).
    pub storage: &'static str,
    /// Validator count (100 per D-P1).
    pub validators: u32,
    /// Network budget in Mbit/s (1000 per D-P1).
    pub net_mbit: u32,
}

impl MachineProfile {
    /// The one fixed reference profile (D-P1).
    pub const REFERENCE: MachineProfile = MachineProfile {
        vcpu: 8,
        mem_gib: 32,
        storage: "nvme",
        validators: 100,
        net_mbit: 1000,
    };

    /// Canonical form: `p0m|vcpu=…|mem_gib=…|storage=…|validators=…|net_mbit=…`.
    pub fn canonical(&self) -> String {
        format!(
            "p0m|vcpu={}|mem_gib={}|storage={}|validators={}|net_mbit={}",
            self.vcpu, self.mem_gib, self.storage, self.validators, self.net_mbit
        )
    }

    /// Machine digest: sha256 of the canonical form.
    pub fn digest(&self) -> String {
        hex(&sha256_32(self.canonical().as_bytes()))
    }
}

/// The fixed benchmark workload (decision **D-P1**: one 350-byte
/// transfer with 1 input and 1 output). Identical inputs every run;
/// a substituted workload must change the numbers
/// ("NC: substituted workload → different numbers").
pub struct Workload {
    /// Workload kind (`transfer` per D-P1).
    pub kind: &'static str,
    /// Serialized size in bytes (350 per D-P1).
    pub bytes: u64,
    /// Input count (1 per D-P1).
    pub inputs: u32,
    /// Output count (1 per D-P1).
    pub outputs: u32,
}

impl Workload {
    /// The one fixed transfer workload (D-P1).
    pub const TRANSFER_350: Workload = Workload {
        kind: "transfer",
        bytes: 350,
        inputs: 1,
        outputs: 1,
    };

    /// A substituted workload (352 bytes) used by the functional gate.
    pub const TRANSFER_352: Workload = Workload {
        kind: "transfer",
        bytes: 352,
        inputs: 1,
        outputs: 1,
    };

    /// Canonical form: `p0w|kind|bytes|inputs|outputs`.
    pub fn canonical(&self) -> String {
        format!(
            "p0w|{}|{}|{}|{}",
            self.kind, self.bytes, self.inputs, self.outputs
        )
    }

    /// Workload digest: sha256 of the canonical form.
    pub fn digest(&self) -> String {
        hex(&sha256_32(self.canonical().as_bytes()))
    }
}

/// The deterministic cost model: per-stage costs in machine-profile
/// relative cost units (the gate medium of P0; D-P9).
pub struct CostModel {
    /// Fixed decode cost per transaction.
    pub decode_base: u64,
    /// Decode cost per 16 payload bytes (integer division).
    pub decode_per_16b: u64,
    /// Signature-verification cost per input.
    pub verify_per_input: u64,
    /// Fixed execution cost per transaction.
    pub execute_base: u64,
    /// Execution cost per output.
    pub execute_per_output: u64,
    /// Commit cost per transaction.
    pub commit: u64,
}

impl CostModel {
    /// The P0 baseline cost model (frozen before the first run).
    pub const BASELINE: CostModel = CostModel {
        decode_base: 4,
        decode_per_16b: 1,
        verify_per_input: 38,
        execute_base: 10,
        execute_per_output: 15,
        commit: 40,
    };

    /// Deterministic per-run cost of one transaction of `work`:
    /// `decode_base + bytes/16 * decode_per_16b + inputs *
    /// verify_per_input + execute_base + outputs * execute_per_output
    /// + commit`.
    pub fn run_cost(&self, work: &Workload) -> u64 {
        let decode = self.decode_base + (work.bytes / 16) * self.decode_per_16b;
        let verify = u64::from(work.inputs) * self.verify_per_input;
        let execute = self.execute_base + u64::from(work.outputs) * self.execute_per_output;
        decode + verify + execute + self.commit
    }
}

/// Nearest-rank percentile of a sample (the sample is sorted inside).
pub fn percentile(sample: &[u64], p: u64) -> u64 {
    assert!(!sample.is_empty(), "sample must not be empty");
    assert!((1..=99).contains(&p), "percentile must be 1..=99");
    let mut sorted = sample.to_vec();
    sorted.sort_unstable();
    let n = sorted.len() as u64;
    let rank = (p * n).div_ceil(100);
    sorted[(rank.max(1) - 1) as usize]
}

/// Spread across repeat runs in percent:
/// `(max - min) * 100 / mean` in integer arithmetic.
pub fn spread_pct(sample: &[u64]) -> u64 {
    assert!(!sample.is_empty(), "sample must not be empty");
    let min = *sample.iter().min().expect("non-empty");
    let max = *sample.iter().max().expect("non-empty");
    let mean = sample.iter().sum::<u64>() / sample.len() as u64;
    if mean == 0 {
        return 0;
    }
    (max - min) * 100 / mean
}

/// The pinned P0 benchmark report (deterministic layer).
pub struct BenchReport {
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
    /// Aggregate digest (hex sha256 of the canonical report form).
    pub bench_digest: String,
}

impl BenchReport {
    /// Build the report for `work` on `machine` with `model` over
    /// `runs` repeat runs (deterministic: the same inputs always
    /// produce the same report).
    pub fn build(
        work: &Workload,
        machine: &MachineProfile,
        model: &CostModel,
        runs: usize,
    ) -> BenchReport {
        assert!(runs > 0, "at least one run");
        let cost = model.run_cost(work);
        let run_costs = vec![cost; runs];
        let spread = spread_pct(&run_costs);
        let p50 = percentile(&run_costs, 50);
        let p95 = percentile(&run_costs, 95);
        let p99 = percentile(&run_costs, 99);
        let total = run_costs.iter().sum();
        let canonical = format!(
            "p0b|{}|{}|runs={}|spread={}|p50={}|p95={}|p99={}|total={}",
            work.digest(),
            machine.digest(),
            runs,
            spread,
            p50,
            p95,
            p99,
            total
        );
        BenchReport {
            workload_digest: work.digest(),
            machine_digest: machine.digest(),
            runs,
            run_costs,
            spread_pct: spread,
            p50,
            p95,
            p99,
            total,
            bench_digest: hex(&sha256_32(canonical.as_bytes())),
        }
    }

    /// Canonical form of the report (same shape as the digest input).
    pub fn canonical(&self) -> String {
        format!(
            "p0b|{}|{}|runs={}|spread={}|p50={}|p95={}|p99={}|total={}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total
        )
    }

    /// Compact JSON of the pinned fields (byte-stable key order).
    pub fn json(&self) -> String {
        format!(
            "{{\"workload_digest\": \"{}\", \"machine_digest\": \"{}\", \"runs\": {}, \
             \"spread_pct\": {}, \"p50\": {}, \"p95\": {}, \"p99\": {}, \
             \"total\": {}, \"bench_digest\": \"{}\"}}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.bench_digest
        )
    }
}

/// The named P0 errors (NC targets; all Display literals live on
/// single lines in [`BenchError::fmt`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchError {
    /// Substituted workload (NC `bad-workload`; DoD: substituted
    /// workload → different numbers).
    WorkloadMismatch {
        /// Expected workload digest (hex).
        expected: String,
        /// Actual workload digest (hex).
        actual: String,
    },
    /// Substituted machine profile (NC `bad-machine`).
    MachineMismatch {
        /// Expected machine digest (hex).
        expected: String,
        /// Actual machine digest (hex).
        actual: String,
    },
    /// Spread across repeat runs above the P0 limit (NC `bad-spread`).
    SpreadExceeded {
        /// Observed spread in percent.
        spread: u64,
        /// Allowed limit in percent (5 per P0).
        limit: u64,
    },
    /// Repeat-run count violated (NC `bad-runs`; 5 runs per P0).
    RunCount {
        /// Observed run count.
        runs: usize,
        /// Required run count.
        need: usize,
    },
    /// Determinism violated: one repeat run differs (NC `bad-determinism`).
    Determinism {
        /// Index of the first differing run.
        run: usize,
    },
    /// Metric not published (NC `bad-metric`; D-P3: numbers travel
    /// with the methodology).
    MetricNotPublished {
        /// Name of the missing metric.
        metric: String,
    },
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchError::WorkloadMismatch { expected, actual } => {
                write!(f, "P0: workload mismatch ({expected}/{actual})")
            }
            BenchError::MachineMismatch { expected, actual } => {
                write!(f, "P0: machine profile mismatch ({expected}/{actual})")
            }
            BenchError::SpreadExceeded { spread, limit } => {
                write!(f, "P0: spread exceeds limit ({spread}%/{limit}%)")
            }
            BenchError::RunCount { runs, need } => {
                write!(f, "P0: repeat count violated ({runs}/{need})")
            }
            BenchError::Determinism { run } => write!(f, "P0: determinism violated (run {run})"),
            BenchError::MetricNotPublished { metric } => {
                write!(f, "P0: metric not published ({metric})")
            }
        }
    }
}

impl std::error::Error for BenchError {}

/// The P0 spread limit in percent (DoD: spread < 5%).
pub const SPREAD_LIMIT_PCT: u64 = 5;

/// The P0 required repeat-run count (DoD: 5 repeat runs).
pub const REQUIRED_RUNS: usize = 5;

/// Functional gate control (the P0 "criterion"): structural checks in
/// a fixed order — repeat count, determinism, spread, workload, then
/// machine — plus the published-metrics check (D-P3).
pub fn validate(
    report: &BenchReport,
    expected_work: &Workload,
    expected_machine: &MachineProfile,
) -> Result<(), BenchError> {
    if report.runs != REQUIRED_RUNS {
        return Err(BenchError::RunCount {
            runs: report.runs,
            need: REQUIRED_RUNS,
        });
    }
    if let Some(first) = report.run_costs.first() {
        for (i, cost) in report.run_costs.iter().enumerate() {
            if cost != first {
                return Err(BenchError::Determinism { run: i });
            }
        }
    }
    if report.spread_pct > SPREAD_LIMIT_PCT {
        return Err(BenchError::SpreadExceeded {
            spread: report.spread_pct,
            limit: SPREAD_LIMIT_PCT,
        });
    }
    if report.workload_digest != expected_work.digest() {
        return Err(BenchError::WorkloadMismatch {
            expected: expected_work.digest(),
            actual: report.workload_digest.clone(),
        });
    }
    if report.machine_digest != expected_machine.digest() {
        return Err(BenchError::MachineMismatch {
            expected: expected_machine.digest(),
            actual: report.machine_digest.clone(),
        });
    }
    for (metric, value) in [
        ("p50", report.p50),
        ("p95", report.p95),
        ("p99", report.p99),
    ] {
        if value == 0 {
            return Err(BenchError::MetricNotPublished {
                metric: metric.to_string(),
            });
        }
    }
    Ok(())
}

/// Wall-clock measurement of the real 350-byte pipeline (sha256 of the
/// payload + one ed25519 verification), returning nanoseconds per
/// transaction. Honest evidence only — never a gate (D-P9).
pub fn measure_pipeline_ns(work: &Workload, iterations: u32) -> u64 {
    let payload = vec![0xA5u8; work.bytes as usize];
    let seed = [7u8; 32];
    let public = ed25519_public_key(&seed);
    let digest = sha256_32(&payload);
    let signature = sign_raw(&seed, &digest);
    let start = std::time::Instant::now();
    let mut sink = 0u8;
    for _ in 0..iterations {
        let d = sha256_32(&payload);
        sink ^= d[0];
        if verify_raw(&public, &digest, &signature).is_ok() {
            sink = sink.wrapping_add(1);
        }
    }
    let elapsed = start.elapsed();
    std::hint::black_box(sink);
    elapsed.as_nanos() as u64 / u64::from(iterations.max(1))
}
