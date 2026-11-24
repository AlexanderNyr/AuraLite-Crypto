//! P2 parallel execution gates (PERF_PLAN P2 DoD).
//!
//! DoD: **1000 txs: parallel result == sequential; conflicts roll back
//! honestly**. NC: revert → red (`tools/nc_p2.py`, six named messages).

use std::collections::BTreeMap;

use aura_bench::p2::{
    ExecResult, FIXTURE_CONFLICTS, FIXTURE_TXS, P2Error, P2Report, Tx, WORKERS, diagnose_mismatch,
    fixture_state, fixture_txs, gate_commit_order, gate_fixture, gate_parallel, par_cost,
    parallel_execute, seq_cost, sequential_execute, state_canonical, state_digest,
};

/// Golden numbers pinned before the first run (2026-11-24):
/// 1000 txs over 2096 accounts, 8 workers, 32 structural conflicts;
/// seq 99 000 -> par 15 543 units (+536% exec throughput);
/// state digest `e5759337…75537`, report digest `96863c0b…65174`.
#[test]
fn p2_golden_numbers() {
    let par = parallel_execute(&fixture_txs(), &fixture_state(), WORKERS);
    let report = P2Report::build(&par, 5);
    assert_eq!(report.runs, 5);
    assert_eq!(report.run_costs, vec![15543, 15543, 15543, 15543, 15543]);
    assert_eq!(report.total, 77715);
    assert_eq!((report.p50, report.p95, report.p99), (15543, 15543, 15543));
    assert_eq!(report.spread_pct, 0);
    assert_eq!(report.seq_cost, 99000);
    assert_eq!(report.par_cost, 15543);
    assert_eq!(report.workers, 8);
    assert_eq!(report.aborted, 32);
    assert_eq!(report.speedup_pct, 536);
    assert_eq!(
        report.workload_digest,
        "c106ad1566590c1f0c7432e15089caf249e2ba32b0a8e6432e567dc7b429eb09"
    );
    assert_eq!(
        report.state_digest,
        "e57593374e5098bae2515d1ef9c70e1c8e6a0624114279dc01eb0b62e7757537"
    );
    assert_eq!(
        report.bench_digest,
        "96863c0b96671e12424ec6befc818b1a5028fda01789ad585179424aa3265174"
    );
    assert!(report.validate().is_ok());
}

/// The P2 DoD, asserted literally on the pinned fixture.
#[test]
fn p2_do_d_parallel_equals_sequential() {
    let txs = fixture_txs();
    let base = fixture_state();
    let seq = sequential_execute(&txs, &base);
    let par = gate_parallel(&txs, &base).expect("P2 DoD: parallel == sequential");
    assert_eq!(
        par.state, seq.state,
        "1000 txs: parallel result == sequential"
    );
    assert_eq!(par.state_digest, seq.state_digest);
    assert_eq!(seq_cost(FIXTURE_TXS), 99_000);
    assert_eq!(par_cost(FIXTURE_TXS, WORKERS, 32), 15_543);
}

/// The six named P2 messages (NC targets): exact Display literals.
#[test]
fn p2_named_messages() {
    assert_eq!(
        P2Error::ParallelMismatch {
            expected: "abcd".into(),
            actual: "ef01".into()
        }
        .to_string(),
        "P2: parallel result mismatch (abcd/ef01)"
    );
    assert_eq!(
        P2Error::ConflictNotRolledBack { tx: 7 }.to_string(),
        "P2: conflict not rolled back (tx 7)"
    );
    assert_eq!(
        P2Error::RetryOrderViolated { tx: 3 }.to_string(),
        "P2: retry order violated (tx 3)"
    );
    assert_eq!(
        P2Error::SpeedupRegression {
            before: 99000,
            after: 99000
        }
        .to_string(),
        "P2: speedup regression (99000/99000)"
    );
    assert_eq!(
        P2Error::ConflictCountMismatch {
            expected: 32,
            actual: 33
        }
        .to_string(),
        "P2: conflict count mismatch (32/33)"
    );
    assert_eq!(
        P2Error::DeterminismViolated { run: 2 }.to_string(),
        "P2: determinism violated (run 2)"
    );
}

/// Conflicts roll back honestly: a crafted cross-chunk conflict aborts
/// exactly once, the speculative writes are discarded, the retry lands
/// the sequential result.
#[test]
fn p2_conflicts_roll_back_honestly() {
    // tx0 (chunk 0) and tx1 (chunk 1) both draw from account 0.
    let txs = vec![
        Tx {
            from: 0,
            to: 1,
            amount: 10,
        },
        Tx {
            from: 0,
            to: 2,
            amount: 20,
        },
    ];
    let base: BTreeMap<usize, u128> = [(0, 1_000_000u128), (1, 0), (2, 0)].into();
    let seq = sequential_execute(&txs, &base);
    let par = parallel_execute(&txs, &base, WORKERS);
    assert_eq!(par.aborted, 1, "exactly the conflicting tx aborts");
    assert_eq!(par.applied, 2);
    assert_eq!(par.state, seq.state, "honest rollback: retry == sequential");
    // The aborted incarnation's speculative writes were recorded and
    // discarded: tx1 speculated account 0 -> 999_980 and account 2 -> 20.
    assert!(par.discarded.contains(&(1, 0, 999_980)));
    assert!(par.discarded.contains(&(1, 2, 20)));
    // The leak diagnosis agrees there is no leak in an honest run.
    assert_ne!(
        diagnose_mismatch(&seq, &par),
        P2Error::ConflictNotRolledBack { tx: 1 }
    );
}

/// The leak diagnosis fires when a discarded speculative value
/// survives into the final state (`P2: conflict not rolled back`).
#[test]
fn p2_rollback_leak_detected() {
    let seq = ExecResult {
        state_digest: state_digest(&[(0, 970u128), (1, 10), (2, 20)].into()),
        state: [(0, 970u128), (1, 10), (2, 20)].into(),
        applied: 2,
        rejected: 0,
        aborted: 1,
        commit_order: vec![0, 1],
        discarded: vec![(1, 0, 960), (1, 2, 40)],
    };
    let par = ExecResult {
        state_digest: state_digest(&[(0, 960u128), (1, 10), (2, 40)].into()),
        state: [(0, 960u128), (1, 10), (2, 40)].into(),
        applied: 2,
        rejected: 0,
        aborted: 1,
        commit_order: vec![0, 1],
        discarded: vec![(1, 0, 960), (1, 2, 40)],
    };
    assert_eq!(
        diagnose_mismatch(&seq, &par),
        P2Error::ConflictNotRolledBack { tx: 1 },
        "the discarded value 960 leaked into the final state"
    );
}

/// Serializable commit order: transactions commit exactly in index
/// order (the deterministic in-block order).
#[test]
fn p2_commit_order_gate() {
    let txs = fixture_txs();
    let base = fixture_state();
    gate_commit_order(&txs, &base).expect("commit order 0..n");
    let par = parallel_execute(&txs, &base, WORKERS);
    assert_eq!(par.commit_order, (0..FIXTURE_TXS).collect::<Vec<_>>());
}

/// The planted-conflict accounting: exactly `FIXTURE_CONFLICTS`
/// structural cross-chunk conflicts, nothing else.
#[test]
fn p2_structural_conflicts_only() {
    let par = gate_fixture(&fixture_txs(), &fixture_state(), FIXTURE_CONFLICTS)
        .expect("32 structural conflicts");
    assert_eq!(par.aborted, FIXTURE_CONFLICTS);
    assert_eq!(par.applied + par.rejected, FIXTURE_TXS);
}

/// X7 determinism: the engine is a pure function of its inputs (real
/// threads only change timing, never results) and the report is pure.
#[test]
fn p2_engine_determinism() {
    let txs = fixture_txs();
    let base = fixture_state();
    let a = parallel_execute(&txs, &base, WORKERS);
    let b = parallel_execute(&txs, &base, WORKERS);
    assert_eq!(a, b, "thread timing cannot change the result");
    let ra = P2Report::build(&a, 5);
    let rb = P2Report::build(&b, 5);
    assert_eq!(ra, rb);
    assert_eq!(ra.canonical(), rb.canonical());
}

/// Metamorphic scaling: 2000 disjoint transfers have zero conflicts
/// and parallel == sequential; costs scale linearly.
#[test]
fn p2_scale_disjoint() {
    let txs: Vec<Tx> = (0..2000)
        .map(|i| Tx {
            from: 2 * i,
            to: 2 * i + 1,
            amount: 1,
        })
        .collect();
    let base: BTreeMap<usize, u128> = (0..4000).map(|i| (i, 1_000u128)).collect();
    let seq = sequential_execute(&txs, &base);
    let par = parallel_execute(&txs, &base, WORKERS);
    assert_eq!(par.aborted, 0, "disjoint txs never conflict");
    assert_eq!(par.state, seq.state);
    assert_eq!(seq_cost(2000), 198_000);
    assert_eq!(par_cost(2000, WORKERS, 0), 24_750);
}

/// Metamorphic invariant: both executors conserve the total balance.
#[test]
fn p2_balance_conservation() {
    let txs = fixture_txs();
    let base = fixture_state();
    let total: u128 = base.values().sum();
    for result in [
        sequential_execute(&txs, &base),
        parallel_execute(&txs, &base, WORKERS),
    ] {
        assert_eq!(result.state.values().sum::<u128>(), total, "conservation");
    }
}

/// NC: revert → red. Each reverted optimization state fails with its
/// named message (the evidence for the P2 DoD clause).
#[test]
fn p2_validate_reverts_are_red() {
    let par = parallel_execute(&fixture_txs(), &fixture_state(), WORKERS);
    let good = P2Report::build(&par, 5);

    // Revert the parallel win: par cost back to sequential.
    let mut reverted_speedup = good.clone();
    reverted_speedup.par_cost = reverted_speedup.seq_cost;
    assert_eq!(
        reverted_speedup.validate().unwrap_err(),
        P2Error::SpeedupRegression {
            before: 99000,
            after: 99000
        }
    );

    // Break determinism: one run differs.
    let mut nondet = good.clone();
    nondet.run_costs[2] += 1;
    assert_eq!(
        nondet.validate().unwrap_err(),
        P2Error::DeterminismViolated { run: 2 }
    );

    // Conflict accounting drift.
    assert_eq!(
        gate_fixture(&fixture_txs(), &fixture_state(), 31).unwrap_err(),
        P2Error::ConflictCountMismatch {
            expected: 31,
            actual: 32
        }
    );
}

/// The canonical state form and digest are stable (the golden digest
/// input is exactly `p2s|idx:bal|…`).
#[test]
fn p2_state_canonical_stable() {
    let state: BTreeMap<usize, u128> = [(3, 7u128), (1, 5), (2, 6)].into();
    assert_eq!(state_canonical(&state), "p2s|1:5|2:6|3:7");
    let d = state_digest(&state);
    assert_eq!(d.len(), 64);
    let again = state_digest(&state);
    assert_eq!(d, again);
}
