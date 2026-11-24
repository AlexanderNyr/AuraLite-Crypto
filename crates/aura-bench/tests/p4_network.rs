//! P4 network gates (PERF_PLAN P4 DoD).
//!
//! DoD: **100 nodes: traffic/validator within budget** — the budget
//! is published with its methodology (`docs/perf/p4-network-report.md`,
//! D-P3). Each of the four optimizations carries a before/after pair
//! (D-P2): vote batching, compression, gossipsub parameters, mesh
//! tuning. NC: revert → red (`tools/nc_p4.py`, six named messages).

use aura_bench::p4::{
    BUDGET_BYTES, D_AFTER, GOSSIP_FACTOR_AFTER, N_NODES, P4Error, P4Pairs, P4Report, build_mesh,
    gate_budget, gate_compression, gate_gossip_params, gate_mesh, gate_vote_batching, ledger_state,
    mesh_connected, msg_bytes, overhead_bytes, per_validator_bytes, proposal_wire_after,
    proposal_wire_before, rle_decode, rle_encode, votes_wire_after, votes_wire_before,
};

/// Golden numbers pinned before the first run (2026-11-24): 100
/// validators at 1 view/s; baseline 334 848 B/s per validator (one
/// envelope per vote, raw proposal, gossip 25%, D=8) honestly fails
/// the published budget of 125 000 B/s (1 Mbit/s); P4 path 86 495 B/s
/// (batched votes, RLE proposal, gossip 10%, D=5) fits with headroom;
/// report digest
/// `ab618903bda0b4dc15705a228a0cd9992e6f76ecf0c20bfe8bf8463281ce1108`.
#[test]
fn p4_golden_numbers() {
    let report = P4Report::build(5);
    assert_eq!(report.runs, 5);
    assert_eq!((report.p50, report.p95, report.p99), (86495, 86495, 86495));
    assert_eq!(report.total, 432475);
    assert_eq!(report.spread_pct, 0);
    assert_eq!(report.before, 334848);
    assert_eq!(report.after, 86495);
    assert_eq!(report.budget, 125000);
    assert_eq!(report.reduction_pct, 74);
    assert_eq!(report.pairs.votes, (19200, 13632));
    assert_eq!(report.pairs.proposal, (8512, 592));
    assert_eq!(report.pairs.overhead, (13952, 2883));
    assert_eq!(report.pairs.mesh, (8, 5));
    assert_eq!(report.pairs.traffic, (334848, 86495));
    assert_eq!(
        report.workload_digest,
        "3b105a693a17bddf29ae5fa2f583cdd0982131e5155bfa1078bddf6e598debf6"
    );
    assert_eq!(
        report.bench_digest,
        "ab618903bda0b4dc15705a228a0cd9992e6f76ecf0c20bfe8bf8463281ce1108"
    );
    assert!(report.validate().is_ok());
}

/// The P4 DoD, asserted literally: 100 nodes, per-validator gossip
/// traffic within the published budget; the baseline fails honestly.
#[test]
fn p4_do_d_traffic_within_budget() {
    assert_eq!(N_NODES, 100);
    assert_eq!(BUDGET_BYTES, 125_000);
    let traffic = gate_budget().expect("P4 DoD");
    assert_eq!(traffic, 86_495);
    assert_eq!(P4Pairs::compute().traffic.0, 334_848);
    assert!(
        P4Pairs::compute().traffic.0 > BUDGET_BYTES,
        "the unoptimized path fails the budget honestly"
    );
}

/// The six named P4 messages (NC targets): exact Display literals.
#[test]
fn p4_named_messages() {
    assert_eq!(
        P4Error::BudgetExceeded {
            bytes: 334848,
            budget: 125000
        }
        .to_string(),
        "P4: traffic/validator exceeds budget (334848/125000)"
    );
    assert_eq!(
        P4Error::BatchingWinReverted {
            before: 19200,
            after: 19200
        }
        .to_string(),
        "P4: vote batching win reverted (19200/19200)"
    );
    assert_eq!(
        P4Error::CompressionRoundtrip { byte: 7 }.to_string(),
        "P4: compression roundtrip mismatch (byte 7)"
    );
    assert_eq!(
        P4Error::ParamsInconsistent { d: 5, lo: 3, hi: 4 }.to_string(),
        "P4: gossip parameter set inconsistent (D=5/3/4)"
    );
    assert_eq!(
        P4Error::MeshDisconnected { nodes: 100 }.to_string(),
        "P4: mesh disconnected at 100 nodes"
    );
    assert_eq!(
        P4Error::DeterminismViolated { run: 2 }.to_string(),
        "P4: determinism violated (run 2)"
    );
}

/// Vote batching changes the wire, never the votes: unpacking the
/// batches yields exactly the input votes in order, and the pair
/// 19 200 → 13 632 is the batching win.
#[test]
fn p4_vote_batching_equivalence() {
    let votes = aura_bench::p4::vote_fixture();
    let (before, after) = gate_vote_batching(&votes).expect("batching equivalence");
    assert_eq!((before, after), (19200, 13632));
    assert_eq!(votes_wire_before(100), 19200);
    assert_eq!(votes_wire_after(100, 8), 13632);
    assert_eq!(votes_wire_after(100, 1), 19200, "k=1 is the reverted state");

    // An empty and a partial batch still round-trip.
    let empty: Vec<(u64, Vec<u8>)> = Vec::new();
    assert!(gate_vote_batching(&empty).is_ok());
    let small: Vec<(u64, Vec<u8>)> = (0..5u64).map(|i| (i, vec![i as u8; 4])).collect();
    assert!(gate_vote_batching(&small).is_ok());
}

/// Compression is lossless (functional gate) and its pair
/// 8 512 → 592 is the codec win on the fixture.
#[test]
fn p4_compression_lossless() {
    let (before, after) = gate_compression().expect("lossless roundtrip");
    assert_eq!((before, after), (8512, 592));
    assert_eq!(proposal_wire_before(), 8512);
    assert_eq!(proposal_wire_after(), 592);

    let body = vec![1u8, 1, 1, 2, 3, 3];
    assert_eq!(rle_decode(&rle_encode(&body)), body);
    let plain = vec![7u8; 300];
    let pairs = rle_encode(&plain);
    assert_eq!(pairs, vec![(7u8, 255u8), (7, 45)], "runs cap at 255");
    assert_eq!(rle_decode(&pairs), plain);
}

/// Gossipsub parameters: the tuned set satisfies the spec invariants
/// (D_low ≤ D ≤ D_high, sane heartbeat/mcache) and cuts advertisement
/// overhead 13 952 → 2 883.
#[test]
fn p4_gossip_params_tuned() {
    let (before, after) = gate_gossip_params().expect("tuned parameter set");
    assert_eq!((before, after), (13952, 2883));
    assert_eq!(overhead_bytes(25, 27904), 13952);
    assert_eq!(overhead_bytes(10, 14416), 2883);

    // The message bytes pair feeds the overhead pair deterministically.
    let msg_b = msg_bytes(19200, 8512);
    let msg_a = msg_bytes(13632, 592);
    assert_eq!((msg_b, msg_a), (27904, 14416));
}

/// Mesh tuning: the tuned degree-5 mesh connects the 100-node stand
/// and every node has exactly 5 peers.
#[test]
fn p4_mesh_connects_stand() {
    let (before, after) = gate_mesh().expect("connected mesh");
    assert_eq!((before, after), (8, 5));
    let adj = build_mesh(N_NODES, D_AFTER);
    assert_eq!(adj.len(), 100);
    for peers in &adj {
        assert_eq!(peers.len(), 5);
    }
    assert!(mesh_connected(&adj));
    let one = build_mesh(1, 1);
    assert!(mesh_connected(&one), "trivial mesh");
}

/// Metamorphic scaling: traffic scales linearly with the mesh degree;
/// the per-validator formula is degree × (message + overhead).
#[test]
fn p4_scale_traffic() {
    let msg = 14416u64;
    let over = 2883u64;
    assert_eq!(per_validator_bytes(5, msg, over), 86495);
    assert_eq!(per_validator_bytes(10, msg, over), 172990, "linear in D");
    assert_eq!(
        per_validator_bytes(8, 27904, 13952),
        334848,
        "the baseline formula reproduces 334 848"
    );
}

/// Conservative sensitivity (published in the report): if real blocks
/// compress at only 2:1, the tuned path still fits the budget.
#[test]
fn p4_sensitivity_two_to_one_fits() {
    let votes = votes_wire_after(N_NODES, 8);
    let proposal_2to1 = (8192 + 256) / 2 + 32 + 64;
    let msg = msg_bytes(votes, proposal_2to1);
    let over = overhead_bytes(GOSSIP_FACTOR_AFTER, msg);
    let traffic = per_validator_bytes(D_AFTER, msg, over);
    assert_eq!(traffic, 108860);
    assert!(traffic <= BUDGET_BYTES, "2:1 sensitivity fits the budget");
}

/// X7 determinism: the report is a pure function of its inputs and
/// the ledger state is stable.
#[test]
fn p4_determinism() {
    let r1 = P4Report::build(5);
    let r2 = P4Report::build(5);
    assert_eq!(r1, r2);
    assert_eq!(r1.canonical(), r2.canonical());
    assert_eq!(r1.bench_digest, r2.bench_digest);
    let s1 = ledger_state();
    let s2 = ledger_state();
    assert_eq!(s1, s2);
    assert_eq!(s1.get("per_val"), Some(&86495));
    assert_eq!(s1.get("votes"), Some(&13632));
}

/// NC: revert → red. Each reverted optimization state fails with its
/// named message (the evidence for the D-P2 contract).
#[test]
fn p4_validate_reverts_are_red() {
    let good = P4Report::build(5);

    // Revert mesh tuning (D 5 → 8): traffic goes back over budget.
    let mut reverted_mesh = good.clone();
    reverted_mesh.after = 138392;
    reverted_mesh.p99 = 138392;
    assert_eq!(
        reverted_mesh.validate().unwrap_err(),
        P4Error::BudgetExceeded {
            bytes: 138392,
            budget: 125000
        }
    );

    // Revert vote batching (k 8 → 1): the batching win disappears.
    let mut reverted_batch = good.clone();
    reverted_batch.pairs.votes = (19200, 19200);
    assert_eq!(
        reverted_batch.validate().unwrap_err(),
        P4Error::BatchingWinReverted {
            before: 19200,
            after: 19200
        }
    );

    // Break determinism: one run differs.
    let mut nondet = good.clone();
    nondet.after = 86496;
    assert_eq!(
        nondet.validate().unwrap_err(),
        P4Error::DeterminismViolated { run: 1 }
    );
}

/// The state digest is stable across rebuilds (X7 evidence).
#[test]
fn p4_state_digest_stable() {
    let report = P4Report::build(5);
    let again = P4Report::build(5);
    assert_eq!(report.state_digest, again.state_digest);
    assert_eq!(report.state_digest.len(), 64);
}
