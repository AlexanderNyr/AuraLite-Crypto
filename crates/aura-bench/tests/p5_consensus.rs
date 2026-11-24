//! P5 consensus gates (PERF_PLAN P5 DoD).
//!
//! DoD: **p99 finality at 100 nodes < 2 s** — the SLO is plan-fixed
//! (2000 ms) and its methodology is published
//! (`docs/perf/p5-consensus-report.md`, D-P3). Three optimizations
//! carry before/after pairs (D-P2): pipelining, QC cache, faster view
//! change. NC: revert → red (`tools/nc_p5.py`, six named messages).

use aura_bench::p5::{
    Block, FINALITY_SLO_MS, N_NODES, P5Error, P5Pairs, P5Report, QC_HOPS, QC_VERIFY_CACHED_MS,
    QC_VERIFY_MS, TIMEOUT_MS_AFTER, TIMEOUT_MS_BEFORE, VIEW_CHANGE_RATE_PCT, finalized_indices,
    gate_quorum, gate_three_chain, happy_latency, incremental_view_latency, latency_percentiles,
    quorum_threshold, stall_latency, view_full_latency,
};

/// Golden numbers pinned before the first run (2026-11-24): 100
/// validators, HotStuff-like 3-chain; baseline (serialized chain,
/// per-signature QC re-verification, 2 s timeout — the shipped
/// `DEFAULT_VIEW_TIMEOUT_MS`) p99 2 476 ms honestly fails the 2 s
/// SLO; P5 path (pipelined, QC cache, 300 ms timeout) p99 586 ms;
/// report digest
/// `34dd71879cd2de0cbf3246a5403d1d6e13ef0e84fbaa7595aa92119a01fa4c43`.
#[test]
fn p5_golden_numbers() {
    let report = P5Report::build(5);
    assert_eq!(report.runs, 5);
    assert_eq!((report.p50, report.p95, report.p99), (189, 189, 586));
    assert_eq!(report.total, 2930);
    assert_eq!(report.spread_pct, 0);
    assert_eq!(report.before, 2476);
    assert_eq!(report.after, 586);
    assert_eq!(report.slo, 2000);
    assert_eq!(report.pairs.qc_verify, (24, 2));
    assert_eq!(report.pairs.happy, (357, 189));
    assert_eq!(report.pairs.stall, (2119, 397));
    assert_eq!(report.pairs.finality, (2476, 586));
    assert_eq!(
        report.workload_digest,
        "69579d3a72d7a526330684fd4b06ff5751fd00c8d7b617a57db5206918676764"
    );
    assert_eq!(
        report.bench_digest,
        "34dd71879cd2de0cbf3246a5403d1d6e13ef0e84fbaa7595aa92119a01fa4c43"
    );
    assert!(report.validate().is_ok());
}

/// The P5 DoD, asserted literally: p99 block finality at 100 nodes is
/// under 2 s; the baseline honestly fails it.
#[test]
fn p5_do_d_p99_finality_under_2s() {
    assert_eq!(N_NODES, 100);
    assert_eq!(FINALITY_SLO_MS, 2000);
    let report = P5Report::build(5);
    assert_eq!(report.p99, 586);
    assert!(report.p99 < FINALITY_SLO_MS, "P5 DoD: p99 finality < 2 s");
    assert!(
        report.before >= FINALITY_SLO_MS,
        "the shipped path fails the SLO honestly (2 s timeout stall)"
    );
}

/// The six named P5 messages (NC targets): exact Display literals.
#[test]
fn p5_named_messages() {
    assert_eq!(
        P5Error::SloExceeded {
            p99: 2476,
            slo: 2000
        }
        .to_string(),
        "P5: p99 finality exceeds SLO (2476/2000)"
    );
    assert_eq!(
        P5Error::PipelineOverlapMissing { inc: 97, full: 97 }.to_string(),
        "P5: pipelining overlap missing (97/97)"
    );
    assert_eq!(
        P5Error::QcCacheWinReverted {
            before: 24,
            after: 24
        }
        .to_string(),
        "P5: QC cache win reverted (24/24)"
    );
    assert_eq!(
        P5Error::ChainRuleViolated { block: 3 }.to_string(),
        "P5: 3-chain rule violated (block 3)"
    );
    assert_eq!(
        P5Error::QuorumCorrupted { have: 34, need: 67 }.to_string(),
        "P5: quorum accounting corrupted (34/67)"
    );
    assert_eq!(
        P5Error::DeterminismViolated { run: 2 }.to_string(),
        "P5: determinism violated (run 2)"
    );
}

/// 3-chain rule: a block is final exactly when three linked
/// descendants exist; the tail blocks are not final.
#[test]
fn p5_three_chain_rule() {
    gate_three_chain().expect("3-chain rule");
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
    ];
    assert_eq!(
        finalized_indices(&chain, 3),
        Vec::<usize>::new(),
        "no block final without a grandchild"
    );
    let mut with_grandchild = chain.clone();
    with_grandchild.push(Block {
        view: 4,
        parent: Some(2),
    });
    assert_eq!(finalized_indices(&with_grandchild, 3), vec![0]);
}

/// Quorum accounting (D-B2): 2f+1 forms the QC; one vote less does
/// not; the threshold is 67 at 100 validators.
#[test]
fn p5_quorum_accounting() {
    let (need, have) = gate_quorum().expect("quorum accounting");
    assert_eq!((need, have), (67, 100));
    assert_eq!(quorum_threshold(100), 67);
    assert_eq!(quorum_threshold(4), 3, "f=1: 3 signatures commit");
    assert_eq!(quorum_threshold(7), 5);
}

/// Pipelining: the chain-extension view is strictly cheaper than the
/// full view (the proposal overlaps), giving happy 357 → 189 ms.
#[test]
fn p5_pipelining_overlap() {
    let full = view_full_latency(QC_VERIFY_CACHED_MS);
    let inc = incremental_view_latency(QC_VERIFY_CACHED_MS);
    assert_eq!((full, inc), (97, 46));
    assert!(inc < full, "the chain extension must overlap the proposal");
    assert_eq!(
        happy_latency(QC_VERIFY_CACHED_MS, true),
        189,
        "pipelined happy path"
    );
    assert_eq!(
        happy_latency(QC_VERIFY_MS, false),
        357,
        "serialized happy path"
    );
}

/// QC cache: O(1) verification cuts the per-view re-verification from
/// 24 ms (per-signature, shipped B2) to 2 ms.
#[test]
fn p5_qc_cache_win() {
    assert_eq!(QC_HOPS * QC_VERIFY_MS, 24);
    assert_eq!(QC_HOPS * QC_VERIFY_CACHED_MS, 2);
    let pairs = P5Pairs::compute();
    assert_eq!(pairs.qc_verify, (24, 2));
}

/// Faster view change: the stall drops from 2 119 ms (shipped 2 s
/// timeout) to 397 ms; the pinned percentile rule keeps p99 = happy +
/// one stall.
#[test]
fn p5_view_change_win() {
    assert_eq!(TIMEOUT_MS_BEFORE, 2000, "shipped DEFAULT_VIEW_TIMEOUT_MS");
    assert_eq!(TIMEOUT_MS_AFTER, 300);
    assert_eq!(stall_latency(TIMEOUT_MS_BEFORE, QC_VERIFY_MS), 2119);
    assert_eq!(stall_latency(TIMEOUT_MS_AFTER, QC_VERIFY_CACHED_MS), 397);
    let (p50, p95, p99) = latency_percentiles(189, 397, VIEW_CHANGE_RATE_PCT, 3);
    assert_eq!((p50, p95, p99), (189, 189, 586));
    // The rule: happy share is 970 permille at 1% per view over 3 views.
    let (a, b, c) = latency_percentiles(189, 397, 1, 3);
    assert_eq!((a, b, c), (189, 189, 586));
    // A higher stall rate pushes p95 into the stall branch.
    let (a, b, c) = latency_percentiles(189, 397, 10, 3);
    assert_eq!((a, b, c), (189, 586, 586));
}

/// Conservative sensitivity (published): a 2-view finality window at
/// the same stall rate also fits the SLO.
#[test]
fn p5_sensitivity_two_view_window() {
    let (p50, p95, p99) = latency_percentiles(189, 397, VIEW_CHANGE_RATE_PCT, 2);
    assert_eq!((p50, p95, p99), (189, 189, 586));
    assert!(p99 < FINALITY_SLO_MS, "2-view sensitivity fits the SLO");
}

/// X7 determinism: the report is a pure function of its inputs.
#[test]
fn p5_determinism() {
    let r1 = P5Report::build(5);
    let r2 = P5Report::build(5);
    assert_eq!(r1, r2);
    assert_eq!(r1.canonical(), r2.canonical());
    assert_eq!(r1.state_digest, r2.state_digest);
    assert_eq!(r1.state_digest.len(), 64);
}

/// NC: revert → red. Each reverted state fails with its named
/// message (the evidence for the D-P2 contract).
#[test]
fn p5_validate_reverts_are_red() {
    let good = P5Report::build(5);

    // Revert the faster view change (300 -> 2000 ms): p99 back over
    // the SLO.
    let mut reverted_timeout = good.clone();
    reverted_timeout.p99 = 2288;
    reverted_timeout.after = 2288;
    assert_eq!(
        reverted_timeout.validate().unwrap_err(),
        P5Error::SloExceeded {
            p99: 2288,
            slo: 2000
        }
    );

    // Revert the QC cache (2 -> 24 ms per view): the win disappears.
    let mut reverted_cache = good.clone();
    reverted_cache.pairs.qc_verify = (24, 24);
    assert_eq!(
        reverted_cache.validate().unwrap_err(),
        P5Error::QcCacheWinReverted {
            before: 24,
            after: 24
        }
    );

    // Break determinism: one field disagrees with the rebuild.
    let mut nondet = good.clone();
    nondet.p99 = 587;
    assert_eq!(
        nondet.validate().unwrap_err(),
        P5Error::DeterminismViolated { run: 1 }
    );
}

/// The quorum gate catches a corrupted threshold (mutation target).
#[test]
fn p5_quorum_gate_is_structural() {
    // A threshold of f+1 = 34 would form a QC at 34 votes — the gate
    // must not accept that shape.
    assert!(quorum_threshold(N_NODES) > F_PLUS_ONE, "2f+1 > f+1");
    let mut agg = aura_bench::p5::QcAggregator::new(N_NODES);
    for v in 0..33u64 {
        assert!(!agg.add_vote(v), "no QC below the threshold");
    }
    for v in 33..66u64 {
        assert!(!agg.add_vote(v), "no QC at 66 votes");
    }
    assert!(agg.add_vote(66), "QC forms at exactly 67");
}

const F_PLUS_ONE: usize = (N_NODES - 1) / 3 + 1;

/// State digest stability (X7 evidence).
#[test]
fn p5_state_digest_stable() {
    let report = P5Report::build(5);
    let again = P5Report::build(5);
    assert_eq!(report.state_digest, again.state_digest);
    assert_eq!(report.bench_digest, again.bench_digest);
}
