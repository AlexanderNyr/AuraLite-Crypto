//! P4 network bench example — deterministic golden output.
//!
//! Output is byte-identical to `tools/p4_network_check.py` (the
//! independent Python model). `--measured` adds honest wall-clock
//! numbers of the real RLE codec (evidence only, D-P9 — never
//! byte-compared, never a gate).

use aura_bench::p4::{
    BUDGET_BYTES, P4Report, gate_budget, gate_compression, gate_gossip_params, gate_mesh,
    gate_vote_batching, measure, vote_fixture,
};

fn main() {
    // 1. The four optimization gates (each carries its before/after
    //    pair, D-P2).
    let (votes_b, votes_a) = gate_vote_batching(&vote_fixture()).expect("vote batching");
    assert!(votes_a < votes_b, "batching must win");
    let (proposal_b, proposal_a) = gate_compression().expect("compression lossless");
    assert!(proposal_a < proposal_b, "compression must win");
    let (over_b, over_a) = gate_gossip_params().expect("gossip params tuned");
    assert!(over_a < over_b, "parameter tuning must win");
    let (mesh_b, mesh_a) = gate_mesh().expect("mesh connected");
    assert!(mesh_a < mesh_b, "mesh tuning must win");

    // 2. The P4 DoD: traffic/validator within the published budget.
    let traffic = gate_budget().expect("traffic within budget");
    assert_eq!(traffic, 86_495);

    // 3. Deterministic report + DoD gates.
    let report = P4Report::build(5);
    report.validate().expect("P4 gates green");
    assert_eq!(report.budget, BUDGET_BYTES);

    println!("P4-RUNS-OK {}", report.runs);
    println!("P4-NETWORK-OK {}", report.json());

    // 4. Measured layer (evidence only, D-P9).
    if std::env::args().any(|a| a == "--measured") {
        let m = measure(5);
        println!("P4-MEASURED {}", m.json());
    }
}
