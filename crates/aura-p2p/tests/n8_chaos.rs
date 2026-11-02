//! N8 chaos harness gate & `NET_PLAN` closure criteria:
//! 1. 20 scenarios × 3 runs (= 60 runs) across the 100-node stand complete
//!    with zero finalized-head divergence (`divergent_runs == 0`).
//! 2. 100 nodes with 20% packet loss and 1 Byzantine peer reach the exact
//!    same finalized `state_root`, and an N7 light client verifies it without
//!    a full node.

use aura_p2p::chaos::{ChaosStand, DEFAULT_STAND_NODES, canonical_20_scenarios, run_chaos_matrix};

const CHAIN: &str = "auralite-devnet-1";

#[test]
fn twenty_scenarios_times_three_runs_on_100_node_stand_have_zero_finalized_head_divergence() {
    let scenarios = canonical_20_scenarios();
    assert_eq!(scenarios.len(), 20);
    for scenario in &scenarios {
        assert_eq!(
            scenario.node_count, DEFAULT_STAND_NODES,
            "every canonical N8 scenario must run on the 100-node stand"
        );
    }

    let summary = run_chaos_matrix(CHAIN, &scenarios, 3)
        .expect("20 scenarios × 3 runs chaos matrix must complete");
    assert_eq!(summary.scenarios_count, 20);
    assert_eq!(summary.runs_per_scenario, 3);
    assert_eq!(summary.total_runs, 60);
    assert_eq!(
        summary.divergent_runs, 0,
        "20 scenarios × 3 runs must have zero finalized-head divergence"
    );
    assert_eq!(summary.reports.len(), 60);

    for report in &summary.reports {
        assert_eq!(report.node_count, 100);
        assert!(
            report.honest_nodes_converged >= 99,
            "all honest nodes in {} run {} must converge",
            report.scenario,
            report.run_index
        );
        assert!(
            report.light_client_verified,
            "light client must verify the finalized state_root in {} run {}",
            report.scenario, report.run_index
        );
    }

    // Explicitly verify NET_PLAN closure criteria on S01 (100 nodes, 20% packet loss, 1 Byzantine peer).
    let s01 = &scenarios[0];
    assert_eq!(s01.name, "S01_100nodes_20pct_loss_1_byzantine");
    assert_eq!(s01.node_count, 100);
    assert_eq!(s01.packet_loss_bps, 2_000);
    assert_eq!(s01.byzantine_node, Some(6));

    let mut stand = ChaosStand::new(CHAIN, s01.clone()).unwrap();
    let closure_report = stand.execute_run(0).unwrap();
    assert_eq!(closure_report.honest_nodes_converged, 99);
    assert_eq!(closure_report.byzantine_bans_recorded, 99);
    assert!(closure_report.light_client_verified);
    for node in stand.nodes().iter().filter(|n| !n.is_byzantine) {
        assert_eq!(node.finalized_height, s01.target_heights);
        assert_eq!(
            node.finalized_block_hash,
            closure_report.finalized_block_hash
        );
        assert_eq!(
            node.finalized_state_root,
            closure_report.finalized_state_root
        );
    }
}
