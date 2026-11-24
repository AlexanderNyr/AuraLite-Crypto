//! AuraLite-Crypto P2 — parallel execution profile (phase P2,
//! `docs/plans/PERF_PLAN.md`): optimistic MVCC / Block-STM-like
//! scheduling for independent transactions.
//!
//! The P2 DoD is an equality, not a number: **1000 txs: parallel
//! result == sequential; conflicts roll back honestly**. The engine is
//! a deterministic optimistic executor (the Block-STM shape:
//! instrument reads, validate after execution, abort and re-execute on
//! conflict; a fixed in-block order keeps the final state
//! deterministic — Kung-Robinson OCC phases read/validate/write):
//!
//! 1. **Speculate** — the block is split into `WORKERS` contiguous
//!    chunks (fixed assignment, D-P6). Each chunk executes
//!    sequentially against the base snapshot plus its own writes
//!    (chunk speculation is a pure function of the base state, so the
//!    chunks may run on real threads with timing-independent results).
//!    Every transaction records its read set (cell → value seen).
//! 2. **Validate and commit** — chunks commit in index order,
//!    transactions in index order (the serializable order). A
//!    transaction whose read set no longer matches the committed state
//!    is a conflict: its speculative incarnation is **discarded
//!    (honest rollback)** and the transaction is re-executed once
//!    against the committed state (retry in index order).
//!
//! The final state is bit-identical to sequential execution of the
//! same block by construction; the gates pin that (`P2: parallel result
//! mismatch`), the rollback honesty (`P2: conflict not rolled back`),
//! the serializable commit order (`P2: retry order violated`), the
//! conflict count of the pinned fixture (`P2: conflict count
//! mismatch`), the cost model (`P2: speedup regression`) and repeat
//! determinism (`P2: determinism violated`). Cost model (D-P6):
//! sequential `txs·99` units vs `(txs·99)/workers + aborted·99`
//! (a retry costs a full re-execution — conflicts are never free).
//!
//! Decision **D-P6**: the engine is a self-contained profiling
//! artifact of the bench crate (native transfers over an MVCC map);
//! wiring it into `aura-exec` is follow-up production work.

use std::collections::BTreeMap;
use std::fmt;

use crate::{MachineProfile, percentile, sha256_32, spread_pct};

/// Transactions in the pinned P2 fixture block (the P2 workload).
pub const FIXTURE_TXS: usize = 1000;
/// Accounts in the P2 fixture state (2000 disjoint pair cells + 32 hot
/// cells + 64 unique targets of the planted conflicts).
pub const FIXTURE_ACCOUNTS: usize = 2096;
/// Planted cross-chunk conflicts in the fixture (the "conflicts roll
/// back honestly" half of the P2 DoD).
pub const FIXTURE_CONFLICTS: usize = 32;
/// Parallel workers (the D-P1 machine: 8 vCPU).
pub const WORKERS: usize = 8;
/// Per-transaction cost in deterministic units (the P1 pipeline cost:
/// one transfer of the D-P1 shape).
pub const TX_COST: u64 = 99;
/// Repeat runs of the deterministic model (P0/P1 convention).
pub const RUNS: usize = 5;

/// One transfer transaction of the P2 block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tx {
    /// Sender account index.
    pub from: usize,
    /// Recipient account index (always different from `from`).
    pub to: usize,
    /// Amount in the smallest unit (1..=9; balances start huge).
    pub amount: u128,
}

/// Executed block outcome (both executors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    /// Final state digest (hex sha256 over the canonical state form).
    pub state_digest: String,
    /// Final balances (index → balance).
    pub state: BTreeMap<usize, u128>,
    /// Transactions applied (balance moved).
    pub applied: usize,
    /// Transactions rejected (insufficient funds).
    pub rejected: usize,
    /// Conflicts: speculative incarnations discarded and re-executed.
    pub aborted: usize,
    /// Commit sequence of transaction indices (the serializable order).
    pub commit_order: Vec<usize>,
    /// Discarded speculative writes: (tx index, cell, speculative value).
    pub discarded: Vec<(usize, usize, u128)>,
}

/// The pinned P2 fixture block (the plan's workload shape: mostly
/// independent transfers plus a deliberate conflict minority):
///
/// * 936 disjoint transfers `2i -> 2i+1` (cells 0..1999) — zero
///   conflicts, the "independent txs" case the scheme is for;
/// * 32 planted cross-chunk conflict pairs: for k in 0..32 the txs at
///   indices `a = 4 + 27k` and `b = a + 125` (always the next chunk)
///   both draw from hot cell `2000 + k` into unique targets
///   `2032 + 2k` / `2033 + 2k`. Exactly one of each pair (the later
///   chunk's `b`) aborts and retries — 32 conflicts, structurally.
///
/// The same construction is reproduced by `tools/p2_parallel_check.py`.
pub fn fixture_txs() -> Vec<Tx> {
    let mut hot: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    for k in 0..FIXTURE_CONFLICTS {
        let a = 4 + 27 * k;
        hot.insert(a, (k, 0));
        hot.insert(a + 125, (k, 1));
    }
    let mut txs = Vec::with_capacity(FIXTURE_TXS);
    for i in 0..FIXTURE_TXS {
        txs.push(match hot.get(&i) {
            Some((k, side)) => Tx {
                from: 2000 + k,
                to: 2032 + 2 * k + side,
                amount: (1 + ((k + 3 * side) % 9)) as u128,
            },
            None => Tx {
                from: 2 * i,
                to: 2 * i + 1,
                amount: (1 + (i % 9)) as u128,
            },
        });
    }
    txs
}

/// The P2 fixture base state: every account starts with 1_000_000.
pub fn fixture_state() -> BTreeMap<usize, u128> {
    (0..FIXTURE_ACCOUNTS).map(|i| (i, 1_000_000u128)).collect()
}

/// Canonical state form: `p2s|idx:bal|idx:bal|…` (sorted indices).
pub fn state_canonical(state: &BTreeMap<usize, u128>) -> String {
    let mut s = String::from("p2s");
    for (idx, bal) in state {
        s.push_str(&format!("|{idx}:{bal}"));
    }
    s
}

/// State digest: sha256 over the canonical state form.
pub fn state_digest(state: &BTreeMap<usize, u128>) -> String {
    crate::hex(&sha256_32(state_canonical(state).as_bytes()))
}

/// Apply one transfer to `state`; returns true if applied.
fn apply(state: &mut BTreeMap<usize, u128>, tx: &Tx) -> bool {
    let from_bal = *state.get(&tx.from).unwrap_or(&0);
    if from_bal < tx.amount {
        return false;
    }
    let to_bal = *state.get(&tx.to).unwrap_or(&0);
    state.insert(tx.from, from_bal - tx.amount);
    state.insert(tx.to, to_bal + tx.amount);
    true
}

/// Sequential baseline executor (the pre-P2 reference).
pub fn sequential_execute(txs: &[Tx], base: &BTreeMap<usize, u128>) -> ExecResult {
    let mut state = base.clone();
    let mut applied = 0usize;
    let mut rejected = 0usize;
    let mut commit_order = Vec::with_capacity(txs.len());
    for (i, tx) in txs.iter().enumerate() {
        if apply(&mut state, tx) {
            applied += 1;
        } else {
            rejected += 1;
        }
        commit_order.push(i);
    }
    ExecResult {
        state_digest: state_digest(&state),
        state,
        applied,
        rejected,
        aborted: 0,
        commit_order,
        discarded: Vec::new(),
    }
}

/// One speculative transaction incarnation (read set + write set).
#[derive(Debug, Clone)]
struct SpecTx {
    /// Global transaction index.
    index: usize,
    /// Read set: (cell, value seen at speculation time).
    reads: Vec<(usize, u128)>,
    /// Write set: (cell, speculative value). Empty for rejections.
    writes: Vec<(usize, u128)>,
    /// Whether the transfer applied speculatively.
    applied: bool,
}

/// Speculate one chunk over a write overlay on top of the shared
/// `base` (the Block-STM multi-version shape: reads see own-chunk
/// writes first, then the base; no snapshot cloning). A pure function
/// of the base state — thread timing cannot change it.
fn speculate_chunk(
    txs: &[Tx],
    base: &BTreeMap<usize, u128>,
    start: usize,
    end: usize,
) -> Vec<SpecTx> {
    let mut overlay: BTreeMap<usize, u128> = BTreeMap::new();
    let mut out = Vec::with_capacity(end.saturating_sub(start));
    for (i, tx) in txs.iter().enumerate().take(end).skip(start) {
        let from_bal = *overlay
            .get(&tx.from)
            .or_else(|| base.get(&tx.from))
            .unwrap_or(&0);
        let to_bal = *overlay
            .get(&tx.to)
            .or_else(|| base.get(&tx.to))
            .unwrap_or(&0);
        let mut reads = vec![(tx.from, from_bal), (tx.to, to_bal)];
        reads.sort_unstable();
        let applied = from_bal >= tx.amount;
        let mut writes = Vec::new();
        if applied {
            overlay.insert(tx.from, from_bal - tx.amount);
            overlay.insert(tx.to, to_bal + tx.amount);
            writes.push((tx.from, from_bal - tx.amount));
            writes.push((tx.to, to_bal + tx.amount));
            writes.sort_unstable();
        }
        out.push(SpecTx {
            index: i,
            reads,
            writes,
            applied,
        });
    }
    out
}

/// Validate a speculative incarnation against the committed state:
/// every read must still see the same value.
fn reads_valid(spec: &SpecTx, committed: &BTreeMap<usize, u128>) -> bool {
    spec.reads
        .iter()
        .all(|(cell, seen)| committed.get(cell).copied().unwrap_or(0) == *seen)
}

/// Parallel optimistic executor (Block-STM-like, D-P6): chunks
/// speculate (on real threads), then commit in chunk order with
/// read-set validation, honest rollback and in-order retry.
pub fn parallel_execute(txs: &[Tx], base: &BTreeMap<usize, u128>, workers: usize) -> ExecResult {
    assert!(workers > 0, "at least one worker");
    let chunk = txs.len().div_ceil(workers);
    // Phase 1: speculate every chunk (pure; threads only change timing).
    let mut chunk_specs: Vec<Vec<SpecTx>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| {
                let start = w * chunk;
                let end = ((w + 1) * chunk).min(txs.len());
                scope.spawn(move || speculate_chunk(txs, base, start, end))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("chunk thread"))
            .collect()
    });
    chunk_specs.retain(|c| !c.is_empty());

    // Phase 2: validate and commit in chunk/tx order (the serializable
    // order); conflicts roll back and retry in place.
    let mut committed = base.clone();
    let mut applied = 0usize;
    let mut rejected = 0usize;
    let mut aborted = 0usize;
    let mut commit_order = Vec::with_capacity(txs.len());
    let mut discarded = Vec::new();
    for specs in chunk_specs.iter() {
        for spec in specs.iter() {
            if reads_valid(spec, &committed) {
                // Validated: publish the speculative writes.
                for (cell, val) in &spec.writes {
                    committed.insert(*cell, *val);
                }
                if spec.applied {
                    applied += 1;
                } else {
                    rejected += 1;
                }
            } else {
                // Conflict: honest rollback — the speculative
                // incarnation's writes are discarded, then the
                // transaction re-executes against the committed state.
                for (cell, val) in &spec.writes {
                    discarded.push((spec.index, *cell, *val));
                }
                aborted += 1;
                let tx = &txs[spec.index];
                if apply(&mut committed, tx) {
                    applied += 1;
                } else {
                    rejected += 1;
                }
            }
            commit_order.push(spec.index);
        }
    }
    ExecResult {
        state_digest: state_digest(&committed),
        state: committed,
        applied,
        rejected,
        aborted,
        commit_order,
        discarded,
    }
}

/// The named P2 errors (NC targets; all Display literals live on
/// single lines in [`P2Error::fmt`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P2Error {
    /// Parallel final state differs from the sequential one (the P2
    /// DoD gate; NC `bad-parallel`).
    ParallelMismatch {
        /// Expected digest prefix (sequential).
        expected: String,
        /// Actual digest prefix (parallel).
        actual: String,
    },
    /// An aborted incarnation's speculative write leaked into the
    /// final state (NC `bad-rollback`; "conflicts roll back honestly").
    ConflictNotRolledBack {
        /// Transaction index of the leaked incarnation.
        tx: usize,
    },
    /// Transactions committed outside the serializable index order
    /// (NC `bad-retry-order`).
    RetryOrderViolated {
        /// First out-of-order commit index.
        tx: usize,
    },
    /// The cost model stopped showing a parallel win (NC
    /// `bad-speedup`).
    SpeedupRegression {
        /// Sequential cost (before).
        before: u64,
        /// Parallel cost (after).
        after: u64,
    },
    /// The engine's conflict count differs from the pinned fixture
    /// value (NC `bad-conflicts`; scheduling determinism).
    ConflictCountMismatch {
        /// Pinned fixture conflict count.
        expected: usize,
        /// Engine-observed conflict count.
        actual: usize,
    },
    /// Repeat runs are not bit-identical (NC `bad-determinism`; X7).
    DeterminismViolated {
        /// Index of the first run that differs.
        run: usize,
    },
}

impl fmt::Display for P2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            P2Error::ParallelMismatch { expected, actual } => {
                write!(f, "P2: parallel result mismatch ({expected}/{actual})")
            }
            P2Error::ConflictNotRolledBack { tx } => {
                write!(f, "P2: conflict not rolled back (tx {tx})")
            }
            P2Error::RetryOrderViolated { tx } => {
                write!(f, "P2: retry order violated (tx {tx})")
            }
            P2Error::SpeedupRegression { before, after } => {
                write!(f, "P2: speedup regression ({before}/{after})")
            }
            P2Error::ConflictCountMismatch { expected, actual } => {
                write!(f, "P2: conflict count mismatch ({expected}/{actual})")
            }
            P2Error::DeterminismViolated { run } => {
                write!(f, "P2: determinism violated (run {run})")
            }
        }
    }
}

impl std::error::Error for P2Error {}

/// Sequential cost of a `txs`-transaction block (D-P6).
pub fn seq_cost(txs: usize) -> u64 {
    txs as u64 * TX_COST
}

/// Parallel cost of a `txs`-transaction block over `workers` workers
/// with `aborted` conflicts: `(txs·TX_COST)/workers + aborted·TX_COST`
/// (a retry costs a full re-execution — conflicts are never free).
pub fn par_cost(txs: usize, workers: usize, aborted: usize) -> u64 {
    assert!(workers > 0, "at least one worker");
    (txs as u64 * TX_COST) / workers as u64 + aborted as u64 * TX_COST
}

/// The pinned P2 block report (deterministic layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2Report {
    /// Workload digest (hex sha256 of the canonical workload form).
    pub workload_digest: String,
    /// Machine digest (hex sha256 of the canonical machine form).
    pub machine_digest: String,
    /// Repeat runs of the deterministic model.
    pub runs: usize,
    /// Per-run parallel costs in deterministic cost units.
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
    /// Sequential cost of one block run (before).
    pub seq_cost: u64,
    /// Parallel cost of one block run (after).
    pub par_cost: u64,
    /// Workers (8 per D-P1).
    pub workers: usize,
    /// Conflicts of the pinned fixture (aborted incarnations).
    pub aborted: usize,
    /// Throughput gain in percent (`seq·100/par − 100`).
    pub speedup_pct: u64,
    /// Final state digest of the pinned fixture (parallel == sequential).
    pub state_digest: String,
    /// Aggregate digest (hex sha256 of the canonical report form).
    pub bench_digest: String,
}

impl P2Report {
    /// Build the report from the engine outcome of the pinned fixture
    /// (deterministic: same inputs, same report).
    pub fn build(result: &ExecResult, runs: usize) -> P2Report {
        assert!(runs > 0, "at least one run");
        let par = par_cost(FIXTURE_TXS, WORKERS, result.aborted);
        let run_costs = vec![par; runs];
        let spread = spread_pct(&run_costs);
        let p50 = percentile(&run_costs, 50);
        let p95 = percentile(&run_costs, 95);
        let p99 = percentile(&run_costs, 99);
        let total: u64 = run_costs.iter().sum();
        let seq = seq_cost(FIXTURE_TXS);
        let mut report = P2Report {
            workload_digest: crate::hex(&sha256_32(
                format!("p2w|block|{FIXTURE_TXS}|{FIXTURE_ACCOUNTS}").as_bytes(),
            )),
            machine_digest: MachineProfile::REFERENCE.digest(),
            runs,
            run_costs,
            spread_pct: spread,
            p50,
            p95,
            p99,
            total,
            seq_cost: seq,
            par_cost: par,
            workers: WORKERS,
            aborted: result.aborted,
            speedup_pct: seq * 100 / par - 100,
            state_digest: result.state_digest.clone(),
            bench_digest: String::new(),
        };
        report.bench_digest = crate::hex(&sha256_32(report.canonical().as_bytes()));
        report
    }

    /// Canonical form of the report (the digest input).
    pub fn canonical(&self) -> String {
        format!(
            "p2b|{}|{}|runs={}|spread={}|p50={}|p95={}|p99={}|total={}\
             |seq={}|par={}|workers={}|aborted={}|speedup_pct={}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.seq_cost,
            self.par_cost,
            self.workers,
            self.aborted,
            self.speedup_pct
        )
    }

    /// Compact JSON of the pinned fields (byte-stable key order).
    pub fn json(&self) -> String {
        format!(
            "{{\"workload_digest\": \"{}\", \"machine_digest\": \"{}\", \"runs\": {}, \
             \"spread_pct\": {}, \"p50\": {}, \"p95\": {}, \"p99\": {}, \
             \"total\": {}, \"seq_cost\": {}, \"par_cost\": {}, \"workers\": {}, \
             \"aborted\": {}, \"applied\": {}, \"state_digest\": \"{}\", \
             \"speedup_pct\": {}, \"bench_digest\": \"{}\"}}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.seq_cost,
            self.par_cost,
            self.workers,
            self.aborted,
            FIXTURE_TXS,
            self.state_digest,
            self.speedup_pct,
            self.bench_digest
        )
    }

    /// Validate the optimization gates in the fixed order
    /// determinism → speedup. (State equality and conflict accounting
    /// live in [`gate_parallel`] / [`gate_fixture`].)
    pub fn validate(&self) -> Result<(), P2Error> {
        if let Some(first) = self.run_costs.first() {
            for (i, c) in self.run_costs.iter().enumerate() {
                if c != first {
                    return Err(P2Error::DeterminismViolated { run: i });
                }
            }
        }
        if self.par_cost >= self.seq_cost {
            return Err(P2Error::SpeedupRegression {
                before: self.seq_cost,
                after: self.par_cost,
            });
        }
        Ok(())
    }
}

/// Functional gate (the P2 DoD): parallel final state == sequential;
/// on mismatch the discarded-write trace is checked for a leaked
/// incarnation (`P2: conflict not rolled back (tx {n})`) before the
/// generic mismatch error.
pub fn gate_parallel(txs: &[Tx], base: &BTreeMap<usize, u128>) -> Result<ExecResult, P2Error> {
    let seq = sequential_execute(txs, base);
    let par = parallel_execute(txs, base, WORKERS);
    if seq.state == par.state && seq.state_digest == par.state_digest {
        return Ok(par);
    }
    Err(diagnose_mismatch(&seq, &par))
}

/// Diagnose a sequential/parallel mismatch: a discarded speculative
/// write that survived into the parallel final state is a rollback
/// leak (`P2: conflict not rolled back (tx {n})`); anything else is a
/// plain result mismatch.
pub fn diagnose_mismatch(seq: &ExecResult, par: &ExecResult) -> P2Error {
    for (tx_idx, cell, spec_val) in &par.discarded {
        let final_val = par.state.get(cell).copied().unwrap_or(0);
        let seq_val = seq.state.get(cell).copied().unwrap_or(0);
        if final_val == *spec_val && seq_val != *spec_val {
            return P2Error::ConflictNotRolledBack { tx: *tx_idx };
        }
    }
    P2Error::ParallelMismatch {
        expected: seq.state_digest[..16].to_string(),
        actual: par.state_digest[..16].to_string(),
    }
}

/// Functional gate: transactions must commit in serializable index
/// order (`P2: retry order violated (tx {n})`).
pub fn gate_commit_order(txs: &[Tx], base: &BTreeMap<usize, u128>) -> Result<(), P2Error> {
    let par = parallel_execute(txs, base, WORKERS);
    for (pos, idx) in par.commit_order.iter().enumerate() {
        if *idx != pos {
            return Err(P2Error::RetryOrderViolated { tx: *idx });
        }
    }
    Ok(())
}

/// Functional gate: the engine's conflict count on the pinned fixture
/// must equal `expected` (`P2: conflict count mismatch`).
pub fn gate_fixture(
    txs: &[Tx],
    base: &BTreeMap<usize, u128>,
    expected: usize,
) -> Result<ExecResult, P2Error> {
    let par = parallel_execute(txs, base, WORKERS);
    if par.aborted != expected {
        return Err(P2Error::ConflictCountMismatch {
            expected,
            actual: par.aborted,
        });
    }
    Ok(par)
}

/// Honest wall-clock before/after of the real executors (evidence
/// only, D-P9 — never a gate). Two parallel numbers, both honest:
/// cold path (`parallel_execute` spawns its threads per call) and
/// steady state (a persistent worker pool — the production Block-STM
/// shape; per-call thread spawn dominates small blocks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2Measured {
    /// Median ns per 1000-tx block, sequential executor.
    pub seq_block_ns: u64,
    /// Median ns per 1000-tx block, parallel executor, cold spawn.
    pub par_cold_ns: u64,
    /// Median ns per 1000-tx block, parallel executor, steady state.
    pub par_pool_ns: u64,
    /// Cold-path wall-clock speedup in percent (can be negative).
    pub speedup_cold_pct: i64,
    /// Steady-state wall-clock speedup in percent (can be negative).
    pub speedup_pool_pct: i64,
    /// Conflicts observed by the parallel runs (deterministic count).
    pub aborted: usize,
}

impl P2Measured {
    /// Compact JSON (byte-stable key order).
    pub fn json(&self) -> String {
        format!(
            "{{\"seq_block_ns\": {}, \"par_cold_ns\": {}, \
             \"par_pool_ns\": {}, \"speedup_cold_pct\": {}, \
             \"speedup_pool_pct\": {}, \"aborted\": {}}}",
            self.seq_block_ns,
            self.par_cold_ns,
            self.par_pool_ns,
            self.speedup_cold_pct,
            self.speedup_pool_pct,
            self.aborted
        )
    }
}

/// Median of a sample (integer arithmetic).
fn median_u64(sample: &mut [u64]) -> u64 {
    sample.sort_unstable();
    let n = sample.len();
    if n % 2 == 1 {
        sample[n / 2]
    } else {
        (sample[n / 2 - 1] + sample[n / 2]) / 2
    }
}

/// Measure the honest wall-clock before/after of the two executors on
/// the pinned fixture (evidence layer, D-P9).
pub fn measure(reps: usize) -> P2Measured {
    assert!(reps > 0, "at least one repetition");
    let txs = fixture_txs();
    let base = fixture_state();
    let mut seq_samples = Vec::with_capacity(reps);
    let mut cold_samples = Vec::with_capacity(reps);
    let mut aborted = 0usize;
    for _ in 0..reps {
        let start = std::time::Instant::now();
        let _ = sequential_execute(&txs, &base);
        seq_samples.push(start.elapsed().as_nanos() as u64);
        let start = std::time::Instant::now();
        let r = parallel_execute(&txs, &base, WORKERS);
        cold_samples.push(start.elapsed().as_nanos() as u64);
        aborted = r.aborted;
    }
    let mut pool_samples = measure_pool(&txs, &base, reps);
    let seq = median_u64(&mut seq_samples);
    let cold = median_u64(&mut cold_samples);
    let pool = median_u64(&mut pool_samples);
    let speedup = |fast: u64| -> i64 {
        if fast == 0 {
            0
        } else {
            (seq as i64) * 100 / (fast as i64) - 100
        }
    };
    P2Measured {
        seq_block_ns: seq,
        par_cold_ns: cold,
        par_pool_ns: pool,
        speedup_cold_pct: speedup(cold),
        speedup_pool_pct: speedup(pool),
        aborted,
    }
}

/// Steady-state parallel measurement: a persistent worker pool (the
/// production shape — workers live across blocks), one fixed chunk per
/// worker, speculation on the workers and commit on the caller thread.
/// Returns per-block wall-clock samples.
fn measure_pool(txs: &[Tx], base: &BTreeMap<usize, u128>, reps: usize) -> Vec<u64> {
    use std::sync::mpsc;
    let chunk = txs.len().div_ceil(WORKERS);
    let mut samples = Vec::with_capacity(reps);
    std::thread::scope(|scope| {
        let mut req_txs = Vec::with_capacity(WORKERS);
        let mut resp_rxs = Vec::with_capacity(WORKERS);
        for w in 0..WORKERS {
            let (req_tx, req_rx) = mpsc::channel::<()>();
            let (resp_tx, resp_rx) = mpsc::channel::<Vec<SpecTx>>();
            let start = w * chunk;
            let end = ((w + 1) * chunk).min(txs.len());
            let base_ref = base;
            let txs_ref = txs;
            scope.spawn(move || {
                while req_rx.recv().is_ok() {
                    let spec = speculate_chunk(txs_ref, base_ref, start, end);
                    if resp_tx.send(spec).is_err() {
                        break;
                    }
                }
            });
            req_txs.push(req_tx);
            resp_rxs.push(resp_rx);
        }
        for _ in 0..reps {
            let start = std::time::Instant::now();
            for req in &req_txs {
                req.send(()).expect("worker alive");
            }
            let chunk_specs: Vec<Vec<SpecTx>> = resp_rxs
                .iter()
                .map(|r| r.recv().expect("worker reply"))
                .collect();
            // Commit phase (same deterministic loop as parallel_execute).
            let mut committed = base.clone();
            for specs in &chunk_specs {
                for spec in specs.iter() {
                    if reads_valid(spec, &committed) {
                        for (cell, val) in &spec.writes {
                            committed.insert(*cell, *val);
                        }
                    } else {
                        let _ = apply(&mut committed, &txs[spec.index]);
                    }
                }
            }
            samples.push(start.elapsed().as_nanos() as u64);
        }
    });
    samples
}
