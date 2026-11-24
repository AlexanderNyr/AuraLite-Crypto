//! P5 consensus bench example — deterministic golden output.
//!
//! Output is byte-identical to `tools/p5_consensus_check.py` (the
//! independent Python model). `--measured` adds honest wall-clock
//! numbers of the real chain/QC loops (evidence only, D-P9 — never
//! byte-compared, never a gate).

use aura_bench::p5::{
    FINALITY_SLO_MS, P5Report, gate_quorum, gate_three_chain, latency_percentiles, measure,
};

fn main() {
    // 1. Structural gates: the 3-chain rule and 2f+1 quorum.
    gate_three_chain().expect("3-chain rule");
    let (need, have) = gate_quorum().expect("quorum accounting");
    assert_eq!((need, have), (67, 100));

    // 2. The P4-style pairs (D-P2), each lever must win.
    let report = P5Report::build(5);
    assert!(
        report.pairs.qc_verify.1 < report.pairs.qc_verify.0,
        "QC cache"
    );
    assert!(report.pairs.happy.1 < report.pairs.happy.0, "pipelining");
    assert!(report.pairs.stall.1 < report.pairs.stall.0, "view change");
    assert!(
        report.pairs.finality.1 < report.pairs.finality.0,
        "finality"
    );

    // 3. The P5 DoD: p99 finality at 100 nodes < 2 s.
    let (p50, p95, p99) = latency_percentiles(report.pairs.happy.1, report.pairs.stall.1, 1, 3);
    assert_eq!((p50, p95, p99), (189, 189, 586));
    assert!(p99 < FINALITY_SLO_MS, "P5 DoD");

    // 4. Deterministic report + DoD gates.
    report.validate().expect("P5 gates green");

    println!("P5-RUNS-OK {}", report.runs);
    println!("P5-CONSENSUS-OK {}", report.json());

    // 5. Measured layer (evidence only, D-P9).
    if std::env::args().any(|a| a == "--measured") {
        let m = measure(5);
        println!("P5-MEASURED {}", m.json());
    }
}
