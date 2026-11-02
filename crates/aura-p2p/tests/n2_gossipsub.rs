//! N2 gossipsub gate: 1000 signed transactions broadcast across a 10-node mesh
//! in < 1 s with deduplication, and flooding/forging peers lose score and are banned.

use aura_p2p::error::P2pError;
use aura_p2p::gossipsub::{
    GossipEngine, GossipMesh, GossipMessage, GossipOutcome, GossipTopic, PeerScoreParams,
};
use aura_p2p::identity::{PeerId, PeerIdentity};
use std::time::Instant;

const CHAIN: &str = "auralite-devnet-1";

#[test]
fn thousand_txs_broadcast_across_ten_node_mesh_under_one_second() {
    let mut mesh = GossipMesh::new();
    let mut ids: Vec<PeerId> = Vec::with_capacity(10);

    for i in 0..10u8 {
        let identity = PeerIdentity::from_seed([i.wrapping_add(1); 32]);
        ids.push(identity.peer_id());
        mesh.add_node(GossipEngine::new(
            identity,
            CHAIN,
            PeerScoreParams::default(),
        ));
    }

    // Connect a ring plus chords (i <-> (i+1)%10 and i <-> (i+3)%10) so every
    // broadcast traverses redundant mesh links and exercises SeenCache dedup.
    for i in 0..10 {
        mesh.connect(ids[i], ids[(i + 1) % 10]);
        mesh.connect(ids[i], ids[(i + 3) % 10]);
    }

    let start = Instant::now();
    for tx_idx in 0..1000usize {
        let publisher = ids[tx_idx % 10];
        let payload = format!("signed-tx-{tx_idx:04}").into_bytes();
        let deliveries = mesh
            .broadcast(publisher, GossipTopic::Tx, payload, 1_000)
            .expect("broadcast succeeds");
        assert_eq!(
            deliveries, 9,
            "each unique tx is accepted once by each of the 9 remote peers"
        );
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "1000 txs broadcast across 10-node mesh took {elapsed:?} (>= 1.0s)"
    );

    // Every one of the 10 nodes must hold all 1000 validated transactions and
    // have deduplicated redundant chord deliveries.
    for peer_id in &ids {
        let node = mesh.node(peer_id).expect("node exists");
        assert_eq!(node.messages_for_topic(GossipTopic::Tx).len(), 1000);
        assert_eq!(node.seen_cache().len(), 1000);
    }
}

#[test]
fn flooding_and_forging_peers_lose_score_and_are_banned() {
    let params = PeerScoreParams {
        max_messages_per_window: 10,
        rate_window_ms: 1_000,
        flood_penalty: 20,
        invalid_message_penalty: 30,
        graylist_threshold: -60,
        ban_threshold: -50,
        ..PeerScoreParams::default()
    };
    let mut validator = GossipEngine::new(PeerIdentity::from_seed([1u8; 32]), CHAIN, params);
    let flooder = PeerIdentity::from_seed([2u8; 32]);
    let forger = PeerIdentity::from_seed([3u8; 32]);
    validator.register_peer(flooder.peer_id(), GossipTopic::ALL);
    validator.register_peer(forger.peer_id(), GossipTopic::ALL);

    // Flooder sends 10 valid messages (+10 score), then 3 rate-exceeding messages
    // (-20 each -> 10 -> -10 -> -30 -> -50 -> banned).
    for seq in 1..=13 {
        let msg = GossipMessage::sign(
            &flooder,
            CHAIN,
            GossipTopic::Tx,
            seq,
            format!("flood-{seq}").into_bytes(),
        )
        .unwrap();
        let outcome = validator.ingest(flooder.peer_id(), msg, 500);
        if seq <= 10 {
            assert!(matches!(outcome, GossipOutcome::Accepted { .. }));
        } else if seq < 13 {
            assert!(matches!(
                outcome,
                GossipOutcome::Rejected {
                    error: P2pError::GossipRateLimitExceeded { .. },
                    banned: false,
                    ..
                }
            ));
        } else {
            assert!(matches!(
                outcome,
                GossipOutcome::Rejected {
                    error: P2pError::GossipRateLimitExceeded { .. },
                    peer_score: -50,
                    banned: true,
                }
            ));
        }
    }
    assert!(validator.is_peer_banned(&flooder.peer_id(), 500));

    // Forger sends 2 messages with tampered signatures (-30 each -> -60 -> banned).
    for seq in 1..=2 {
        let mut forged = GossipMessage::sign(
            &forger,
            CHAIN,
            GossipTopic::Proposal,
            seq,
            b"forged-block".to_vec(),
        )
        .unwrap();
        forged.signature[0] ^= 0xAA;
        let _ = validator.ingest(forger.peer_id(), forged, 600);
    }
    assert!(validator.is_peer_banned(&forger.peer_id(), 600));
}
