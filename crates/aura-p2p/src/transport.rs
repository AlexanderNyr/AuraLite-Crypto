//! N1 authenticated P2P transport, Noise-inspired handshake, session encryption, and swarm.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use aura_crypto::{blake3_hash, constant_time_eq};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey};
use zeroize::Zeroize;

use crate::ban::{BanList, BanReason};
use crate::error::P2pError;
use crate::identity::{
    HandshakeRole, PEER_ID_LEN, PeerId, PeerIdentity, verify_transcript_signature,
};

/// Wire protocol version enforced during the N1 handshake.
pub const PROTOCOL_VERSION: u16 = 2;

/// Maximum wire frame length (64 KiB) enforced before allocation.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Maximum agent string length in `HandshakeHello`.
pub const MAX_AGENT_LEN: usize = 128;

/// Default agent string advertised by `aura-p2p`.
pub const DEFAULT_AGENT_VERSION: &str = "aura-p2p/0.1.0-n1";

/// Underlying transport profile advertised in the handshake (`NET_PLAN` D-N1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// TCP with signed X25519 ephemeral key agreement and AES-256-GCM framing.
    TcpNoise,
    /// QUIC profile identifier (same identity binding; transport is TCP in Stage 1).
    QuicNoise,
}

impl TransportKind {
    const fn to_byte(self) -> u8 {
        match self {
            Self::TcpNoise => 1,
            Self::QuicNoise => 2,
        }
    }

    fn from_byte(byte: u8) -> Result<Self, P2pError> {
        match byte {
            1 => Ok(Self::TcpNoise),
            2 => Ok(Self::QuicNoise),
            other => Err(P2pError::Codec(format!(
                "unknown transport kind byte {other}"
            ))),
        }
    }
}

/// Initial handshake frame exchanged by both initiator and responder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeHello {
    /// Wire protocol version (`PROTOCOL_VERSION`).
    pub protocol_version: u16,
    /// Transport profile (`TcpNoise` or `QuicNoise`).
    pub transport_kind: TransportKind,
    /// Network `chain_id` (e.g. `auralite-test-1`).
    pub chain_id: String,
    /// Claimed [`PeerId`], which must equal `BLAKE3(public_key)`.
    pub claimed_peer_id: PeerId,
    /// Peer's 32-byte Ed25519 public key.
    pub public_key: [u8; 32],
    /// Per-handshake X25519 ephemeral public key, bound into the signed transcript.
    pub ephemeral_public_key: [u8; 32],
    /// Bounded identify agent string (`aura-p2p/0.1.0-n1`).
    pub agent_version: String,
    /// Local listener port advertised for peer table identification.
    pub listen_port: u16,
}

impl HandshakeHello {
    /// Build a fresh [`HandshakeHello`] for `identity`.
    pub fn new(
        identity: &PeerIdentity,
        chain_id: impl Into<String>,
        transport_kind: TransportKind,
        listen_port: u16,
    ) -> (Self, EphemeralSecret) {
        let secret = EphemeralSecret::random();
        let ephemeral_public_key = X25519PublicKey::from(&secret).to_bytes();
        let hello = Self {
            protocol_version: PROTOCOL_VERSION,
            transport_kind,
            chain_id: chain_id.into(),
            claimed_peer_id: identity.peer_id(),
            public_key: *identity.public_key(),
            ephemeral_public_key,
            agent_version: DEFAULT_AGENT_VERSION.to_owned(),
            listen_port,
        };
        (hello, secret)
    }

    /// Encode this hello message into canonical length-prefixed bytes.
    pub fn encode(&self) -> Result<Vec<u8>, P2pError> {
        if self.agent_version.len() > MAX_AGENT_LEN {
            return Err(P2pError::Codec("agent_version exceeds limit".into()));
        }
        let chain_bytes = self.chain_id.as_bytes();
        let agent_bytes = self.agent_version.as_bytes();
        let mut out = Vec::with_capacity(128 + chain_bytes.len() + agent_bytes.len());
        out.extend_from_slice(&self.protocol_version.to_le_bytes());
        out.push(self.transport_kind.to_byte());
        out.extend_from_slice(&(chain_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(chain_bytes);
        out.extend_from_slice(self.claimed_peer_id.as_bytes());
        out.extend_from_slice(&self.public_key);
        out.extend_from_slice(&self.ephemeral_public_key);
        out.extend_from_slice(&(agent_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(agent_bytes);
        out.extend_from_slice(&self.listen_port.to_le_bytes());
        Ok(out)
    }

    /// Decode a [`HandshakeHello`] from wire bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, P2pError> {
        let mut cursor = 0usize;
        let protocol_version = read_u16(bytes, &mut cursor)?;
        let transport_kind = TransportKind::from_byte(read_u8(bytes, &mut cursor)?)?;
        let chain_len = read_u16(bytes, &mut cursor)? as usize;
        if chain_len == 0 || chain_len > 64 {
            return Err(P2pError::Codec(format!("invalid chain_id len {chain_len}")));
        }
        let chain_id = std::str::from_utf8(read_slice(bytes, &mut cursor, chain_len)?)
            .map_err(|_| P2pError::Codec("non-utf8 chain_id".into()))?
            .to_owned();
        let claimed_peer_id = PeerId::from_bytes(read_array::<PEER_ID_LEN>(bytes, &mut cursor)?);
        let public_key = read_array::<32>(bytes, &mut cursor)?;
        let ephemeral_public_key = read_array::<32>(bytes, &mut cursor)?;
        let agent_len = read_u16(bytes, &mut cursor)? as usize;
        if agent_len > MAX_AGENT_LEN {
            return Err(P2pError::Codec(format!(
                "agent_version len {agent_len} > {MAX_AGENT_LEN}"
            )));
        }
        let agent_version = std::str::from_utf8(read_slice(bytes, &mut cursor, agent_len)?)
            .map_err(|_| P2pError::Codec("non-utf8 agent_version".into()))?
            .to_owned();
        let listen_port = read_u16(bytes, &mut cursor)?;
        if cursor != bytes.len() {
            return Err(P2pError::Codec(
                "trailing bytes in HandshakeHello".to_owned(),
            ));
        }
        Ok(Self {
            protocol_version,
            transport_kind,
            chain_id,
            claimed_peer_id,
            public_key,
            ephemeral_public_key,
            agent_version,
            listen_port,
        })
    }
}

/// Validate the contents and local policy of an untrusted remote [`HandshakeHello`].
///
/// This function does not authenticate ownership of `claimed_peer_id`, consult or
/// mutate the peer ban list, or check the dialer's expected identity. Callers must
/// verify the transcript signature before treating the claimed identity as an
/// authenticated peer or consulting identity-based bans.
pub fn validate_remote_hello(
    local_peer_id: PeerId,
    local_chain_id: &str,
    local_protocol_version: u16,
    remote: &HandshakeHello,
) -> Result<(), P2pError> {
    if remote.claimed_peer_id == local_peer_id {
        return Err(P2pError::SelfConnection(local_peer_id));
    }
    if remote.protocol_version != local_protocol_version {
        return Err(P2pError::ProtocolVersionMismatch {
            local: local_protocol_version,
            remote: remote.protocol_version,
        });
    }
    if remote.chain_id != local_chain_id {
        return Err(P2pError::ChainIdMismatch {
            expected: local_chain_id.to_owned(),
            actual: remote.chain_id.clone(),
        });
    }
    remote.claimed_peer_id.verify_public_key(&remote.public_key)
}

/// Reject an authenticated remote identity that differs from the dialer's expected [`PeerId`].
///
/// A mismatch is not automatically banned: dialing the wrong endpoint can reach
/// an honest, unrelated peer, and a pre-signature identity claim is untrusted.
pub fn verify_expected_peer(expected: Option<PeerId>, actual: PeerId) -> Result<(), P2pError> {
    if let Some(expected_peer) = expected
        && !constant_time_eq(expected_peer.as_bytes(), actual.as_bytes())
    {
        return Err(P2pError::IdentitySubstitution {
            expected: expected_peer,
            actual,
        });
    }
    Ok(())
}

/// Compute the canonical 32-byte BLAKE3 digest over both sides' [`HandshakeHello`] messages.
pub fn compute_transcript_hash(
    initiator: &HandshakeHello,
    responder: &HandshakeHello,
) -> Result<[u8; 32], P2pError> {
    let init_bytes = initiator.encode()?;
    let resp_bytes = responder.encode()?;
    let mut buf = Vec::with_capacity(64 + init_bytes.len() + resp_bytes.len());
    buf.extend_from_slice(b"AuraLite-Crypto/p2p-transcript-v1\0");
    buf.extend_from_slice(&(init_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&init_bytes);
    buf.extend_from_slice(&(resp_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&resp_bytes);
    Ok(*blake3_hash(&buf).as_bytes())
}

/// Post-handshake directional AES-256-GCM cipher. Keys are derived from an
/// ephemeral X25519 shared secret and the signed handshake transcript.
pub struct SessionCipher {
    send_key: [u8; 32],
    recv_key: [u8; 32],
    send_seq: u64,
    recv_seq: u64,
    transcript_hash: [u8; 32],
}

impl SessionCipher {
    /// Derive a directional cipher from this side's ephemeral secret and the
    /// remote ephemeral public key. The secret is consumed and zeroized.
    pub fn new(
        transcript_hash: &[u8; 32],
        local_secret: EphemeralSecret,
        remote_public: &[u8; 32],
        role: HandshakeRole,
    ) -> Result<Self, P2pError> {
        let remote = X25519PublicKey::from(*remote_public);
        let shared = local_secret.diffie_hellman(&remote);
        if constant_time_eq(shared.as_bytes(), &[0u8; 32]) {
            return Err(P2pError::InvalidHandshakeSignature);
        }
        let hk = Hkdf::<Sha256>::new(Some(transcript_hash), shared.as_bytes());
        let mut okm = [0u8; 64];
        hk.expand(b"AuraLite-Crypto/p2p-session-v2", &mut okm)
            .map_err(|_| P2pError::InvalidHandshakeSignature)?;
        let mut i2r = [0u8; 32];
        let mut r2i = [0u8; 32];
        i2r.copy_from_slice(&okm[..32]);
        r2i.copy_from_slice(&okm[32..]);
        okm.zeroize();
        let (send_key, recv_key) = match role {
            HandshakeRole::Initiator => (i2r, r2i),
            HandshakeRole::Responder => (r2i, i2r),
        };
        Ok(Self {
            send_key,
            recv_key,
            send_seq: 0,
            recv_seq: 0,
            transcript_hash: *transcript_hash,
        })
    }

    /// Encrypt and authenticate `plaintext` with the next monotonic sequence number.
    pub fn seal_frame(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, P2pError> {
        if plaintext.len() > MAX_FRAME_BYTES {
            return Err(P2pError::FrameTooLarge {
                size: plaintext.len(),
                max: MAX_FRAME_BYTES,
            });
        }
        let seq = self.send_seq;
        self.send_seq = self
            .send_seq
            .checked_add(1)
            .ok_or(P2pError::FrameAuthenticationFailed)?;
        let nonce = frame_nonce(seq);
        let aad = frame_aad(&self.transcript_hash, seq);
        let cipher = Aes256Gcm::new_from_slice(&self.send_key)
            .map_err(|_| P2pError::FrameAuthenticationFailed)?;
        let ciphertext = cipher
            .encrypt(
                &nonce.into(),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| P2pError::FrameAuthenticationFailed)?;
        let mut frame = Vec::with_capacity(8 + ciphertext.len());
        frame.extend_from_slice(&seq.to_le_bytes());
        frame.extend_from_slice(&ciphertext);
        Ok(frame)
    }

    /// Authenticate and decrypt an incoming frame, enforcing strict sequence ordering.
    pub fn open_frame(&mut self, frame: &[u8]) -> Result<Vec<u8>, P2pError> {
        if frame.len() < 24 {
            return Err(P2pError::FrameAuthenticationFailed);
        }
        if frame.len() - 8 > MAX_FRAME_BYTES + 16 {
            return Err(P2pError::FrameTooLarge {
                size: frame.len() - 8,
                max: MAX_FRAME_BYTES + 16,
            });
        }
        let seq = u64::from_le_bytes(frame[..8].try_into().unwrap());
        if seq != self.recv_seq {
            return Err(P2pError::FrameAuthenticationFailed);
        }
        let nonce = frame_nonce(seq);
        let aad = frame_aad(&self.transcript_hash, seq);
        let cipher = Aes256Gcm::new_from_slice(&self.recv_key)
            .map_err(|_| P2pError::FrameAuthenticationFailed)?;
        let plaintext = cipher
            .decrypt(
                &nonce.into(),
                Payload {
                    msg: &frame[8..],
                    aad: &aad,
                },
            )
            .map_err(|_| P2pError::FrameAuthenticationFailed)?;
        self.recv_seq = self
            .recv_seq
            .checked_add(1)
            .ok_or(P2pError::FrameAuthenticationFailed)?;
        Ok(plaintext)
    }
}

impl Drop for SessionCipher {
    fn drop(&mut self) {
        self.send_key.zeroize();
        self.recv_key.zeroize();
    }
}

fn frame_nonce(seq: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&seq.to_le_bytes());
    nonce
}

fn frame_aad(transcript: &[u8; 32], seq: u64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(40);
    aad.extend_from_slice(transcript);
    aad.extend_from_slice(&seq.to_le_bytes());
    aad
}

/// Authenticated peer metadata recorded once the N1 handshake + encrypted identify check pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedPeer {
    /// Verified `PeerId = BLAKE3(public_key)`.
    pub peer_id: PeerId,
    /// Verified 32-byte Ed25519 public key.
    pub public_key: [u8; 32],
    /// Remote socket address observed on the transport connection.
    pub remote_addr: SocketAddr,
    /// Remote peer's advertised listen port.
    pub listen_port: u16,
    /// Remote peer's identify agent version.
    pub agent_version: String,
    /// Transport profile negotiated during handshake.
    pub transport_kind: TransportKind,
    /// Canonical 32-byte handshake transcript hash.
    pub transcript_hash: [u8; 32],
}

/// Configuration for a [`TransportSwarm`] node.
#[derive(Debug, Clone)]
pub struct SwarmConfig {
    /// Network `chain_id` required by handshake.
    pub chain_id: String,
    /// Socket address to bind (`127.0.0.1:0` for ephemeral test ports).
    pub bind_addr: SocketAddr,
    /// Transport profile to advertise (`TransportKind::TcpNoise`).
    pub transport_kind: TransportKind,
    /// Maximum number of concurrent authenticated peers.
    pub max_peers: usize,
    /// Per-socket read/write timeout.
    pub io_timeout: Duration,
}

impl SwarmConfig {
    /// Default bounded configuration for `chain_id` on `127.0.0.1:0`.
    pub fn for_local_test(chain_id: impl Into<String>) -> Self {
        Self {
            chain_id: chain_id.into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            transport_kind: TransportKind::TcpNoise,
            max_peers: 64,
            io_timeout: Duration::from_secs(2),
        }
    }
}

/// Live N1 transport swarm managing inbound/outbound authenticated peer connections and bans.
pub struct TransportSwarm {
    identity: Arc<PeerIdentity>,
    config: SwarmConfig,
    local_addr: SocketAddr,
    peers: Arc<Mutex<BTreeMap<PeerId, ConnectedPeer>>>,
    bans: Arc<Mutex<BanList>>,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl TransportSwarm {
    /// Bind the TCP listener and start the authenticated responder loop.
    pub fn bind(identity: PeerIdentity, config: SwarmConfig) -> Result<Self, P2pError> {
        let listener =
            TcpListener::bind(config.bind_addr).map_err(|e| P2pError::Io(e.to_string()))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| P2pError::Io(e.to_string()))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| P2pError::Io(e.to_string()))?;

        let identity = Arc::new(identity);
        let peers = Arc::new(Mutex::new(BTreeMap::new()));
        let bans = Arc::new(Mutex::new(BanList::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread_identity = Arc::clone(&identity);
        let thread_config = config.clone();
        let thread_peers = Arc::clone(&peers);
        let thread_bans = Arc::clone(&bans);
        let thread_shutdown = Arc::clone(&shutdown);
        let listen_port = local_addr.port();

        let accept_thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, remote_addr)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(thread_config.io_timeout));
                        let _ = stream.set_write_timeout(Some(thread_config.io_timeout));
                        let _ = handle_inbound_stream(
                            &mut stream,
                            remote_addr,
                            &thread_identity,
                            &thread_config,
                            listen_port,
                            &thread_peers,
                            &thread_bans,
                        );
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        });

        Ok(Self {
            identity,
            config,
            local_addr,
            peers,
            bans,
            shutdown,
            accept_thread: Some(accept_thread),
        })
    }

    /// Local [`PeerId`] of this swarm node.
    pub fn local_peer_id(&self) -> PeerId {
        self.identity.peer_id()
    }

    /// Local bound socket address of this swarm node.
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Dial a remote swarm node at `addr`, optionally requiring a specific `expected_peer` identity.
    pub fn dial(
        &self,
        addr: SocketAddr,
        expected_peer: Option<PeerId>,
    ) -> Result<ConnectedPeer, P2pError> {
        if addr == self.local_addr || expected_peer == Some(self.identity.peer_id()) {
            return Err(P2pError::SelfConnection(self.identity.peer_id()));
        }
        {
            let bans = self.bans.lock().unwrap();
            bans.check_ip(&addr.ip(), 0)?;
            if let Some(expected) = expected_peer {
                bans.check_peer(&expected, 0)?;
            }
        }
        {
            let peers = self.peers.lock().unwrap();
            if peers.len() >= self.config.max_peers {
                return Err(P2pError::MaxPeersReached {
                    max: self.config.max_peers,
                });
            }
        }

        let mut stream = TcpStream::connect_timeout(&addr, self.config.io_timeout)
            .map_err(|e| P2pError::Io(e.to_string()))?;
        stream
            .set_read_timeout(Some(self.config.io_timeout))
            .map_err(|e| P2pError::Io(e.to_string()))?;
        stream
            .set_write_timeout(Some(self.config.io_timeout))
            .map_err(|e| P2pError::Io(e.to_string()))?;

        let (init_hello, init_secret) = HandshakeHello::new(
            &self.identity,
            &self.config.chain_id,
            self.config.transport_kind,
            self.local_addr.port(),
        );
        write_raw_frame(&mut stream, &init_hello.encode()?)?;
        let resp_bytes = read_raw_frame(&mut stream)?;
        let resp_hello = HandshakeHello::decode(&resp_bytes)?;

        validate_remote_hello(
            self.identity.peer_id(),
            &self.config.chain_id,
            PROTOCOL_VERSION,
            &resp_hello,
        )?;

        let transcript_hash = compute_transcript_hash(&init_hello, &resp_hello)?;
        let init_sig = self.identity.sign_transcript(
            &self.config.chain_id,
            HandshakeRole::Initiator,
            &transcript_hash,
        );
        write_raw_frame(&mut stream, &init_sig)?;

        let resp_sig_vec = read_raw_frame(&mut stream)?;
        if resp_sig_vec.len() != 64 {
            return Err(P2pError::InvalidHandshakeSignature);
        }
        let mut resp_sig = [0u8; 64];
        resp_sig.copy_from_slice(&resp_sig_vec);
        verify_transcript_signature(
            &resp_hello.public_key,
            &self.config.chain_id,
            HandshakeRole::Responder,
            &transcript_hash,
            &resp_sig,
        )?;

        // The signed transcript is the first proof that the remote controls the
        // key associated with this PeerId. Only now may identity-based bans or
        // expected-peer policy be applied.
        self.bans
            .lock()
            .unwrap()
            .check_peer(&resp_hello.claimed_peer_id, 0)?;
        verify_expected_peer(expected_peer, resp_hello.claimed_peer_id)?;

        let mut initiator_cipher = SessionCipher::new(
            &transcript_hash,
            init_secret,
            &resp_hello.ephemeral_public_key,
            HandshakeRole::Initiator,
        )?;
        let ping = initiator_cipher.seal_frame(b"identify:ping")?;
        write_raw_frame(&mut stream, &ping)?;
        let pong_encrypted = read_raw_frame(&mut stream)?;
        let pong = initiator_cipher.open_frame(&pong_encrypted)?;
        if pong != b"identify:pong" {
            return Err(P2pError::FrameAuthenticationFailed);
        }

        let connected = ConnectedPeer {
            peer_id: resp_hello.claimed_peer_id,
            public_key: resp_hello.public_key,
            remote_addr: addr,
            listen_port: resp_hello.listen_port,
            agent_version: resp_hello.agent_version,
            transport_kind: resp_hello.transport_kind,
            transcript_hash,
        };
        self.peers
            .lock()
            .unwrap()
            .insert(connected.peer_id, connected.clone());
        Ok(connected)
    }

    /// Snapshot of all currently authenticated peers in deterministic `PeerId` order.
    pub fn connected_peers(&self) -> Vec<ConnectedPeer> {
        self.peers.lock().unwrap().values().cloned().collect()
    }

    /// Number of currently authenticated peers.
    pub fn peer_count(&self) -> usize {
        self.peers.lock().unwrap().len()
    }

    /// Ban a [`PeerId`] and evict it from the active peer table if present.
    pub fn ban_peer(&self, peer_id: PeerId, reason: BanReason, expires_at_ms: Option<u64>) {
        self.peers.lock().unwrap().remove(&peer_id);
        self.bans
            .lock()
            .unwrap()
            .ban_peer(peer_id, reason, expires_at_ms);
    }

    /// Ban an [`IpAddr`] and evict any connected peers from that IP.
    pub fn ban_ip(&self, ip: IpAddr, reason: BanReason, expires_at_ms: Option<u64>) {
        self.peers
            .lock()
            .unwrap()
            .retain(|_, peer| peer.remote_addr.ip() != ip);
        self.bans.lock().unwrap().ban_ip(ip, reason, expires_at_ms);
    }

    /// True when `peer_id` is currently banned at `now_ms`.
    pub fn is_peer_banned(&self, peer_id: &PeerId, now_ms: u64) -> bool {
        self.bans.lock().unwrap().is_peer_banned(peer_id, now_ms)
    }

    /// Stop the listener thread and close the swarm.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for TransportSwarm {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn handle_inbound_stream(
    stream: &mut TcpStream,
    remote_addr: SocketAddr,
    identity: &PeerIdentity,
    config: &SwarmConfig,
    listen_port: u16,
    peers: &Mutex<BTreeMap<PeerId, ConnectedPeer>>,
    bans: &Mutex<BanList>,
) -> Result<(), P2pError> {
    {
        let bans_guard = bans.lock().unwrap();
        bans_guard.check_ip(&remote_addr.ip(), 0)?;
    }
    {
        let peers_guard = peers.lock().unwrap();
        if peers_guard.len() >= config.max_peers {
            return Err(P2pError::MaxPeersReached {
                max: config.max_peers,
            });
        }
    }

    let init_bytes = read_raw_frame(stream)?;
    let init_hello = HandshakeHello::decode(&init_bytes)?;
    validate_remote_hello(
        identity.peer_id(),
        &config.chain_id,
        PROTOCOL_VERSION,
        &init_hello,
    )?;

    let (resp_hello, resp_secret) = HandshakeHello::new(
        identity,
        &config.chain_id,
        config.transport_kind,
        listen_port,
    );
    write_raw_frame(stream, &resp_hello.encode()?)?;

    let transcript_hash = compute_transcript_hash(&init_hello, &resp_hello)?;
    let init_sig_vec = read_raw_frame(stream)?;
    if init_sig_vec.len() != 64 {
        return Err(P2pError::InvalidHandshakeSignature);
    }
    let mut init_sig = [0u8; 64];
    init_sig.copy_from_slice(&init_sig_vec);
    verify_transcript_signature(
        &init_hello.public_key,
        &config.chain_id,
        HandshakeRole::Initiator,
        &transcript_hash,
        &init_sig,
    )?;

    // Only a valid transcript signature authenticates the claimed identity.
    bans.lock()
        .unwrap()
        .check_peer(&init_hello.claimed_peer_id, 0)?;

    let resp_sig =
        identity.sign_transcript(&config.chain_id, HandshakeRole::Responder, &transcript_hash);
    write_raw_frame(stream, &resp_sig)?;

    let mut responder_cipher = SessionCipher::new(
        &transcript_hash,
        resp_secret,
        &init_hello.ephemeral_public_key,
        HandshakeRole::Responder,
    )?;
    let ping_encrypted = read_raw_frame(stream)?;
    let ping = responder_cipher.open_frame(&ping_encrypted)?;
    if ping != b"identify:ping" {
        return Err(P2pError::FrameAuthenticationFailed);
    }
    let pong = responder_cipher.seal_frame(b"identify:pong")?;
    write_raw_frame(stream, &pong)?;

    let connected = ConnectedPeer {
        peer_id: init_hello.claimed_peer_id,
        public_key: init_hello.public_key,
        remote_addr,
        listen_port: init_hello.listen_port,
        agent_version: init_hello.agent_version,
        transport_kind: init_hello.transport_kind,
        transcript_hash,
    };
    peers.lock().unwrap().insert(connected.peer_id, connected);
    Ok(())
}

fn write_raw_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), P2pError> {
    if payload.len() > MAX_FRAME_BYTES + 64 {
        return Err(P2pError::FrameTooLarge {
            size: payload.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    let len = (payload.len() as u32).to_le_bytes();
    stream
        .write_all(&len)
        .map_err(|e| P2pError::Io(e.to_string()))?;
    stream
        .write_all(payload)
        .map_err(|e| P2pError::Io(e.to_string()))?;
    stream.flush().map_err(|e| P2pError::Io(e.to_string()))
}

fn read_raw_frame(stream: &mut TcpStream) -> Result<Vec<u8>, P2pError> {
    let mut len_bytes = [0u8; 4];
    stream
        .read_exact(&mut len_bytes)
        .map_err(|e| P2pError::Io(e.to_string()))?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    if len > MAX_FRAME_BYTES + 64 {
        return Err(P2pError::FrameTooLarge {
            size: len,
            max: MAX_FRAME_BYTES,
        });
    }
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .map_err(|e| P2pError::Io(e.to_string()))?;
    Ok(buf)
}

fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, P2pError> {
    let slice = read_slice(bytes, cursor, 1)?;
    Ok(slice[0])
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, P2pError> {
    let arr = read_array::<2>(bytes, cursor)?;
    Ok(u16::from_le_bytes(arr))
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], P2pError> {
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| P2pError::Codec("cursor overflow".into()))?;
    if end > bytes.len() {
        return Err(P2pError::Codec("unexpected EOF in frame".into()));
    }
    let out = &bytes[*cursor..end];
    *cursor = end;
    Ok(out)
}

fn read_array<const N: usize>(bytes: &[u8], cursor: &mut usize) -> Result<[u8; N], P2pError> {
    let slice = read_slice(bytes, cursor, N)?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_peer_mismatch_is_rejected_without_an_automatic_ban() {
        let local = PeerIdentity::from_seed([0x01; 32]);
        let expected_b = PeerIdentity::from_seed([0x02; 32]);
        let peer_c = PeerIdentity::from_seed([0x03; 32]);
        let (hello_c, _) =
            HandshakeHello::new(&peer_c, "auralite-test-1", TransportKind::TcpNoise, 9000);
        validate_remote_hello(
            local.peer_id(),
            "auralite-test-1",
            PROTOCOL_VERSION,
            &hello_c,
        )
        .expect("hello structure is valid before signature verification");
        assert_eq!(
            verify_expected_peer(Some(expected_b.peer_id()), hello_c.claimed_peer_id),
            Err(P2pError::IdentitySubstitution {
                expected: expected_b.peer_id(),
                actual: peer_c.peer_id(),
            })
        );
    }

    #[test]
    fn claimed_peer_id_must_match_public_key() {
        let local = PeerIdentity::from_seed([0x04; 32]);
        let remote = PeerIdentity::from_seed([0x05; 32]);
        let other = PeerIdentity::from_seed([0x06; 32]);
        let (mut hello, _) =
            HandshakeHello::new(&remote, "auralite-test-1", TransportKind::TcpNoise, 9001);
        hello.claimed_peer_id = other.peer_id();
        assert!(matches!(
            validate_remote_hello(local.peer_id(), "auralite-test-1", PROTOCOL_VERSION, &hello),
            Err(P2pError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn tampered_handshake_signature_is_rejected() {
        let initiator = PeerIdentity::from_seed([0x10; 32]);
        let responder = PeerIdentity::from_seed([0x20; 32]);
        let (hello_i, _) =
            HandshakeHello::new(&initiator, "auralite-test-1", TransportKind::TcpNoise, 9001);
        let (hello_r, _) =
            HandshakeHello::new(&responder, "auralite-test-1", TransportKind::TcpNoise, 9002);
        let transcript = compute_transcript_hash(&hello_i, &hello_r).unwrap();
        let mut sig =
            initiator.sign_transcript("auralite-test-1", HandshakeRole::Initiator, &transcript);
        sig[0] ^= 0x01;
        assert_eq!(
            verify_transcript_signature(
                initiator.public_key(),
                "auralite-test-1",
                HandshakeRole::Initiator,
                &transcript,
                &sig,
            ),
            Err(P2pError::InvalidHandshakeSignature)
        );
    }

    #[test]
    fn protocol_version_and_chain_id_mismatches_are_rejected() {
        let local = PeerIdentity::from_seed([0x30; 32]);
        let remote = PeerIdentity::from_seed([0x40; 32]);
        let (mut hello, _) =
            HandshakeHello::new(&remote, "auralite-test-1", TransportKind::TcpNoise, 9003);
        hello.protocol_version = PROTOCOL_VERSION + 1;
        assert_eq!(
            validate_remote_hello(local.peer_id(), "auralite-test-1", PROTOCOL_VERSION, &hello,),
            Err(P2pError::ProtocolVersionMismatch {
                local: PROTOCOL_VERSION,
                remote: PROTOCOL_VERSION + 1,
            })
        );

        hello.protocol_version = PROTOCOL_VERSION;
        hello.chain_id = "auralite-other-1".into();
        assert_eq!(
            validate_remote_hello(local.peer_id(), "auralite-test-1", PROTOCOL_VERSION, &hello,),
            Err(P2pError::ChainIdMismatch {
                expected: "auralite-test-1".into(),
                actual: "auralite-other-1".into(),
            })
        );
    }

    #[test]
    fn rejects_low_order_x25519_peer_key() {
        let secret = EphemeralSecret::random();
        assert!(SessionCipher::new(&[42; 32], secret, &[0; 32], HandshakeRole::Initiator).is_err());
    }

    fn cipher_pair(transcript: &[u8; 32]) -> (SessionCipher, SessionCipher) {
        let client_secret = EphemeralSecret::random();
        let server_secret = EphemeralSecret::random();
        let client_public = X25519PublicKey::from(&client_secret).to_bytes();
        let server_public = X25519PublicKey::from(&server_secret).to_bytes();
        let client = SessionCipher::new(
            transcript,
            client_secret,
            &server_public,
            HandshakeRole::Initiator,
        )
        .unwrap();
        let server = SessionCipher::new(
            transcript,
            server_secret,
            &client_public,
            HandshakeRole::Responder,
        )
        .unwrap();
        (client, server)
    }

    #[test]
    fn tampered_encrypted_frame_is_rejected() {
        let (mut client, mut server) = cipher_pair(&[0x55; 32]);
        let mut frame = client.seal_frame(b"hello authenticated swarm").unwrap();
        frame[8] ^= 0x80;
        assert_eq!(
            server.open_frame(&frame),
            Err(P2pError::FrameAuthenticationFailed)
        );
    }

    #[test]
    fn session_cipher_roundtrips_and_rejects_replay() {
        let (mut client, mut server) = cipher_pair(&[0xAA; 32]);
        let f0 = client.seal_frame(b"frame-0").unwrap();
        let f1 = client.seal_frame(b"frame-1").unwrap();
        assert_eq!(server.open_frame(&f0).unwrap(), b"frame-0");
        assert_eq!(
            server.open_frame(&f0),
            Err(P2pError::FrameAuthenticationFailed),
            "replayed frame must fail sequence check"
        );
        assert_eq!(server.open_frame(&f1).unwrap(), b"frame-1");
    }

    #[test]
    fn passive_observer_cannot_derive_session_keys_from_public_transcript() {
        let transcript = [0xAB; 32];
        let (mut client, mut server) = cipher_pair(&transcript);
        let frame = client.seal_frame(b"secret").unwrap();
        let observer_secret = EphemeralSecret::random();
        let dummy_secret = EphemeralSecret::random();
        let dummy_public = X25519PublicKey::from(&dummy_secret).to_bytes();
        let mut observer = SessionCipher::new(
            &transcript,
            observer_secret,
            &dummy_public,
            HandshakeRole::Responder,
        )
        .unwrap();
        assert!(observer.open_frame(&frame).is_err());
        assert_eq!(server.open_frame(&frame).unwrap(), b"secret");
    }
}
