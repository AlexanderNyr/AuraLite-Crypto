//! N3 discovery gate: 30 nodes behind NAT find each other with no central server
//! using Kademlia DHT iterative lookup, UPnP port mappings, and Circuit Relay v2.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use aura_p2p::ban::BanList;
use aura_p2p::discovery::{
    BootstrapList, DiscoveryNetwork, KADEMLIA_K, KademliaTable, NatStatus, SignedPeerRecord,
    UpnpMapper, XorDistance,
};
use aura_p2p::error::P2pError;
use aura_p2p::identity::PeerIdentity;

const CHAIN: &str = "auralite-devnet-1";
const NOW_MS: u64 = 1_700_000_000_000;

#[test]
fn thirty_nodes_behind_nat_discover_each_other_with_no_central_server() {
    let mut net = DiscoveryNetwork::new_natted_mesh(CHAIN, 30, 5, NOW_MS)
        .expect("30-node NATed mesh initializes");

    // Initially, each node knows at most 2 neighbors (no central server).
    for node in &net.nodes {
        assert!(
            node.table.len() <= 2,
            "initial bootstrap view must be sparse (<= 2 peers), got {}",
            node.table.len()
        );
    }

    let rounds = net
        .run_iterative_discovery(6, NOW_MS)
        .expect("iterative Kademlia discovery succeeds");
    assert!(
        rounds <= 4,
        "30-node Kademlia swarm should converge in <= 4 rounds, took {rounds}"
    );

    // Every one of the 30 nodes must hold verified records for all 29 other peers.
    for (idx, node) in net.nodes.iter().enumerate() {
        assert_eq!(
            node.table.len(),
            29,
            "node {idx} discovered {}/29 peers",
            node.table.len()
        );
        for (other_idx, other) in net.nodes.iter().enumerate() {
            if idx == other_idx {
                continue;
            }
            let rec = node
                .table
                .get_record(&other.identity.peer_id())
                .expect("peer record present");
            rec.verify(CHAIN).expect("stored record is valid");
        }
    }

    // Verify end-to-end delivery between symmetric-NAT nodes over Circuit Relay v2
    // and between symmetric-NAT and UPnP-mapped cone-NAT nodes.
    let msg = b"n3-nat-traversal-frame";
    let delivered_sym_to_sym = net
        .route_message(29, 17, msg, NOW_MS)
        .expect("symmetric-NAT node 29 reaches symmetric-NAT node 17 via relay");
    assert_eq!(delivered_sym_to_sym, msg);

    let delivered_sym_to_upnp = net
        .route_message(25, 2, msg, NOW_MS)
        .expect("symmetric-NAT node 25 reaches UPnP-mapped node 2");
    assert_eq!(delivered_sym_to_upnp, msg);
}

#[test]
fn bootstrap_list_upnp_and_kademlia_security_invariants() {
    let mut bootstrap = BootstrapList::new(CHAIN);
    assert!(bootstrap.is_empty());

    let boot_id = PeerIdentity::from_seed([0xB1; 32]);
    let internal = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 30_300);
    let mapper = UpnpMapper::new(true, IpAddr::V4(Ipv4Addr::new(198, 51, 100, 10)));
    assert_eq!(
        mapper.request_mapping(internal, 0),
        Err(P2pError::InvalidUpnpPort(0))
    );
    let nat = mapper.request_mapping(internal, 40_300).unwrap();
    assert!(matches!(nat, NatStatus::UpnpMapped { .. }));

    let boot_rec = SignedPeerRecord::sign(&boot_id, CHAIN, 1, vec![internal], nat);
    bootstrap.add_bootstrap(boot_rec.clone()).unwrap();
    assert_eq!(bootstrap.len(), 1);

    // Cross-chain bootstrap record must be rejected.
    let wrong_chain = SignedPeerRecord::sign(
        &boot_id,
        "other-chain-99",
        2,
        vec![internal],
        NatStatus::Public,
    );
    assert!(matches!(
        bootstrap.add_bootstrap(wrong_chain),
        Err(P2pError::ChainIdMismatch { .. })
    ));

    // Verify XOR closest_peers ordering in KademliaTable.
    let local = PeerIdentity::from_seed([0xAA; 32]);
    let mut table = KademliaTable::new(local.peer_id(), CHAIN, KADEMLIA_K);
    let ban = BanList::new();
    for s in 1..=10u8 {
        let peer = PeerIdentity::from_seed([s; 32]);
        let rec = SignedPeerRecord::sign(&peer, CHAIN, 1, vec![internal], NatStatus::Public);
        table.upsert_record(rec, &ban, NOW_MS).unwrap();
    }
    let target = PeerIdentity::from_seed([0x55; 32]).peer_id();
    let closest = table.closest_peers(&target, 5);
    assert_eq!(closest.len(), 5);
    for pair in closest.windows(2) {
        let d0 = XorDistance::between(&pair[0].peer_id, &target);
        let d1 = XorDistance::between(&pair[1].peer_id, &target);
        assert!(
            d0 <= d1,
            "closest_peers must be sorted by ascending XOR distance"
        );
    }
}
