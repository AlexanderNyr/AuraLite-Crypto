//! D7 four-validator devnet composition, M1 consensus/P2P enforcement, and
//! multi-process `aurad` coordination.
//!
//! This module connects:
//! - N1 authenticated peer identity (`PeerId`, `PeerIdentity`, `TransportSwarm`),
//! - N2 signed, deduplicated gossip (`GossipEngine`, `GossipMessage`, `GossipTopic`),
//! - B3–B5 consensus (`FinalityEngine`, `QcAggregator`, `ValidatorSet`,
//!   `VrfLeaderElection`, `FourValidatorNetwork`), and
//! - D1–D6 node pipeline, mempool, JSON-RPC/REST/gRPC, and observability (`Pipeline`).

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aura_block::Block;
use aura_consensus::bls::{BlsPublicKey, BlsSecretKey, address_from_public_key};
use aura_consensus::finality::verify_vote;
use aura_consensus::qc::{QcAggregator, QuorumCertificate, Vote, VoteType};
use aura_consensus::vrf_leader::{FourValidatorNetwork, VoteEnvelope, VrfError};
use aura_crypto::ed25519_public_key;
use aura_p2p::gossipsub::{
    GossipEngine, GossipMessage, GossipOutcome, GossipTopic, PeerScoreParams,
};
use aura_p2p::identity::{PeerId, PeerIdentity};
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Domain, Gas, Hash, Height, Nonce};

use crate::observability::NodeMetrics;
use crate::pipeline::{
    Pipeline, PipelineConfig, PipelineError, TRANSFER_GAS, encode_transfer_payload,
};

/// Default chain identifier for the D7 local devnet.
pub const DEVNET_CHAIN_ID: &str = "auralite-devnet-1";
/// Number of genesis validators in the D7 devnet (`n = 4`, `f = 1`).
pub const DEVNET_VALIDATOR_COUNT: usize = 4;
/// Required 2f+1 quorum size for 4 validators (`3 of 4`).
pub const DEVNET_QUORUM_THRESHOLD: u64 = 3;
/// Deterministic genesis timestamp (unix ms) for the D7 devnet.
pub const DEVNET_GENESIS_TIME_MS: u64 = 1_700_000_000_000;
/// Canonical genesis proposer seed shared across devnet pipelines so all 4
/// independent stores compute identical genesis and block headers.
pub const DEVNET_CANONICAL_VALIDATOR_SEED: [u8; 32] = [0xD7; 32];
/// Distinct BLS12-381 secret key seeds for the 4 devnet validators.
pub const DEVNET_BLS_SEEDS: [[u8; 32]; 4] = [[11u8; 32], [22u8; 32], [33u8; 32], [44u8; 32]];
/// Distinct Ed25519 P2P identity seeds for the 4 devnet validators.
pub const DEVNET_P2P_SEEDS: [[u8; 32]; 4] = [[101u8; 32], [102u8; 32], [103u8; 32], [104u8; 32]];
/// Development-only faucet Ed25519 seed funded at genesis.
pub const DEVNET_FAUCET_SEED: [u8; 32] = [0xFA; 32];
/// Initial balance credited to the development faucet account at genesis.
pub const DEVNET_FAUCET_INITIAL_BALANCE: u128 = 1_000_000_000_000;

/// Errors returned by D7 devnet P2P, gossip, quorum, and coordination gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevnetError {
    /// Claimed `PeerId` does not match `BLAKE3(ed25519_public_key)`.
    SubstitutedPeerIdentity,
    /// Incoming gossip frame was malformed, unsigned, or failed verification.
    MalformedOrUnsignedGossip(String),
    /// Vote or QC failed cryptographic verification.
    InvalidVoteOrQc(String),
    /// Quorum was not reached (e.g. only 2 of 4 votes available).
    InsufficientQuorum {
        /// Number of valid votes collected.
        got: u64,
        /// Minimum `2f + 1` votes required.
        required: u64,
    },
    /// Underlying VRF/BFT error.
    Vrf(VrfError),
    /// Underlying node pipeline error.
    Pipeline(PipelineError),
    /// I/O error during multi-process coordination.
    Io(String),
}

impl fmt::Display for DevnetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubstitutedPeerIdentity => {
                f.write_str("peer identity substitution rejected: PeerId != BLAKE3(public_key)")
            }
            Self::MalformedOrUnsignedGossip(msg) => {
                write!(f, "malformed or unsigned gossip rejected: {msg}")
            }
            Self::InvalidVoteOrQc(msg) => write!(f, "invalid consensus vote or QC: {msg}"),
            Self::InsufficientQuorum { got, required } => {
                write!(f, "insufficient quorum: got {got} votes, need {required}")
            }
            Self::Vrf(err) => write!(f, "VRF/BFT error: {err}"),
            Self::Pipeline(err) => write!(f, "pipeline error: {err}"),
            Self::Io(msg) => write!(f, "devnet I/O error: {msg}"),
        }
    }
}

impl std::error::Error for DevnetError {}

impl From<VrfError> for DevnetError {
    fn from(value: VrfError) -> Self {
        Self::Vrf(value)
    }
}

impl From<PipelineError> for DevnetError {
    fn from(value: PipelineError) -> Self {
        Self::Pipeline(value)
    }
}

/// Deterministic development-only faucet Ed25519 public key.
pub fn faucet_public_key() -> [u8; 32] {
    ed25519_public_key(&DEVNET_FAUCET_SEED)
}

/// Deterministic development-only faucet account address.
pub fn faucet_address() -> Address {
    signer_address(&faucet_public_key())
}

/// Build and sign a native transfer transaction from the development faucet.
pub fn build_faucet_transfer(
    chain_id: &str,
    recipient: Address,
    amount: Amount,
    nonce: u64,
) -> Result<Tx, DevnetError> {
    let cid = ChainId::new(chain_id).map_err(|e| PipelineError::InvalidChainId(e.to_string()))?;
    let unsigned = UnsignedTx::new(TxParams {
        chain_id: cid,
        signer: faucet_address(),
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: encode_transfer_payload(recipient, amount),
        gas_limit: Gas::new(TRANSFER_GAS),
        max_fee: Amount::from_units(2),
        tip: Amount::ONE,
        expiry: Height::new(1_000_000),
    })
    .map_err(PipelineError::InvalidTransaction)?;
    unsigned
        .sign(&DEVNET_FAUCET_SEED)
        .map_err(PipelineError::InvalidTransaction)
        .map_err(DevnetError::from)
}

/// Verify that `claimed_peer_id` matches `BLAKE3(public_key)` in constant time.
///
/// Rejects substituted peer identities before any devnet consensus message is
/// accepted from that peer.
pub fn verify_devnet_peer_identity(
    claimed_peer_id: &PeerId,
    public_key: &[u8; 32],
) -> Result<(), DevnetError> {
    if claimed_peer_id.verify_public_key(public_key).is_err() {
        return Err(DevnetError::SubstitutedPeerIdentity);
    }
    Ok(())
}

/// Decode, verify, and deduplicate a raw N2 wire gossip frame.
///
/// - Returns `Err(DevnetError::MalformedOrUnsignedGossip)` if the wire frame is
///   truncated, malformed, unsigned, or fails Ed25519 signature verification.
/// - Returns `Ok(None)` if the message has already been seen (`SeenCache`
///   deduplication).
/// - Returns `Ok(Some(msg))` when a fresh, validly signed message is accepted.
pub fn ingest_devnet_gossip(
    engine: &mut GossipEngine,
    from_peer: PeerId,
    raw_wire_bytes: &[u8],
    now_ms: u64,
) -> Result<Option<GossipMessage>, DevnetError> {
    let decoded = GossipMessage::decode(raw_wire_bytes)
        .map_err(|e| DevnetError::MalformedOrUnsignedGossip(e.to_string()))?;
    match engine.ingest(from_peer, decoded.clone(), now_ms) {
        GossipOutcome::Accepted { .. } => Ok(Some(decoded)),
        GossipOutcome::Duplicate { .. } => Ok(None),
        GossipOutcome::Rejected { error, .. } => {
            Err(DevnetError::MalformedOrUnsignedGossip(error.to_string()))
        }
    }
}

/// Compute the count quorum `n - floor((n - 1) / 3)` for `n` validators.
///
/// For the 4-validator devnet (`f = 1`), this returns `3`.
pub fn devnet_quorum_threshold(validator_count: u64) -> u64 {
    aura_consensus::qc::quorum_threshold(validator_count)
}

/// Verify and aggregate a set of validator BLS votes for a devnet round.
///
/// Enforces:
/// 1. Every vote signature is cryptographically valid under `(chain_id, Domain::Vote)`.
/// 2. At least `devnet_quorum_threshold(validator_count)` distinct validators voted
///    (so 2 votes out of 4 never produce a QC).
/// 3. The resulting [`QuorumCertificate`] passes `qc.verify(chain_id, Domain::Vote)`.
pub fn try_commit_round_votes(
    chain_id: &str,
    validator_keys: &[BlsPublicKey],
    votes: &[(BlsPublicKey, Vote)],
) -> Result<Option<QuorumCertificate>, DevnetError> {
    let required = devnet_quorum_threshold(validator_keys.len() as u64);
    let mut agg = QcAggregator::with_signers(chain_id, validator_keys);
    let mut formed: Option<QuorumCertificate> = None;

    for (pk, vote) in votes {
        if address_from_public_key(pk) != vote.validator {
            return Err(DevnetError::InvalidVoteOrQc(
                "voter address does not match BLS public key".to_owned(),
            ));
        }
        verify_vote(chain_id, vote, pk).map_err(|e| DevnetError::InvalidVoteOrQc(e.to_string()))?;
        if let Some(qc) = agg.add_vote(pk.clone(), vote.clone()) {
            formed = Some(qc);
            break;
        }
    }

    let Some(qc) = formed else {
        return Ok(None);
    };
    if (qc.signatures.len() as u64) < required {
        return Err(DevnetError::InsufficientQuorum {
            got: qc.signatures.len() as u64,
            required,
        });
    }
    qc.verify(chain_id, Domain::Vote)
        .map_err(|e| DevnetError::InvalidVoteOrQc(e.to_string()))?;
    Ok(Some(qc))
}

/// Canonical devnet proposal commitment: binds the round to the actual block,
/// transaction root and post-state root. A round-only hash is never sufficient.
pub fn devnet_proposal_hash(round: u64, block: &Block) -> Hash {
    let mut payload = Vec::with_capacity(8 + 8 + 32 * 3);
    payload.extend_from_slice(&round.to_le_bytes());
    payload.extend_from_slice(&block.header.height.value().to_le_bytes());
    payload.extend_from_slice(block.block_hash().as_bytes());
    payload.extend_from_slice(block.header.tx_root.as_bytes());
    payload.extend_from_slice(block.header.state_root.as_bytes());
    aura_crypto::hash_domain(DEVNET_CHAIN_ID, Domain::Block, &payload)
}

/// In-process 4-validator devnet cluster combining `FourValidatorNetwork` (B5 + N1/N2)
/// with 4 independent `Pipeline` + `Storage` instances (D2–D6).
pub struct DevnetCluster {
    /// Underlying 4-validator N1/N2 + B3–B5 consensus network.
    pub consensus_net: FourValidatorNetwork,
    /// Independent D2/D3 pipelines (one per validator, each with its own storage directory).
    pub pipelines: Vec<Pipeline>,
    /// Next consensus round timestamp in milliseconds.
    pub next_timestamp_ms: u64,
}

impl DevnetCluster {
    /// Open a 4-validator devnet cluster rooted under `base_dir`, funding the
    /// development faucet account in all 4 independent validator stores.
    pub fn open(base_dir: &Path) -> Result<Self, DevnetError> {
        let consensus_net = FourValidatorNetwork::new(DEVNET_CHAIN_ID, 100);
        let allocations = vec![(
            faucet_address(),
            Amount::from_units(DEVNET_FAUCET_INITIAL_BALANCE),
        )];
        let mut pipelines = Vec::with_capacity(DEVNET_VALIDATOR_COUNT);
        for idx in 0..DEVNET_VALIDATOR_COUNT {
            let data_dir = base_dir.join(format!("validator-{idx}"));
            fs::create_dir_all(&data_dir).map_err(|e| DevnetError::Io(e.to_string()))?;
            let pipeline = Pipeline::open(PipelineConfig {
                chain_id: DEVNET_CHAIN_ID.to_owned(),
                data_dir,
                validator_seed: DEVNET_CANONICAL_VALIDATOR_SEED,
                allocations: allocations.clone(),
                genesis_time_ms: DEVNET_GENESIS_TIME_MS,
            })?;
            pipelines.push(pipeline);
        }
        Ok(Self {
            consensus_net,
            pipelines,
            next_timestamp_ms: DEVNET_GENESIS_TIME_MS + 1_000,
        })
    }

    /// Set whether validator `index` (`0..4`) is online (`true`) or down (`false`).
    pub fn set_validator_online(&mut self, index: usize, online: bool) {
        self.consensus_net.set_online(index, online);
    }

    /// Submit a transaction to validator `index` and gossip it to all online
    /// validators over N2 `GossipTopic::Tx`.
    pub fn submit_and_gossip_tx(
        &mut self,
        origin_index: usize,
        tx: Tx,
    ) -> Result<Hash, DevnetError> {
        let raw_tx = tx.to_bytes();
        let now_ms = self.next_timestamp_ms;
        let origin_peer = self.consensus_net.nodes[origin_index].peer_id();
        let (msg, _) = self.consensus_net.nodes[origin_index]
            .gossip
            .publish(GossipTopic::Tx, raw_tx)
            .map_err(|e| DevnetError::MalformedOrUnsignedGossip(e.to_string()))?;
        let wire = msg.encode();

        let txid = self.pipelines[origin_index].submit(tx)?;
        for idx in 0..DEVNET_VALIDATOR_COUNT {
            if idx == origin_index || !self.consensus_net.nodes[idx].online {
                continue;
            }
            if let Some(accepted) = ingest_devnet_gossip(
                &mut self.consensus_net.nodes[idx].gossip,
                origin_peer,
                &wire,
                now_ms,
            )? {
                let decoded_tx = Tx::from_canonical_bytes(&accepted.payload)
                    .map_err(|e| DevnetError::Pipeline(PipelineError::InvalidTransaction(e)))?;
                let _ = self.pipelines[idx].submit(decoded_tx)?;
            }
        }
        Ok(txid)
    }

    /// Run one consensus + execution round across all online validators.
    ///
    /// Requires at least `devnet_quorum_threshold(4) == 3` online validators to
    /// form a verified 3-of-4 BLS `QuorumCertificate` for matching previews
    /// before advancing local pipelines. This is still a devnet simulation,
    /// not a distributed commit protocol or safety boundary.
    pub fn step_block(&mut self) -> Result<QuorumCertificate, DevnetError> {
        let online_count = self.consensus_net.nodes.iter().filter(|n| n.online).count() as u64;
        let required = devnet_quorum_threshold(DEVNET_VALIDATOR_COUNT as u64);
        if online_count < required {
            return Err(DevnetError::InsufficientQuorum {
                got: online_count,
                required,
            });
        }

        let ts = self.next_timestamp_ms;
        let round = ts; // deterministic devnet round identifier
        let mut proposals = Vec::new();
        for idx in 0..DEVNET_VALIDATOR_COUNT {
            if self.consensus_net.nodes[idx].online {
                let block = self.pipelines[idx].preview_block()?;
                proposals.push((idx, devnet_proposal_hash(round, &block)));
            }
        }
        let canonical = proposals
            .first()
            .map(|(_, h)| *h)
            .ok_or(DevnetError::InsufficientQuorum { got: 0, required })?;
        if proposals.iter().any(|(_, h)| *h != canonical) {
            return Err(DevnetError::InvalidVoteOrQc(
                "online validators produced divergent blocks/state for the same round".to_owned(),
            ));
        }
        let validator_keys: Vec<BlsPublicKey> = DEVNET_BLS_SEEDS
            .iter()
            .map(|seed| BlsSecretKey::from_seed(seed).public_key())
            .collect();
        let mut votes = Vec::new();
        for (idx, _) in &proposals {
            let sk = BlsSecretKey::from_seed(&DEVNET_BLS_SEEDS[*idx]);
            let payload = [
                &1u64.to_le_bytes()[..],
                &round.to_le_bytes()[..],
                canonical.as_bytes(),
                &[VoteType::Precommit as u8],
            ]
            .concat();
            votes.push((
                sk.public_key(),
                Vote {
                    view: 1,
                    round,
                    block_hash: canonical,
                    validator: address_from_public_key(&sk.public_key()),
                    vote_type: VoteType::Precommit,
                    signature: sk.sign(DEVNET_CHAIN_ID, Domain::Vote, &payload),
                },
            ));
        }
        let qc = try_commit_round_votes(DEVNET_CHAIN_ID, &validator_keys, &votes)?.ok_or(
            DevnetError::InsufficientQuorum {
                got: votes.len() as u64,
                required,
            },
        )?;
        // Publish only after quorum. This remains a simulation: local
        // pipelines still use one-validator finality, and cross-node storage
        // commits are not a crash-atomic distributed transaction.
        for (idx, _) in proposals {
            let production = self.pipelines[idx].produce_block()?;
            if devnet_proposal_hash(round, &production.block) != canonical {
                return Err(DevnetError::InvalidVoteOrQc(
                    "pipeline changed after preview".to_owned(),
                ));
            }
        }
        self.next_timestamp_ms = self.next_timestamp_ms.saturating_add(1_000);
        Ok(qc)
    }
}

/// Write bytes atomically via a temporary file rename so concurrent processes
/// never observe a partially written wire frame.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), DevnetError> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| DevnetError::Io(e.to_string()))?;
    fs::rename(&tmp, path).map_err(|e| DevnetError::Io(e.to_string()))
}

/// Multi-process devnet worker loop executed inside each `aurad` validator
/// daemon when `AURA_DEVNET_DIR` and `AURA_VALIDATOR_INDEX` are configured.
pub fn run_devnet_worker(
    devnet_dir: PathBuf,
    validator_index: usize,
    pipeline: Arc<Mutex<Pipeline>>,
    metrics: Arc<NodeMetrics>,
    stop: Arc<AtomicBool>,
) {
    let wire_dir = devnet_dir.join("wire");
    let tx_dir = devnet_dir.join("tx");
    let _ = fs::create_dir_all(&wire_dir);
    let _ = fs::create_dir_all(&tx_dir);

    let chain_id = DEVNET_CHAIN_ID;
    let p2p_id = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[validator_index]);
    let mut gossip = GossipEngine::new(
        PeerIdentity::from_seed(DEVNET_P2P_SEEDS[validator_index]),
        chain_id,
        PeerScoreParams {
            rate_window_ms: 1_000,
            max_messages_per_window: 50_000,
            ..PeerScoreParams::default()
        },
    );
    for (idx, seed) in DEVNET_P2P_SEEDS.iter().enumerate() {
        if idx != validator_index {
            let peer = PeerIdentity::from_seed(*seed);
            gossip.register_peer(peer.peer_id(), GossipTopic::ALL);
        }
    }

    let bls_sk = BlsSecretKey::from_seed(&DEVNET_BLS_SEEDS[validator_index]);
    let bls_pk = bls_sk.public_key();
    let validator_addr = address_from_public_key(&bls_pk);
    let all_bls_pks: Vec<BlsPublicKey> = DEVNET_BLS_SEEDS
        .iter()
        .map(|s| BlsSecretKey::from_seed(s).public_key())
        .collect();

    let mut seen_tx_files: BTreeSet<String> = BTreeSet::new();
    let mut current_round: u64 = 1;

    while !stop.load(Ordering::Acquire) {
        // 1. Broadcast any locally submitted mempool transactions to `tx/` wire bus
        //    and ingest any remote `tx/` wire frames into our local mempool.
        if let Ok(guard) = pipeline.lock() {
            for tx in guard.pending_mempool_transactions() {
                let tx_hex = tx.txid().to_hex();
                let fname = format!("tx-{tx_hex}.wire");
                if !seen_tx_files.contains(&fname) {
                    let fpath = tx_dir.join(&fname);
                    if !fpath.exists()
                        && let Ok((msg, _)) = gossip.publish(GossipTopic::Tx, tx.to_bytes())
                    {
                        let _ = write_atomic(&fpath, &msg.encode());
                    }
                    seen_tx_files.insert(fname);
                }
            }
        }

        if let Ok(entries) = fs::read_dir(&tx_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().into_owned();
                if !fname.ends_with(".wire") || seen_tx_files.contains(&fname) {
                    continue;
                }
                if let Ok(raw) = fs::read(entry.path())
                    && let Ok(decoded) = GossipMessage::decode(&raw)
                {
                    let sender = decoded.sender_peer_id;
                    if decoded.topic != GossipTopic::Tx {
                        continue;
                    }
                    let Ok(Some(accepted)) =
                        ingest_devnet_gossip(&mut gossip, sender, &raw, 1_700_000_000_000)
                    else {
                        continue;
                    };
                    if let Ok(tx) = Tx::from_canonical_bytes(&accepted.payload)
                        && let Ok(mut guard) = pipeline.lock()
                    {
                        let _ = guard.submit(tx);
                    }
                    seen_tx_files.insert(fname);
                }
            }
        }

        // 2. Check if a round trigger file `round-<current_round>.req` exists.
        let req_path = wire_dir.join(format!("round-{current_round}.req"));
        if !req_path.exists() {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }

        // 3. Publish our signed BLS Precommit vote for `current_round` wrapped in
        //    an N2 `GossipTopic::Vote` wire frame.
        let vote_path = wire_dir.join(format!("round-{current_round}-vote-{validator_index}.wire"));
        let proposal_path = wire_dir.join(format!(
            "round-{current_round}-proposal-{validator_index}.hash"
        ));
        if !proposal_path.exists() {
            let preview = pipeline
                .lock()
                .ok()
                .and_then(|guard| guard.preview_block().ok());
            let Some(block) = preview else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            let block_hash = devnet_proposal_hash(current_round, &block);
            let _ = write_atomic(&proposal_path, block_hash.as_bytes());
        }
        if !vote_path.exists() {
            let Ok(raw_hash) = fs::read(&proposal_path) else {
                continue;
            };
            let Ok(bytes) = <[u8; 32]>::try_from(raw_hash.as_slice()) else {
                continue;
            };
            let block_hash = Hash::from_bytes(bytes);
            // Vote only for a proposal advertised by at least a quorum of validators.
            let mut matching = 0usize;
            for idx in 0..DEVNET_VALIDATOR_COUNT {
                let ppath = wire_dir.join(format!("round-{current_round}-proposal-{idx}.hash"));
                if fs::read(ppath).ok().as_deref() == Some(block_hash.as_bytes()) {
                    matching += 1;
                }
            }
            if matching < DEVNET_QUORUM_THRESHOLD as usize {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            let mut payload = Vec::with_capacity(8 + 8 + 32 + 1);
            payload.extend_from_slice(&1u64.to_le_bytes());
            payload.extend_from_slice(&current_round.to_le_bytes());
            payload.extend_from_slice(block_hash.as_bytes());
            payload.push(VoteType::Precommit as u8);
            let sig = bls_sk.sign(chain_id, Domain::Vote, &payload);
            let vote = Vote {
                view: 1,
                round: current_round,
                block_hash,
                validator: validator_addr,
                vote_type: VoteType::Precommit,
                signature: sig,
            };
            let env = VoteEnvelope {
                public_key: bls_pk.clone(),
                vote,
            };
            if let Ok((msg, _)) = gossip.publish(GossipTopic::Vote, env.to_bytes()) {
                let _ = write_atomic(&vote_path, &msg.encode());
            }
        }

        let Ok(raw_proposal_hash) = fs::read(&proposal_path) else {
            continue;
        };
        let Ok(proposal_bytes) = <[u8; 32]>::try_from(raw_proposal_hash.as_slice()) else {
            continue;
        };
        let expected_hash = Hash::from_bytes(proposal_bytes);

        // 4. Collect vote wire frames from all online validators until at least
        //    `devnet_quorum_threshold(4) == 3` valid votes are present.
        let mut collected_votes: Vec<(BlsPublicKey, Vote)> = Vec::new();
        for idx in 0..DEVNET_VALIDATOR_COUNT {
            let vpath = wire_dir.join(format!("round-{current_round}-vote-{idx}.wire"));
            let Ok(raw) = fs::read(&vpath) else {
                continue;
            };
            let Ok(msg) = GossipMessage::decode(&raw) else {
                continue;
            };
            let expected_peer = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[idx]);
            if verify_devnet_peer_identity(&msg.sender_peer_id, expected_peer.public_key()).is_err()
            {
                continue;
            }
            if msg.verify(chain_id, 4096).is_err() {
                continue;
            }
            if idx != validator_index {
                let _ = ingest_devnet_gossip(
                    &mut gossip,
                    msg.sender_peer_id,
                    &raw,
                    DEVNET_GENESIS_TIME_MS + current_round * 100,
                );
            }
            let Ok(env) = VoteEnvelope::from_bytes(&msg.payload) else {
                continue;
            };
            if env.public_key != all_bls_pks[idx]
                || env.vote.view != 1
                || env.vote.round != current_round
                || env.vote.block_hash != expected_hash
                || env.vote.vote_type != VoteType::Precommit
            {
                continue;
            }
            collected_votes.push((env.public_key, env.vote));
        }

        if let Ok(Some(qc)) = try_commit_round_votes(chain_id, &all_bls_pks, &collected_votes) {
            if qc.block_hash != expected_hash || qc.round != current_round || qc.view != 1 {
                continue;
            }
            if let Ok(mut guard) = pipeline.lock() {
                // RPC/mempool input can change while votes are collected; fail
                // closed if the current candidate no longer matches the QC.
                let Ok(candidate) = guard.preview_block() else {
                    continue;
                };
                if devnet_proposal_hash(current_round, &candidate) != expected_hash {
                    continue;
                }
                let Ok(production) = guard.produce_block() else {
                    continue;
                };
                if devnet_proposal_hash(current_round, &production.block) != expected_hash {
                    continue;
                }
                for committed in production.committed {
                    metrics.record_committed_block(
                        committed.block.header.gas_used.value(),
                        committed.receipts.len(),
                    );
                }
                let height = guard.committed_height();
                let state_root = guard
                    .committed_block(None)
                    .map(|b| b.header.state_root)
                    .unwrap_or(Hash::ZERO);
                tracing::info!(
                    validator = validator_index,
                    peer_id = %p2p_id.peer_id(),
                    round = current_round,
                    committed_height = height,
                    state_root = %format!("0x{}", state_root.to_hex()),
                    "devnet round finalized with 2f+1 QC"
                );
                let done_path =
                    wire_dir.join(format!("round-{current_round}-done-{validator_index}.ok"));
                let _ = write_atomic(&done_path, height.to_string().as_bytes());
                current_round += 1;
            }
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// Request round `round` on a multi-process devnet rooted at `devnet_dir` and
/// wait until all `online_indices` (`>= 3`) have finalized the round.
pub fn trigger_and_wait_devnet_round(
    devnet_dir: &Path,
    round: u64,
    online_indices: &[usize],
    timeout: Duration,
) -> Result<(), DevnetError> {
    let required = devnet_quorum_threshold(DEVNET_VALIDATOR_COUNT as u64) as usize;
    if online_indices.len() < required {
        return Err(DevnetError::InsufficientQuorum {
            got: online_indices.len() as u64,
            required: required as u64,
        });
    }
    let wire_dir = devnet_dir.join("wire");
    fs::create_dir_all(&wire_dir).map_err(|e| DevnetError::Io(e.to_string()))?;
    let req_path = wire_dir.join(format!("round-{round}.req"));
    if !req_path.exists() {
        write_atomic(&req_path, &round.to_le_bytes())?;
    }

    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let done_count = online_indices
            .iter()
            .filter(|&&idx| {
                wire_dir
                    .join(format!("round-{round}-done-{idx}.ok"))
                    .exists()
            })
            .count();
        if done_count == online_indices.len() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(DevnetError::Io(format!(
        "timed out waiting for round {round} across online validators {online_indices:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aura-d7-unit-{label}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn rejects_substituted_peer_identity() {
        let honest = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[0]);
        let impostor = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[1]);

        assert!(
            verify_devnet_peer_identity(&honest.peer_id(), honest.public_key()).is_ok(),
            "matching peer id and verifying key must succeed"
        );
        assert_eq!(
            verify_devnet_peer_identity(&honest.peer_id(), impostor.public_key()),
            Err(DevnetError::SubstitutedPeerIdentity),
            "substituted verifying key must be rejected"
        );
    }

    #[test]
    fn rejects_malformed_or_unsigned_gossip() {
        let receiver_id = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[0]);
        let sender_id = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[1]);
        let mut engine =
            GossipEngine::new(receiver_id, DEVNET_CHAIN_ID, PeerScoreParams::default());
        engine.register_peer(sender_id.peer_id(), GossipTopic::ALL);

        // Malformed wire bytes.
        assert!(matches!(
            ingest_devnet_gossip(
                &mut engine,
                sender_id.peer_id(),
                b"not-a-valid-frame",
                1_000
            ),
            Err(DevnetError::MalformedOrUnsignedGossip(_))
        ));

        // Validly encoded frame with tampered signature (unsigned/forged).
        let msg = GossipMessage::sign(
            &sender_id,
            DEVNET_CHAIN_ID,
            GossipTopic::Tx,
            1,
            b"transfer-payload".to_vec(),
        )
        .unwrap();
        let mut wire = msg.encode();
        let last = wire.len() - 1;
        wire[last] ^= 0xFF;
        assert!(matches!(
            ingest_devnet_gossip(&mut engine, sender_id.peer_id(), &wire, 1_000),
            Err(DevnetError::MalformedOrUnsignedGossip(_))
        ));
    }

    #[test]
    fn deduplicates_repeated_gossip_messages() {
        let receiver_id = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[0]);
        let sender_id = PeerIdentity::from_seed(DEVNET_P2P_SEEDS[1]);
        let mut engine =
            GossipEngine::new(receiver_id, DEVNET_CHAIN_ID, PeerScoreParams::default());
        engine.register_peer(sender_id.peer_id(), GossipTopic::ALL);

        let msg = GossipMessage::sign(
            &sender_id,
            DEVNET_CHAIN_ID,
            GossipTopic::Tx,
            1,
            b"unique-tx-payload".to_vec(),
        )
        .unwrap();
        let wire = msg.encode();

        let first = ingest_devnet_gossip(&mut engine, sender_id.peer_id(), &wire, 1_000)
            .expect("first delivery succeeds");
        assert!(first.is_some(), "first delivery must be accepted");

        let second = ingest_devnet_gossip(&mut engine, sender_id.peer_id(), &wire, 1_001)
            .expect("duplicate delivery does not error");
        assert!(
            second.is_none(),
            "duplicate gossip message must be deduplicated"
        );
    }

    #[test]
    fn four_validator_quorum_threshold_is_three() {
        assert_eq!(devnet_quorum_threshold(4), 3);
        assert_eq!(devnet_quorum_threshold(7), 5);
    }

    #[test]
    fn two_votes_of_four_never_commit() {
        let all_bls_pks: Vec<BlsPublicKey> = DEVNET_BLS_SEEDS
            .iter()
            .map(|seed| BlsSecretKey::from_seed(seed).public_key())
            .collect();
        let block_hash = Hash::from_bytes([0x42; 32]);
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.extend_from_slice(block_hash.as_bytes());
        payload.push(VoteType::Precommit as u8);

        let mut votes = Vec::new();
        for seed in &DEVNET_BLS_SEEDS[0..3] {
            let sk = BlsSecretKey::from_seed(seed);
            let pk = sk.public_key();
            let validator = address_from_public_key(&pk);
            let signature = sk.sign(DEVNET_CHAIN_ID, Domain::Vote, &payload);
            votes.push((
                pk,
                Vote {
                    view: 1,
                    round: 1,
                    block_hash,
                    validator,
                    vote_type: VoteType::Precommit,
                    signature,
                },
            ));
        }

        // Only 2 of 4 votes -> must return Ok(None) (no commit!).
        let two_votes = try_commit_round_votes(DEVNET_CHAIN_ID, &all_bls_pks, &votes[0..2])
            .expect("valid votes");
        assert!(
            two_votes.is_none(),
            "two votes out of four validators must never form a QC"
        );

        // 3 of 4 votes -> forms a verified QC.
        let three_votes = try_commit_round_votes(DEVNET_CHAIN_ID, &all_bls_pks, &votes[0..3])
            .expect("valid votes");
        assert_eq!(three_votes.expect("3 of 4 forms QC").signatures.len(), 3);
    }

    #[test]
    fn continues_finalization_with_one_of_four_validators_unavailable() {
        let dir = temp_dir("one-down");
        let mut cluster = DevnetCluster::open(&dir).unwrap();

        // Take validator 3 offline: remaining 3 validators preserve f=1 quorum.
        cluster.set_validator_online(3, false);

        let recipient = Address::from_bytes([0x77; 20]);
        let tx = build_faucet_transfer(DEVNET_CHAIN_ID, recipient, Amount::from_units(50_000), 0)
            .unwrap();
        cluster.submit_and_gossip_tx(0, tx).unwrap();

        // Run 5 blocks so the 3-chain commit rule finalizes the transfer block.
        for _ in 0..5 {
            let qc = cluster.step_block().unwrap();
            assert_eq!(qc.signatures.len(), 3);
        }

        let h0 = cluster.pipelines[0].committed_height();
        assert!(h0 >= 3, "expected committed height >= 3, got {h0}");
        let b0 = cluster.pipelines[0].committed_block(None).unwrap();
        for idx in 0..3 {
            assert_eq!(cluster.pipelines[idx].committed_height(), h0);
            let b = cluster.pipelines[idx].committed_block(None).unwrap();
            assert_eq!(b.block_hash(), b0.block_hash());
            assert_eq!(b.header.state_root, b0.header.state_root);
            let acct = cluster.pipelines[idx].account(&recipient).unwrap();
            assert_eq!(acct.balance, Amount::from_units(50_000));
        }
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn divergent_proposals_do_not_advance_pipeline_before_quorum() {
        let dir = temp_dir("divergent-preview");
        let mut cluster = DevnetCluster::open(&dir).unwrap();
        let recipient = Address::from_bytes([0x77; 20]);
        let tx = build_faucet_transfer(DEVNET_CHAIN_ID, recipient, Amount::from_units(50_000), 0)
            .unwrap();
        let txid = cluster.pipelines[0].submit(tx).unwrap();
        let before: Vec<_> = cluster
            .pipelines
            .iter()
            .map(|p| p.preview_block().unwrap())
            .collect();
        assert_ne!(before[0].block_hash(), before[1].block_hash());
        assert!(matches!(
            cluster.step_block(),
            Err(DevnetError::InvalidVoteOrQc(_))
        ));
        for (idx, pipeline) in cluster.pipelines.iter().enumerate() {
            assert_eq!(pipeline.committed_height(), 0);
            assert_eq!(pipeline.preview_block().unwrap(), before[idx]);
        }
        assert!(cluster.pipelines[0].contains_pending_txid(&txid));
        let _ = fs::remove_dir_all(&dir);
    }
}
