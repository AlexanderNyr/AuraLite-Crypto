//! H5 Jepsen-like runs (docs/h5-consensus-plan.md §2, D-H5-4).
//!
//! The N8 chaos matrix (20 scenarios × 3 runs: latency/loss/partition/
//! eclipse) must show zero finalized-head divergence, and the divergence
//! monitor must actually fire on a crafted divergent cluster — a monitor
//! that never rejects is not a monitor.
//! `tools/nc_h5.py` NC-5 disables the monitor; NC-6 truncates the matrix.

use aura_p2p::chaos::{
    ChaosNodeState, DEFAULT_STAND_NODES, canonical_20_scenarios, run_chaos_matrix,
    verify_cluster_convergence,
};
use aura_p2p::dos_defence::{DosDefenceEngine, DosDefenceLimits};
use aura_p2p::identity::PeerId;

fn crafted_node(index: usize, height: u64, block: [u8; 32], root: [u8; 32]) -> ChaosNodeState {
    let mut key = [0u8; 32];
    key[0] = index as u8 + 1;
    ChaosNodeState {
        index,
        peer_id: PeerId::from_public_key(&key),
        is_byzantine: false,
        finalized_height: height,
        finalized_block_hash: block,
        finalized_state_root: root,
        dos_engine: DosDefenceEngine::new("h5-chain", DosDefenceLimits::default()),
        packets_dropped: 0,
        heights_synced_on_heal: 0,
        eclipse_headers_rejected: 0,
    }
}

#[test]
fn h5_chaos_campaign_zero_divergence() {
    let scenarios = canonical_20_scenarios();
    assert_eq!(
        scenarios.len(),
        20,
        "H5: chaos campaign must cover 20 scenarios"
    );
    let summary = run_chaos_matrix("h5-chain", &scenarios, 3)
        .expect("H5: chaos campaign must cover 20 scenarios");
    assert_eq!(
        summary.total_runs, 60,
        "H5: chaos campaign must cover 20 scenarios"
    );
    assert_eq!(
        summary.scenarios_count * summary.runs_per_scenario,
        summary.total_runs,
        "H5: chaos campaign must cover 20 scenarios"
    );
    assert_eq!(
        summary.divergent_runs, 0,
        "H5: zero finalized-head divergence required"
    );
}

#[test]
fn h5_divergence_monitor_fires() {
    let _ = DEFAULT_STAND_NODES;
    // A crafted cluster where two honest nodes finalize different heads at
    // the same height must be rejected.
    let nodes = vec![
        crafted_node(0, 10, [1u8; 32], [2u8; 32]),
        crafted_node(1, 10, [9u8; 32], [2u8; 32]),
    ];
    assert!(
        verify_cluster_convergence("h5-crafted", &nodes, 10).is_err(),
        "H5: divergence monitor must fire"
    );
    // Sensitivity both ways: a convergent cluster must pass.
    let converged = vec![
        crafted_node(0, 10, [1u8; 32], [2u8; 32]),
        crafted_node(1, 10, [1u8; 32], [2u8; 32]),
    ];
    assert!(
        verify_cluster_convergence("h5-converged", &converged, 10).is_ok(),
        "H5: divergence monitor must fire"
    );
}
