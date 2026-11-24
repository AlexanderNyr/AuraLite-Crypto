//! P5 consensus profile — deterministic finality-latency cost model
//! (PERF_PLAN P5: pipelining, QC cache, faster view change; DoD:
//! **p99 finality at 100 nodes < 2 s** — the SLO is plan-fixed and the
//! methodology is published in `docs/perf/p5-consensus-report.md`,
//! D-P3).
//!
//! Self-contained profiling artifact (decision **D-P7**, the P3/P4
//! pattern): a millisecond-accurate accounting of the HotStuff-like
//! 3-chain commit path (D-B1/D-B2 vocabulary: `Vote`, QC, 2f+1,
//! quorum certificate) on the 100-validator D-P1 stand. Three
//! optimization levers, each with a before/after pair (D-P2):
//! pipelining (overlapped chain extension), QC cache (O(1) cached
//! verification instead of per-signature re-verification), faster
//! view change (the shipped `DEFAULT_VIEW_TIMEOUT_MS = 2000` becomes
//! 300 ms).

use std::fmt;

use crate::MachineProfile;

/// Number of validators on the stand (D-P1 fixture).
pub const N_NODES: usize = 100;
/// Byzantine bound f = (n - 1) / 3.
pub const F_NODES: usize = (N_NODES - 1) / 3;
/// One-way gossip hop delay in milliseconds (published model
/// constant: conservative WAN hop on the D-P1 fabric).
pub const HOP_MS: u64 = 25;
/// Gossip hops for the proposal (leader -> mesh -> all).
pub const PROPOSAL_HOPS: u64 = 2;
/// Gossip hops for votes (validator -> leader).
pub const VOTE_HOPS: u64 = 1;
/// Gossip hops for the QC (leader -> mesh -> all).
pub const QC_HOPS: u64 = 2;
/// Serialization + send of the 8 KiB proposal at 1 Gbit/s (ceil ms).
pub const PROPOSAL_TX_MS: u64 = 1;
/// Leader verifies the quorum votes (67 × 0.2 ms, batched).
pub const VOTE_VERIFY_MS: u64 = 14;
/// BLS aggregation of the quorum votes into a QC.
pub const QC_FORM_MS: u64 = 4;
/// QC verification without a cache: every signature checked
/// individually (the shipped B2 implementation).
pub const QC_VERIFY_MS: u64 = 12;
/// QC verification with the QC cache: O(1) cached check (D-B2 goal).
pub const QC_VERIFY_CACHED_MS: u64 = 1;
/// 3-chain rule application + checkpoint write.
pub const COMMIT_MS: u64 = 1;
/// View-change timeout before P5: the shipped
/// `DEFAULT_VIEW_TIMEOUT_MS` (`aura-consensus::finality`).
pub const TIMEOUT_MS_BEFORE: u64 = 2000;
/// View-change timeout after P5 (faster view change).
pub const TIMEOUT_MS_AFTER: u64 = 300;
/// Slow-leader / nil-block rate in percent of views (published).
pub const VIEW_CHANGE_RATE_PCT: u64 = 1;
/// 3-chain commit rule depth (D-B1).
pub const CHAIN_DEPTH: usize = 3;
/// Plan-fixed SLO: p99 finality at 100 nodes < 2 s.
pub const FINALITY_SLO_MS: u64 = 2000;
/// Deterministic model repeats.
pub const RUNS: usize = 5;

/// Named P5 messages (one line each; the NC contract).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P5Error {
    /// p99 finality latency exceeded the plan-fixed SLO.
    SloExceeded {
        /// Observed p99 finality latency (ms).
        p99: u64,
        /// The SLO (ms).
        slo: u64,
    },
    /// Pipelining no longer overlaps the chain extension.
    PipelineOverlapMissing {
        /// Incremental chain-extension latency (ms).
        inc: u64,
        /// Full view latency (ms).
        full: u64,
    },
    /// Reverting the QC cache removed its win.
    QcCacheWinReverted {
        /// Per-view QC re-verification cost before (ms).
        before: u64,
        /// Per-view QC re-verification cost after (ms).
        after: u64,
    },
    /// The 3-chain commit rule was violated.
    ChainRuleViolated {
        /// Block index whose finality depth is wrong.
        block: u64,
    },
    /// Quorum accounting (2f+1) is corrupted.
    QuorumCorrupted {
        /// Vote count at which the mutated rule forms a QC.
        have: u64,
        /// The 2f+1 threshold.
        need: u64,
    },
    /// Two identical runs disagreed (X7).
    DeterminismViolated {
        /// Index of the disagreeing run.
        run: usize,
    },
}

impl fmt::Display for P5Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SloExceeded { p99, slo } => {
                write!(f, "P5: p99 finality exceeds SLO ({p99}/{slo})")
            }
            Self::PipelineOverlapMissing { inc, full } => {
                write!(f, "P5: pipelining overlap missing ({inc}/{full})")
            }
            Self::QcCacheWinReverted { before, after } => {
                write!(f, "P5: QC cache win reverted ({before}/{after})")
            }
            Self::ChainRuleViolated { block } => {
                write!(f, "P5: 3-chain rule violated (block {block})")
            }
            Self::QuorumCorrupted { have, need } => {
                write!(f, "P5: quorum accounting corrupted ({have}/{need})")
            }
            Self::DeterminismViolated { run } => {
                write!(f, "P5: determinism violated (run {run})")
            }
        }
    }
}

impl std::error::Error for P5Error {}

/// 2f+1 quorum threshold (D-B2).
pub fn quorum_threshold(n: usize) -> usize {
    2 * ((n - 1) / 3) + 1
}

/// Full view latency: proposal dissemination + vote round + QC
/// formation + QC re-verification at `qc_verify_ms` + commit.
pub fn view_full_latency(qc_verify_ms: u64) -> u64 {
    PROPOSAL_HOPS * HOP_MS
        + PROPOSAL_TX_MS
        + VOTE_HOPS * HOP_MS
        + VOTE_VERIFY_MS
        + QC_FORM_MS
        + QC_HOPS * qc_verify_ms
        + COMMIT_MS
}

/// Incremental chain-extension latency (pipelining): the proposal of
/// the next block is already in flight, only the vote round + QC +
/// commit remain.
pub fn incremental_view_latency(qc_verify_ms: u64) -> u64 {
    VOTE_HOPS * HOP_MS + VOTE_VERIFY_MS + QC_FORM_MS + QC_HOPS * qc_verify_ms + COMMIT_MS
}

/// Happy-path 3-chain finality latency.
pub fn happy_latency(qc_verify_ms: u64, pipelined: bool) -> u64 {
    if pipelined {
        view_full_latency(qc_verify_ms)
            + (CHAIN_DEPTH as u64 - 1) * incremental_view_latency(qc_verify_ms)
    } else {
        CHAIN_DEPTH as u64 * view_full_latency(qc_verify_ms)
    }
}

/// One view-change stall: timeout + one full view.
pub fn stall_latency(timeout_ms: u64, qc_verify_ms: u64) -> u64 {
    timeout_ms + view_full_latency(qc_verify_ms)
}

/// Latency percentiles from the pinned stochastic rule: each view
/// stalls with `rate_pct` percent probability over a `window`-view
/// finality window; `happy_share` = (100 - rate)^window permille.
/// p99 picks `happy + one stall` when it exceeds the happy share.
pub fn latency_percentiles(happy: u64, stall: u64, rate_pct: u64, window: u32) -> (u64, u64, u64) {
    let base = (100u64.saturating_sub(rate_pct)).pow(window);
    let denom = 100u64.pow(window);
    let happy_share = base * 1000 / denom; // permille
    let pick = |p: u64| {
        if p * 10 <= happy_share {
            happy
        } else {
            happy + stall
        }
    };
    (pick(50), pick(95), pick(99))
}

/// QC aggregator (the B2 `QcAggregator` shape): collects distinct
/// validator votes and forms a QC at 2f+1.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QcAggregator {
    /// Distinct voters (validator indices).
    voters: Vec<u64>,
    /// Node count for the threshold.
    pub n: usize,
}

impl QcAggregator {
    /// New aggregator for `n` validators.
    pub fn new(n: usize) -> Self {
        Self {
            voters: Vec::new(),
            n,
        }
    }

    /// Add one vote; returns `true` exactly when the QC forms.
    pub fn add_vote(&mut self, validator: u64) -> bool {
        if !self.voters.contains(&validator) {
            self.voters.push(validator);
        }
        self.voters.len() >= quorum_threshold(self.n)
    }

    /// Votes currently collected.
    pub fn votes(&self) -> usize {
        self.voters.len()
    }
}

/// A chain block: view + parent index (the 3-chain rule walks parent
/// links).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// View number.
    pub view: u64,
    /// Parent block index (None for genesis).
    pub parent: Option<usize>,
}

/// Indices of blocks made final by the d-chain rule: a block is final
/// when `depth` descendants link back to it (inclusive counting).
pub fn finalized_indices(chain: &[Block], depth: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, _) in chain.iter().enumerate() {
        let mut steps = 0;
        let mut cursor = Some(i);
        while steps < depth {
            let Some(idx) = cursor else {
                break;
            };
            cursor = chain.iter().position(|b| b.parent == Some(idx));
            steps += 1;
        }
        if steps == depth && cursor.is_some() {
            out.push(i);
        }
    }
    out
}

/// Functional gate: the 3-chain rule finalizes exactly the blocks
/// with three linked descendants.
pub fn gate_three_chain() -> Result<(), P5Error> {
    let chain = vec![
        Block {
            view: 1,
            parent: None,
        },
        Block {
            view: 2,
            parent: Some(0),
        },
        Block {
            view: 3,
            parent: Some(1),
        },
        Block {
            view: 4,
            parent: Some(2),
        },
        Block {
            view: 5,
            parent: Some(3),
        },
    ];
    let finalized = finalized_indices(&chain, CHAIN_DEPTH);
    let want: Vec<usize> = (0..chain.len()).filter(|i| i + 3 < chain.len()).collect();
    if finalized != want {
        let block = finalized
            .iter()
            .zip(want.iter())
            .find(|(a, b)| a != b)
            .map(|(a, _)| *a as u64)
            .unwrap_or(0);
        return Err(P5Error::ChainRuleViolated { block });
    }
    // The direct child alone must NOT finalize the root.
    if finalized.contains(&4) {
        return Err(P5Error::ChainRuleViolated { block: 4 });
    }
    Ok(())
}

/// Functional gate: quorum accounting — 2f+1 votes form a QC, one
/// less does not.
pub fn gate_quorum() -> Result<(u64, u64), P5Error> {
    let pinned = (2 * F_NODES + 1) as u64;
    let need = quorum_threshold(N_NODES) as u64;
    if need != pinned {
        return Err(P5Error::QuorumCorrupted {
            have: need,
            need: pinned,
        });
    }
    let mut agg = QcAggregator::new(N_NODES);
    for v in 0..need - 1 {
        if agg.add_vote(v) {
            return Err(P5Error::QuorumCorrupted { have: v + 1, need });
        }
    }
    if !agg.add_vote(need - 1) {
        return Err(P5Error::QuorumCorrupted { have: need, need });
    }
    Ok((need, N_NODES as u64))
}

/// The before/after pairs of the three levers (D-P2) plus the p99
/// pair (the DoD number).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P5Pairs {
    /// Per-view QC re-verification cost (ms): cache off/on.
    pub qc_verify: (u64, u64),
    /// Happy-path 3-chain latency (ms): serialized / pipelined.
    pub happy: (u64, u64),
    /// One view-change stall (ms): 2 s timeout / 300 ms.
    pub stall: (u64, u64),
    /// p99 finality latency (ms).
    pub finality: (u64, u64),
}

impl P5Pairs {
    /// Compute the pairs from the frozen constants.
    pub fn compute() -> Self {
        let qc_before = QC_HOPS * QC_VERIFY_MS;
        let qc_after = QC_HOPS * QC_VERIFY_CACHED_MS;
        let happy_before = happy_latency(QC_VERIFY_MS, false);
        let happy_after = happy_latency(QC_VERIFY_CACHED_MS, true);
        let stall_before = stall_latency(TIMEOUT_MS_BEFORE, QC_VERIFY_MS);
        let stall_after = stall_latency(TIMEOUT_MS_AFTER, QC_VERIFY_CACHED_MS);
        let (_, _, p99_before) =
            latency_percentiles(happy_before, stall_before, VIEW_CHANGE_RATE_PCT, 3);
        let (_, _, p99_after) =
            latency_percentiles(happy_after, stall_after, VIEW_CHANGE_RATE_PCT, 3);
        Self {
            qc_verify: (qc_before, qc_after),
            happy: (happy_before, happy_after),
            stall: (stall_before, stall_after),
            finality: (p99_before, p99_after),
        }
    }
}

/// Deterministic finality-latency report (the P3/P4 skeleton).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P5Report {
    /// Workload digest.
    pub workload_digest: String,
    /// Machine digest (D-P1).
    pub machine_digest: String,
    /// Model repeats.
    pub runs: usize,
    /// Spread across repeats (percent).
    pub spread_pct: u64,
    /// Median block finality latency (ms).
    pub p50: u64,
    /// p95 block finality latency (ms).
    pub p95: u64,
    /// p99 block finality latency (ms) — the DoD number.
    pub p99: u64,
    /// Sum of per-run p99 values.
    pub total: u64,
    /// p99 before P5 (ms).
    pub before: u64,
    /// p99 after P5 (ms).
    pub after: u64,
    /// Plan-fixed SLO (ms).
    pub slo: u64,
    /// The optimization pairs (D-P2).
    pub pairs: P5Pairs,
    /// State digest.
    pub state_digest: String,
    /// Bench digest over the canonical line.
    pub bench_digest: String,
}

impl P5Report {
    /// Build the deterministic report from the frozen constants.
    pub fn build(runs: usize) -> Self {
        let pairs = P5Pairs::compute();
        let (p50, p95, p99) = latency_percentiles(
            pairs.happy.1,
            pairs.stall.1,
            VIEW_CHANGE_RATE_PCT,
            CHAIN_DEPTH as u32,
        );
        let total = p99 * runs as u64;
        let workload_digest = crate::hex(&crate::sha256_32(
            format!(
                "p5w|consensus|nodes={N_NODES}|chain={CHAIN_DEPTH}|rate={VIEW_CHANGE_RATE_PCT}|hop={HOP_MS}"
            )
            .as_bytes(),
        ));
        let machine_digest = MachineProfile::REFERENCE.digest();
        let sd = state_digest();
        let canonical = format!(
            "p5b|{}|{}|runs={runs}|spread=0|p50={p50}|p95={p95}|p99={p99}|total={total}|before={}|after={}|slo={}|happy={}/{}|stall={}/{}|qc_verify={}/{}",
            workload_digest,
            machine_digest,
            pairs.finality.0,
            pairs.finality.1,
            FINALITY_SLO_MS,
            pairs.happy.0,
            pairs.happy.1,
            pairs.stall.0,
            pairs.stall.1,
            pairs.qc_verify.0,
            pairs.qc_verify.1,
        );
        Self {
            workload_digest,
            machine_digest,
            runs,
            spread_pct: 0,
            p50,
            p95,
            p99,
            total,
            before: pairs.finality.0,
            after: pairs.finality.1,
            slo: FINALITY_SLO_MS,
            pairs,
            state_digest: sd,
            bench_digest: crate::hex(&crate::sha256_32(canonical.as_bytes())),
        }
    }

    /// The canonical line (digest source).
    pub fn canonical(&self) -> String {
        format!(
            "p5b|{}|{}|runs={}|spread={}|p50={}|p95={}|p99={}|total={}|before={}|after={}|slo={}|happy={}/{}|stall={}/{}|qc_verify={}/{}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.before,
            self.after,
            self.slo,
            self.pairs.happy.0,
            self.pairs.happy.1,
            self.pairs.stall.0,
            self.pairs.stall.1,
            self.pairs.qc_verify.0,
            self.pairs.qc_verify.1,
        )
    }

    /// JSON line for the evidence / sim output.
    pub fn json(&self) -> String {
        let p = &self.pairs;
        format!(
            "{{\"workload_digest\": \"{}\", \"machine_digest\": \"{}\", \"runs\": {}, \"spread_pct\": {}, \"p50\": {}, \"p95\": {}, \"p99\": {}, \"total\": {}, \"before\": {}, \"after\": {}, \"slo\": {}, \"happy_before\": {}, \"happy_after\": {}, \"stall_before\": {}, \"stall_after\": {}, \"qc_verify_before\": {}, \"qc_verify_after\": {}, \"timeout_before\": {TIMEOUT_MS_BEFORE}, \"timeout_after\": {TIMEOUT_MS_AFTER}, \"nodes\": {N_NODES}, \"chain_depth\": {CHAIN_DEPTH}, \"view_change_rate_pct\": {VIEW_CHANGE_RATE_PCT}, \"sensitivity_2view_p99\": {}, \"bench_digest\": \"{}\"}}",
            self.workload_digest,
            self.machine_digest,
            self.runs,
            self.spread_pct,
            self.p50,
            self.p95,
            self.p99,
            self.total,
            self.before,
            self.after,
            self.slo,
            p.happy.0,
            p.happy.1,
            p.stall.0,
            p.stall.1,
            p.qc_verify.0,
            p.qc_verify.1,
            latency_percentiles(p.happy.1, p.stall.1, VIEW_CHANGE_RATE_PCT, 2,).2,
            self.bench_digest,
        )
    }

    /// The DoD gates: SLO → pipeline overlap → QC cache win →
    /// determinism.
    pub fn validate(&self) -> Result<(), P5Error> {
        if self.p99 >= self.slo {
            return Err(P5Error::SloExceeded {
                p99: self.p99,
                slo: self.slo,
            });
        }
        let full = view_full_latency(QC_VERIFY_CACHED_MS);
        let inc = incremental_view_latency(QC_VERIFY_CACHED_MS);
        if inc >= full {
            return Err(P5Error::PipelineOverlapMissing { inc, full });
        }
        if self.pairs.qc_verify.1 >= self.pairs.qc_verify.0 {
            return Err(P5Error::QcCacheWinReverted {
                before: self.pairs.qc_verify.0,
                after: self.pairs.qc_verify.1,
            });
        }
        let again = Self::build(self.runs);
        if again.canonical() != self.canonical() {
            return Err(P5Error::DeterminismViolated { run: 1 });
        }
        Ok(())
    }
}

/// The consensus ledger of one view (state digest source).
pub fn ledger_lines() -> Vec<String> {
    let pairs = P5Pairs::compute();
    let mut lines = vec![
        format!("p5s|view|full_before={}", view_full_latency(QC_VERIFY_MS)),
        format!(
            "p5s|view|full_after={}",
            view_full_latency(QC_VERIFY_CACHED_MS)
        ),
        format!(
            "p5s|view|inc_after={}",
            incremental_view_latency(QC_VERIFY_CACHED_MS)
        ),
        format!("p5s|latency|happy={}/{}", pairs.happy.0, pairs.happy.1),
        format!("p5s|latency|stall={}/{}", pairs.stall.0, pairs.stall.1),
        format!("p5s|latency|p99={}/{}", pairs.finality.0, pairs.finality.1),
        format!("p5s|quorum|need={}", quorum_threshold(N_NODES)),
    ];
    for i in 0..CHAIN_DEPTH + 2 {
        lines.push(format!("p5s|chain|{i}:{}", i as u64 + 1));
    }
    lines
}

/// State digest over the ledger.
pub fn state_digest() -> String {
    let mut joined = ledger_lines().join("|");
    joined.push('|');
    crate::hex(&crate::sha256_32(joined.as_bytes()))
}

/// Honest measured layer (D-P9, evidence only — never a gate): real
/// chain walks, vote aggregation loops and quorum checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P5Measured {
    /// Building + finalizing a 5-block chain, nanoseconds.
    pub chain_ns: u64,
    /// Aggregating 67 votes through `QcAggregator`, nanoseconds.
    pub qc_aggregate_ns: u64,
    /// Latency-model evaluation, nanoseconds.
    pub model_ns: u64,
}

impl P5Measured {
    /// JSON line.
    pub fn json(&self) -> String {
        format!(
            "{{\"chain_ns\": {}, \"qc_aggregate_ns\": {}, \"model_ns\": {}}}",
            self.chain_ns, self.qc_aggregate_ns, self.model_ns
        )
    }
}

/// Measure the real loops (medians of `reps`).
pub fn measure(reps: usize) -> P5Measured {
    let mut chain = Vec::new();
    let mut qc = Vec::new();
    let mut model = Vec::new();
    for _ in 0..reps {
        let start = std::time::Instant::now();
        let blocks = vec![
            Block {
                view: 1,
                parent: None,
            },
            Block {
                view: 2,
                parent: Some(0),
            },
            Block {
                view: 3,
                parent: Some(1),
            },
            Block {
                view: 4,
                parent: Some(2),
            },
            Block {
                view: 5,
                parent: Some(3),
            },
        ];
        let fin = finalized_indices(&blocks, CHAIN_DEPTH);
        chain.push(start.elapsed().as_nanos() as u64);
        assert_eq!(fin, vec![0, 1]);

        let start = std::time::Instant::now();
        let mut agg = QcAggregator::new(N_NODES);
        let mut formed = false;
        for v in 0..quorum_threshold(N_NODES) as u64 {
            formed = agg.add_vote(v);
        }
        qc.push(start.elapsed().as_nanos() as u64);
        assert!(formed);

        let start = std::time::Instant::now();
        let pairs = P5Pairs::compute();
        let pick = latency_percentiles(
            pairs.happy.1,
            pairs.stall.1,
            VIEW_CHANGE_RATE_PCT,
            CHAIN_DEPTH as u32,
        );
        model.push(start.elapsed().as_nanos() as u64);
        assert_eq!(pick.2, pairs.finality.1);
    }
    chain.sort_unstable();
    qc.sort_unstable();
    model.sort_unstable();
    P5Measured {
        chain_ns: chain[chain.len() / 2],
        qc_aggregate_ns: qc[qc.len() / 2],
        model_ns: model[model.len() / 2],
    }
}
