//! Unauthenticated handshake data must not ban an arbitrary claimed PeerId.
//! All identities in these live loopback tests are public test fixtures.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use aura_p2p::ban::BanReason;
use aura_p2p::error::P2pError;
use aura_p2p::identity::{PeerId, PeerIdentity};
use aura_p2p::transport::{HandshakeHello, SwarmConfig, TransportKind, TransportSwarm};

const CHAIN: &str = "auralite-test-1";

fn spawn_swarm(seed: u8) -> TransportSwarm {
    let identity = PeerIdentity::from_seed([seed; 32]);
    TransportSwarm::bind(identity, SwarmConfig::for_local_test(CHAIN)).unwrap()
}

fn victim_and_public(seed: u8) -> (TransportSwarm, PeerId, [u8; 32]) {
    let identity = PeerIdentity::from_seed([seed; 32]);
    let id = identity.peer_id();
    let public = *identity.public_key();
    let swarm = TransportSwarm::bind(identity, SwarmConfig::for_local_test(CHAIN)).unwrap();
    (swarm, id, public)
}

fn send_frame(stream: &mut TcpStream, data: &[u8]) {
    stream
        .write_all(&(data.len() as u32).to_le_bytes())
        .unwrap();
    stream.write_all(data).unwrap();
    stream.flush().unwrap();
}

fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut len = [0; 4];
    stream.read_exact(&mut len).unwrap();
    let len = u32::from_le_bytes(len) as usize;
    assert!(len <= 65536 + 64);
    let mut data = vec![0; len];
    stream.read_exact(&mut data).unwrap();
    data
}

fn wait_for_close(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let mut byte = [0];
    match stream.read(&mut byte) {
        Ok(0) => {}
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("expected peer to close the unauthenticated connection: {result:?}"),
    }
}

#[test]
fn inbound_claimed_id_mismatch_cannot_ban_victim() {
    let node = spawn_swarm(40);
    let (victim, victim_id, _) = victim_and_public(41);
    let attacker = PeerIdentity::from_seed([42; 32]);

    // The attacker needs only the victim's public PeerId, not its key.
    let (mut hello, _) = HandshakeHello::new(&attacker, CHAIN, TransportKind::TcpNoise, 1);
    hello.claimed_peer_id = victim_id;
    let mut stream = TcpStream::connect(node.local_addr()).unwrap();
    send_frame(&mut stream, &hello.encode().unwrap());
    wait_for_close(&mut stream);
    assert!(
        !node.is_peer_banned(&victim_id, 0),
        "a claimed ID with the wrong public key is not authenticated"
    );
    victim
        .dial(node.local_addr(), Some(node.local_peer_id()))
        .expect("victim can still connect");
}

#[test]
fn inbound_invalid_signature_cannot_ban_victim() {
    let node = spawn_swarm(43);
    let (victim, victim_id, victim_public) = victim_and_public(44);
    let attacker = PeerIdentity::from_seed([45; 32]);

    // The hello is internally consistent, but the attacker cannot sign for
    // the victim's key. Send an invalid transcript signature after the reply.
    let (mut hello, _) = HandshakeHello::new(&attacker, CHAIN, TransportKind::TcpNoise, 1);
    hello.claimed_peer_id = victim_id;
    hello.public_key = victim_public;
    let mut stream = TcpStream::connect(node.local_addr()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    send_frame(&mut stream, &hello.encode().unwrap());
    let _responder_hello = read_frame(&mut stream);
    send_frame(&mut stream, &[0; 64]);
    wait_for_close(&mut stream);
    assert!(
        !node.is_peer_banned(&victim_id, 0),
        "an invalid signature is not an authenticated victim"
    );
    victim
        .dial(node.local_addr(), Some(node.local_peer_id()))
        .expect("victim can still connect");
}

#[test]
fn outbound_invalid_signature_cannot_ban_claimed_victim() {
    let node = spawn_swarm(50);
    let (victim, victim_id, victim_public) = victim_and_public(51);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    // The fake server only receives the victim's public data.
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        let _init_hello = read_frame(&mut stream);
        let attacker = PeerIdentity::from_seed([52; 32]);
        let (mut hello, _) = HandshakeHello::new(&attacker, CHAIN, TransportKind::TcpNoise, 1);
        hello.claimed_peer_id = victim_id;
        hello.public_key = victim_public;
        send_frame(&mut stream, &hello.encode().unwrap());
        let _init_signature = read_frame(&mut stream);
        send_frame(&mut stream, &[0; 64]);
    });
    assert_eq!(
        node.dial(addr, Some(victim_id)).unwrap_err(),
        P2pError::InvalidHandshakeSignature
    );
    server.join().unwrap();
    assert!(
        !node.is_peer_banned(&victim_id, 0),
        "bad responder signature cannot identify a victim as malicious"
    );
    node.dial(victim.local_addr(), Some(victim_id))
        .expect("victim can still be dialed");
}

#[test]
fn inbound_peer_ban_is_checked_after_valid_signature() {
    let node = spawn_swarm(58);
    let banned_peer = spawn_swarm(59);
    let banned_id = banned_peer.local_peer_id();
    node.ban_peer(
        banned_id,
        BanReason::Manual("test_authenticated_peer_ban".into()),
        None,
    );

    // The valid remote signature is verified before the receiver consults its
    // PeerId ban list. The banned peer is then rejected and never registered.
    assert!(matches!(
        banned_peer.dial(node.local_addr(), None),
        Err(P2pError::Io(_))
    ));
    assert_eq!(node.peer_count(), 0);
}

#[test]
fn outbound_authenticated_substitution_does_not_auto_ban_other_peer() {
    let node = spawn_swarm(60);
    let other = spawn_swarm(61);
    let expected_id = PeerIdentity::from_seed([62; 32]).peer_id();
    let other_id = other.local_peer_id();

    // Even a correctly authenticated different endpoint may be an honest peer;
    // reject this dial, but do not treat identity mismatch alone as ban evidence.
    assert_eq!(
        node.dial(other.local_addr(), Some(expected_id))
            .unwrap_err(),
        P2pError::IdentitySubstitution {
            expected: expected_id,
            actual: other_id,
        }
    );
    assert!(
        !node.is_peer_banned(&other_id, 0),
        "a valid but unexpected peer is not automatically malicious"
    );
}
