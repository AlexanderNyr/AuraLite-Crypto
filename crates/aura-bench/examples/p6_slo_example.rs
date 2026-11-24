//! P6 SLO publication example — deterministic golden output.
//!
//! Output is byte-identical to `tools/p6_slo_check.py` (the
//! independent Python model). `--measured` adds honest wall-clock
//! numbers of the real hash-chain build/verify (evidence only, D-P9 —
//! never byte-compared, never a gate).

use aura_bench::p6::{
    P6Report, build_log, gate_honest_failures, gate_methodology, gate_north_star, measure,
    verify_log,
};

fn main() {
    // 1. The publication gates: north star, methodology, honest items.
    gate_north_star().expect("north star confirmed");
    gate_methodology().expect("methodology complete");
    gate_honest_failures().expect("honest items kept");

    // 2. The tamper-evident run log verifies (chain + registry).
    let tip = verify_log(&build_log()).expect("run log clean");
    assert_eq!(
        tip,
        "ac0f90675c11f118f03c1a12007b912651732683a780df1a825df35d9aefe52a"
    );

    // 3. Deterministic report + DoD gates.
    let report = P6Report::build(5);
    report.validate().expect("P6 gates green");
    assert_eq!(report.confirmed, 6);

    println!("P6-RUNS-OK {}", report.runs);
    println!("P6-SLO-OK {}", report.json());

    // 4. Measured layer (evidence only, D-P9).
    if std::env::args().any(|a| a == "--measured") {
        let m = measure(5);
        println!("P6-MEASURED {}", m.json());
    }
}
