//! N1 transport gate: 10 nodes connect over live TCP with Noise-inspired
//! transcript authentication, and identity substitution / banned peers are rejected.

use aura_p2p::ban::BanReason;
use aura_p2p::error::P2pError;
use aura_p2p::identity::{PeerId, PeerIdentity};
use aura_p2p::transport::{SwarmConfig, TransportSwarm};

const CHAIN: &str = "auralite-test-1";

fn spawn_node(index: u8) -> TransportSwarm {
    let identity = PeerIdentity::from_seed([index.wrapping_add(1); 32]);
    let config = SwarmConfig::for_local_test(CHAIN);
    TransportSwarm::bind(identity, config).expect("swarm binds on 127.0.0.1:0")
}

#[test]
fn ten_nodes_connect_and_authenticate_over_tcp_swarm() {
    let mut nodes: Vec<TransportSwarm> = (0..10).map(spawn_node).collect();
    let addrs: Vec<_> = nodes.iter().map(TransportSwarm::local_addr).collect();
    let ids: Vec<_> = nodes.iter().map(TransportSwarm::local_peer_id).collect();

    // Node 0 dials all 9 other nodes (1..10) with explicit expected PeerId verification.
    for target in 1..10 {
        let connected = nodes[0]
            .dial(addrs[target], Some(ids[target]))
            .expect("authenticated handshake succeeds");
        assert_eq!(connected.peer_id, ids[target]);
        assert_eq!(
            PeerId::from_public_key(&connected.public_key),
            ids[target],
            "peer_id must equal BLAKE3(pubkey)"
        );
    }

    // Also form a ring across nodes 1..9 so every node initiates and accepts connections.
    for (i, node) in nodes.iter().enumerate().skip(1) {
        let next = (i + 1) % 10;
        let connected = node
            .dial(addrs[next], Some(ids[next]))
            .expect("ring dial succeeds");
        assert_eq!(connected.peer_id, ids[next]);
    }

    assert_eq!(
        nodes[0].peer_count(),
        9,
        "node 0 is connected to all 9 other nodes in the 10-node swarm"
    );
    for (idx, node) in nodes.iter().enumerate() {
        assert!(
            node.peer_count() >= 2,
            "node {idx} has at least 2 authenticated peers"
        );
        for peer in node.connected_peers() {
            assert!(peer.peer_id.verify_public_key(&peer.public_key).is_ok());
        }
    }

    for node in &mut nodes {
        node.shutdown();
    }
}

#[test]
fn identity_substitution_and_banned_peers_are_rejected_on_live_swarm() {
    let mut node_a = spawn_node(20);
    let mut node_b = spawn_node(21);
    let mut node_c = spawn_node(22);

    // Node A dials Node C's address while expecting Node B's PeerId -> IdentitySubstitution.
    let err = node_a
        .dial(node_c.local_addr(), Some(node_b.local_peer_id()))
        .expect_err("substituting Node C for expected Node B must be rejected");
    assert_eq!(
        err,
        P2pError::IdentitySubstitution {
            expected: node_b.local_peer_id(),
            actual: node_c.local_peer_id(),
        }
    );

    // A wrong endpoint is rejected but never auto-banned; it may be a legitimate
    // unrelated node. Identity-based bans remain an explicit local policy action.
    assert!(!node_a.is_peer_banned(&node_c.local_peer_id(), 0));
    node_a.ban_peer(
        node_c.local_peer_id(),
        BanReason::Manual("test_peer_ban".into()),
        None,
    );
    let banned_err = node_a
        .dial(node_c.local_addr(), Some(node_c.local_peer_id()))
        .expect_err("explicitly banned peer must be rejected before handshake completes");
    assert!(matches!(banned_err, P2pError::PeerBanned { .. }));

    // Self-connection is rejected.
    let self_err = node_a
        .dial(node_a.local_addr(), None)
        .expect_err("self-connection must be rejected");
    assert_eq!(self_err, P2pError::SelfConnection(node_a.local_peer_id()));

    // Honest connection between Node A and Node B still succeeds.
    let ok_peer = node_a
        .dial(node_b.local_addr(), Some(node_b.local_peer_id()))
        .expect("honest peer B connects");
    assert_eq!(ok_peer.peer_id, node_b.local_peer_id());

    // Explicit IP ban evicts and blocks subsequent connections from that IP.
    node_a.ban_ip(
        node_b.local_addr().ip(),
        BanReason::Manual("test_ip_ban".into()),
        None,
    );
    assert_eq!(node_a.peer_count(), 0);
    let ip_err = node_a
        .dial(node_b.local_addr(), Some(node_b.local_peer_id()))
        .expect_err("banned IP must be rejected");
    assert!(matches!(ip_err, P2pError::IpBanned { .. }));

    node_a.shutdown();
    node_b.shutdown();
    node_c.shutdown();
}
