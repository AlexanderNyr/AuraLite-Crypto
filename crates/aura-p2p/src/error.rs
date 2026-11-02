//! Error types for `aura-p2p` transport, identity, handshake, ban list, and gossipsub.

use crate::identity::PeerId;
use std::fmt;
use std::net::IpAddr;

/// Errors returned by `aura-p2p` transport, identity, handshake, ban list, and gossipsub operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P2pError {
    /// Claimed `PeerId` does not equal `BLAKE3(public_key)`.
    IdentityMismatch {
        /// The `PeerId` claimed in the handshake or gossip message.
        claimed: PeerId,
        /// The `PeerId` derived from the public key (`BLAKE3(pubkey)`).
        derived: PeerId,
    },
    /// Remote peer authenticated a valid keypair, but its `PeerId` does not
    /// match the `PeerId` expected by the dialer (identity substitution).
    IdentitySubstitution {
        /// The `PeerId` expected by the dialer.
        expected: PeerId,
        /// The `PeerId` presented by the remote peer.
        actual: PeerId,
    },
    /// The remote `PeerId` is currently on the local ban list.
    PeerBanned {
        /// Banned peer identifier.
        peer_id: PeerId,
        /// Reason recorded in the ban entry.
        reason: String,
    },
    /// The remote IP address is currently on the local ban list.
    IpBanned {
        /// Banned IP address.
        ip: IpAddr,
        /// Reason recorded in the ban entry.
        reason: String,
    },
    /// Handshake protocol version does not match local `PROTOCOL_VERSION`.
    ProtocolVersionMismatch {
        /// Local protocol version.
        local: u16,
        /// Remote protocol version.
        remote: u16,
    },
    /// Handshake or gossip `chain_id` does not match the local node's `chain_id`.
    ChainIdMismatch {
        /// Expected local `chain_id`.
        expected: String,
        /// Actual remote `chain_id`.
        actual: String,
    },
    /// Ed25519 signature over the handshake transcript failed verification.
    InvalidHandshakeSignature,
    /// Post-handshake encrypted frame failed MAC authentication or sequence check.
    FrameAuthenticationFailed,
    /// Frame exceeds `MAX_FRAME_BYTES`.
    FrameTooLarge {
        /// Frame length in bytes.
        size: usize,
        /// Maximum permitted frame length.
        max: usize,
    },
    /// Attempted to dial or accept our own `PeerId`.
    SelfConnection(PeerId),
    /// Connected peer table reached `max_peers`.
    MaxPeersReached {
        /// Maximum allowed peers.
        max: usize,
    },
    /// Wire framing or codec decoding error.
    Codec(String),
    /// Unknown or unsupported gossipsub topic name or wire tag.
    UnknownGossipTopic(String),
    /// Ed25519 signature over the gossip message failed verification.
    InvalidGossipSignature,
    /// Gossip message payload is empty.
    GossipPayloadEmpty,
    /// Gossip message payload exceeds `max_payload_bytes`.
    GossipPayloadTooLarge {
        /// Actual payload size in bytes.
        size: usize,
        /// Maximum permitted payload size in bytes.
        max: usize,
    },
    /// Peer published to a topic that the local node is not subscribed to.
    UnsubscribedTopic(String),
    /// Gossip message has already been seen (`MessageId`).
    DuplicateGossipMessage([u8; 32]),
    /// Peer score dropped to or below the graylist/ban threshold.
    PeerScoreBelowThreshold {
        /// Offending peer identifier.
        peer_id: PeerId,
        /// Updated peer score.
        score: i64,
    },
    /// Peer exceeded the per-window gossip message rate limit.
    GossipRateLimitExceeded {
        /// Offending peer identifier.
        peer_id: PeerId,
        /// Number of messages sent in the active window.
        count: usize,
        /// Maximum allowed messages in the active window.
        max: usize,
    },
    /// Ed25519 signature over a Kademlia [`crate::discovery::SignedPeerRecord`] failed verification.
    InvalidPeerRecordSignature,
    /// Peer record has no reachable direct address, UPnP port, or relay peer.
    UnreachablePeerRecord(PeerId),
    /// Peer record sequence number is older than or equal to the already-known record.
    StalePeerRecord {
        /// Peer whose record is stale.
        peer_id: PeerId,
        /// Sequence number already stored in the routing table.
        existing_seq: u64,
        /// Sequence number of the incoming record.
        got_seq: u64,
    },
    /// UPnP port mapping requested an invalid external port (`0`).
    InvalidUpnpPort(u16),
    /// Circuit Relay v2 target has no active reservation on the relay node.
    RelayReservationMissingOrExpired(PeerId),
    /// Circuit Relay v2 server reached its maximum concurrent reservation capacity.
    RelayCapacityReached {
        /// Maximum allowed reservations on this relay.
        max: usize,
    },
    /// Block sync header height does not equal `parent_height + 1` or expected height.
    BlockSyncHeightMismatch {
        /// Expected block height.
        expected: u64,
        /// Actual block height.
        actual: u64,
    },
    /// Block sync header `parent_hash` does not match the verified parent's `block_hash`.
    BlockSyncParentHashMismatch {
        /// Height of the offending header.
        height: u64,
    },
    /// Block sync header `block_hash` does not match the canonical hash of its fields.
    BlockSyncHeaderHashMismatch {
        /// Height of the offending header.
        height: u64,
    },
    /// Block sync header timestamp regressed relative to its parent.
    BlockSyncTimestampRegression {
        /// Height of the offending header.
        height: u64,
    },
    /// Block sync QC contains a signer that is not in the active validator set.
    BlockSyncQcUnknownValidator(PeerId),
    /// Block sync QC contains duplicate signatures from the same validator.
    BlockSyncQcDuplicateSigner(PeerId),
    /// Block sync QC Ed25519 signature failed verification.
    BlockSyncQcInvalidSignature(PeerId),
    /// Block sync QC signed stake is below the 2f+1 quorum threshold.
    BlockSyncQcInsufficientQuorum {
        /// Signed stake present in the QC.
        signed_stake: u64,
        /// Required quorum stake (`>= 2/3 + 1`).
        required_stake: u64,
    },
    /// Block body arrived before its header+QC was verified (`D-N4` header-first violation).
    BlockSyncHeaderMissing(u64),
    /// Block body `tx_root` does not match the verified header's `tx_root`.
    BlockSyncTxRootMismatch {
        /// Height of the corrupted block body.
        height: u64,
    },
    /// State sync chunk arrived before a verified S4 [`aura_state::SnapshotManifest`] was accepted.
    StateSyncManifestMissing,
    /// S4 snapshot manifest failed canonical decoding, checksum, or `chunk_root` verification.
    StateSyncInvalidManifest(String),
    /// S4 snapshot manifest `state_root` does not match the QC-certified header `state_root`.
    StateSyncRootMismatch {
        /// Expected state root from the QC-certified header.
        expected: [u8; 32],
        /// Actual state root in the manifest or recomputed from state entries.
        actual: [u8; 32],
    },
    /// State sync chunk index is out of range for the accepted manifest.
    StateSyncChunkOutOfRange {
        /// Requested chunk index.
        chunk_index: usize,
        /// Total chunks in manifest.
        total_chunks: usize,
    },
    /// State sync chunk failed SHA-256 digest or canonical S4 chunk decoding (`corrupt chunk`).
    StateSyncCorruptChunk {
        /// Index of the corrupt chunk.
        chunk_index: usize,
        /// Peer that served the corrupt chunk.
        peer_id: PeerId,
    },
    /// State sync finalization attempted while one or more chunks are still missing.
    StateSyncIncompleteChunks {
        /// Number of verified chunks present.
        have: usize,
        /// Total number of chunks required by the manifest.
        need: usize,
    },
    /// DoS defence pre-crypto check rejected an empty or oversized frame.
    DosFrameSizeExceeded {
        /// Frame size in bytes.
        size: usize,
        /// Maximum allowed frame size in bytes.
        max: usize,
    },
    /// Peer exceeded its per-window message rate limit (`msg/s`).
    DosMessageRateExceeded {
        /// Offending peer identifier.
        peer_id: PeerId,
        /// Observed message count in the active window.
        count: usize,
        /// Maximum allowed messages in the active window.
        max: usize,
    },
    /// Peer exceeded its per-window signature verification CPU budget (`sig/s`).
    DosSignatureBudgetExceeded {
        /// Offending peer identifier.
        peer_id: PeerId,
        /// Observed signature verifications in the active window.
        count: usize,
        /// Maximum allowed signature verifications in the active window.
        max: usize,
    },
    /// Peer signed two conflicting block hashes at the same `(height, round)` (double-vote / double-proposal).
    EquivocationDetected {
        /// Equivocating peer identifier.
        peer_id: PeerId,
        /// Block height of the equivocation.
        height: u64,
        /// Consensus round of the equivocation.
        round: u64,
        /// First signed block hash.
        first_hash: [u8; 32],
        /// Second conflicting signed block hash.
        second_hash: [u8; 32],
    },
    /// Light client proof exceeds the configured `max_proof_bytes` bound.
    LightClientProofTooLarge {
        /// Actual encoded proof size in bytes.
        size: usize,
        /// Maximum permitted proof size in bytes.
        max: usize,
    },
    /// Light client proof verification cost exceeds `max_verification_gas`.
    LightClientGasLimitExceeded {
        /// Actual verification gas required by the proof.
        gas: u64,
        /// Maximum permitted verification gas.
        max: u64,
    },
    /// Light client proof violates the trusted finalized checkpoint invariant.
    LightClientCheckpointViolation(String),
    /// Light client proof contains fewer distinct DAS chunk samples than required.
    LightClientDasInsufficientSamples {
        /// Distinct valid samples provided.
        have: usize,
        /// Minimum distinct samples required.
        need: usize,
    },
    /// Light client DAS sample failed Merkle branch verification against `header.tx_root`.
    LightClientDasSampleInvalid {
        /// Index of the invalid or corrupted DAS chunk sample.
        chunk_index: usize,
    },
    /// Chaos harness detected finalized-head (`block_hash` or `state_root`) divergence across honest nodes.
    ChaosHeadDivergence {
        /// Scenario name where divergence occurred.
        scenario: String,
        /// Node index whose finalized head diverged from node 0.
        node_index: usize,
    },
    /// Chaos harness configuration is invalid (e.g. `node_count < 4` or `packet_loss_bps > 10_000`).
    ChaosInvalidConfig(String),
    /// Underlying socket I/O error message.
    Io(String),
}

impl fmt::Display for P2pError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentityMismatch { claimed, derived } => write!(
                f,
                "peer identity mismatch: claimed {claimed}, derived {derived} from pubkey"
            ),
            Self::IdentitySubstitution { expected, actual } => write!(
                f,
                "peer identity substitution rejected: expected {expected}, got {actual}"
            ),
            Self::PeerBanned { peer_id, reason } => {
                write!(f, "peer {peer_id} is banned: {reason}")
            }
            Self::IpBanned { ip, reason } => {
                write!(f, "ip {ip} is banned: {reason}")
            }
            Self::ProtocolVersionMismatch { local, remote } => write!(
                f,
                "protocol version mismatch: local v{local}, remote v{remote}"
            ),
            Self::ChainIdMismatch { expected, actual } => write!(
                f,
                "chain_id mismatch: expected `{expected}`, got `{actual}`"
            ),
            Self::InvalidHandshakeSignature => {
                write!(f, "invalid Ed25519 handshake transcript signature")
            }
            Self::FrameAuthenticationFailed => {
                write!(
                    f,
                    "encrypted session frame authentication or sequence check failed"
                )
            }
            Self::FrameTooLarge { size, max } => {
                write!(f, "frame size {size} exceeds max {max}")
            }
            Self::SelfConnection(peer_id) => {
                write!(f, "refusing self-connection to local peer {peer_id}")
            }
            Self::MaxPeersReached { max } => {
                write!(f, "maximum connected peers ({max}) reached")
            }
            Self::Codec(msg) => write!(f, "wire codec error: {msg}"),
            Self::UnknownGossipTopic(topic) => {
                write!(f, "unknown gossipsub topic `{topic}`")
            }
            Self::InvalidGossipSignature => {
                write!(f, "invalid Ed25519 gossip message signature")
            }
            Self::GossipPayloadEmpty => {
                write!(f, "gossip message payload must not be empty")
            }
            Self::GossipPayloadTooLarge { size, max } => {
                write!(f, "gossip payload size {size} exceeds max {max}")
            }
            Self::UnsubscribedTopic(topic) => {
                write!(f, "gossip topic `{topic}` is not subscribed")
            }
            Self::DuplicateGossipMessage(id) => {
                write!(
                    f,
                    "duplicate gossip message {:02x}{:02x}{:02x}{:02x}",
                    id[0], id[1], id[2], id[3]
                )
            }
            Self::PeerScoreBelowThreshold { peer_id, score } => {
                write!(f, "peer {peer_id} score {score} is below threshold")
            }
            Self::GossipRateLimitExceeded {
                peer_id,
                count,
                max,
            } => {
                write!(
                    f,
                    "peer {peer_id} exceeded gossip rate limit ({count} > {max})"
                )
            }
            Self::InvalidPeerRecordSignature => {
                write!(f, "invalid Ed25519 peer record signature")
            }
            Self::UnreachablePeerRecord(peer_id) => {
                write!(f, "peer record for {peer_id} has no reachable endpoint")
            }
            Self::StalePeerRecord {
                peer_id,
                existing_seq,
                got_seq,
            } => write!(
                f,
                "stale peer record for {peer_id}: got seq {got_seq} <= existing {existing_seq}"
            ),
            Self::InvalidUpnpPort(port) => {
                write!(f, "invalid UPnP external port {port}")
            }
            Self::RelayReservationMissingOrExpired(peer_id) => {
                write!(
                    f,
                    "target peer {peer_id} has no active circuit relay reservation"
                )
            }
            Self::RelayCapacityReached { max } => {
                write!(f, "relay reservation capacity ({max}) reached")
            }
            Self::BlockSyncHeightMismatch { expected, actual } => write!(
                f,
                "block sync height mismatch: expected {expected}, got {actual}"
            ),
            Self::BlockSyncParentHashMismatch { height } => write!(
                f,
                "block sync header at height {height} has mismatched parent_hash"
            ),
            Self::BlockSyncHeaderHashMismatch { height } => write!(
                f,
                "block sync header at height {height} has mismatched block_hash"
            ),
            Self::BlockSyncTimestampRegression { height } => write!(
                f,
                "block sync header at height {height} regressed in timestamp"
            ),
            Self::BlockSyncQcUnknownValidator(peer_id) => {
                write!(f, "block sync QC signer {peer_id} is not in validator set")
            }
            Self::BlockSyncQcDuplicateSigner(peer_id) => {
                write!(f, "block sync QC contains duplicate signer {peer_id}")
            }
            Self::BlockSyncQcInvalidSignature(peer_id) => {
                write!(f, "block sync QC signature from {peer_id} is invalid")
            }
            Self::BlockSyncQcInsufficientQuorum {
                signed_stake,
                required_stake,
            } => write!(
                f,
                "block sync QC quorum insufficient: signed {signed_stake} < required {required_stake}"
            ),
            Self::BlockSyncHeaderMissing(height) => write!(
                f,
                "header-first violation: body at height {height} has no verified header+QC"
            ),
            Self::BlockSyncTxRootMismatch { height } => write!(
                f,
                "block sync body at height {height} does not match verified header tx_root"
            ),
            Self::StateSyncManifestMissing => {
                write!(
                    f,
                    "state sync manifest must be accepted before ingesting chunks"
                )
            }
            Self::StateSyncInvalidManifest(reason) => {
                write!(f, "invalid S4 snapshot manifest: {reason}")
            }
            Self::StateSyncRootMismatch { expected, actual } => write!(
                f,
                "state sync root mismatch: expected {:02x}{:02x}.., got {:02x}{:02x}..",
                expected[0], expected[1], actual[0], actual[1]
            ),
            Self::StateSyncChunkOutOfRange {
                chunk_index,
                total_chunks,
            } => write!(
                f,
                "state sync chunk index {chunk_index} out of range (total {total_chunks})"
            ),
            Self::StateSyncCorruptChunk {
                chunk_index,
                peer_id,
            } => write!(
                f,
                "corrupt state sync chunk {chunk_index} from peer {peer_id}"
            ),
            Self::StateSyncIncompleteChunks { have, need } => {
                write!(f, "state sync incomplete: have {have} of {need} chunks")
            }
            Self::DosFrameSizeExceeded { size, max } => {
                write!(f, "DoS frame size {size} invalid (allowed 1..={max})")
            }
            Self::DosMessageRateExceeded {
                peer_id,
                count,
                max,
            } => write!(
                f,
                "peer {peer_id} exceeded DoS message rate limit ({count} > {max})"
            ),
            Self::DosSignatureBudgetExceeded {
                peer_id,
                count,
                max,
            } => write!(
                f,
                "peer {peer_id} exceeded DoS signature verification budget ({count} > {max})"
            ),
            Self::EquivocationDetected {
                peer_id,
                height,
                round,
                ..
            } => write!(
                f,
                "equivocation detected from {peer_id} at height {height}, round {round}"
            ),
            Self::LightClientProofTooLarge { size, max } => {
                write!(f, "light client proof size {size} exceeds max {max} bytes")
            }
            Self::LightClientGasLimitExceeded { gas, max } => {
                write!(f, "light client proof gas {gas} exceeds max {max}")
            }
            Self::LightClientCheckpointViolation(reason) => {
                write!(f, "light client finalized checkpoint violation: {reason}")
            }
            Self::LightClientDasInsufficientSamples { have, need } => write!(
                f,
                "light client DAS samples insufficient: have {have}, need {need}"
            ),
            Self::LightClientDasSampleInvalid { chunk_index } => write!(
                f,
                "light client DAS sample {chunk_index} failed Merkle proof against tx_root"
            ),
            Self::ChaosHeadDivergence {
                scenario,
                node_index,
            } => write!(
                f,
                "chaos scenario `{scenario}` finalized-head divergence at node {node_index}"
            ),
            Self::ChaosInvalidConfig(reason) => {
                write!(f, "invalid chaos harness configuration: {reason}")
            }
            Self::Io(msg) => write!(f, "transport I/O error: {msg}"),
        }
    }
}

impl std::error::Error for P2pError {}

impl From<std::io::Error> for P2pError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}
