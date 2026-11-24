//! P2 parallel execution bench example — deterministic golden output.
//!
//! Output is byte-identical to `tools/p2_parallel_check.py` (the
//! independent Python model). `--measured` adds honest wall-clock
//! before/after numbers of the real executors (evidence only, D-P9 —
//! never byte-compared, never a gate).

use aura_bench::p2::{
    FIXTURE_CONFLICTS, P2Report, fixture_state, fixture_txs, gate_commit_order, gate_parallel,
    measure, parallel_execute, sequential_execute,
};

fn main() {
    let txs = fixture_txs();
    let base = fixture_state();

    // 1. The P2 DoD: parallel == sequential on 1000 txs.
    let par = gate_parallel(&txs, &base).expect("parallel == sequential");
    // 2. Serializable commit order.
    gate_commit_order(&txs, &base).expect("commit order");
    // 3. Structural conflicts: exactly the 32 planted ones.
    assert_eq!(par.aborted, FIXTURE_CONFLICTS, "planted conflicts only");
    // 4. Conflict accounting == sequential verdicts.
    let seq = sequential_execute(&txs, &base);
    assert_eq!(par.applied, seq.applied);
    assert_eq!(par.rejected, seq.rejected);
    let again = parallel_execute(&txs, &base, aura_bench::p2::WORKERS);
    assert_eq!(par, again, "the engine is deterministic (X7)");

    // 5. Deterministic report + optimization gates.
    let report = P2Report::build(&par, 5);
    report.validate().expect("P2 gates green");

    println!("P2-RUNS-OK {}", report.runs);
    println!("P2-PARALLEL-OK {}", report.json());

    // 6. Measured layer (evidence only, D-P9).
    if std::env::args().any(|a| a == "--measured") {
        let m = measure(5);
        println!("P2-MEASURED {}", m.json());
    }
}
