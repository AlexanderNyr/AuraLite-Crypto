//! Public-input, stake-weighted leader-selection simulation and networked BFT.
//!
//! This is not a secret VRF lottery or a production consensus protocol. Leader
//! selection uses publicly predictable input; boundary randomness may be biased
//! by selecting the signers included in a QC. The networked harness uses
//! signed proposals and quorum-verified votes but lacks persistent locking,
//! robust view-change certificates, and integration into the node swarm.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use aura_crypto::hash_domain;
use aura_p2p::gossipsub::{
    GossipEngine, GossipMessage, GossipOutcome, GossipTopic, PeerScoreParams,
};
use aura_p2p::identity::{PeerId, PeerIdentity};
use aura_types::{Address, Domain, Hash};

use crate::bls::{self, BlsPublicKey, BlsSecretKey, BlsSignature, address_from_public_key};
use crate::finality::{FinalityBlock, FinalityEngine, FinalizedBlock};
use crate::qc::{QcAggregator, QuorumCertificate, Vote, VoteType};
use crate::validator_set::{DEFAULT_MIN_STAKE, Validator, ValidatorSet, ValidatorStatus};

/// Stake amount in base units.
pub type Stake = u64;

const VRF_INPUT_TAG: &[u8] = b"AURA_CONSENSUS_VRF_INPUT_V1\0";
const VRF_OUTPUT_TAG: &[u8] = b"AURA_CONSENSUS_VRF_OUT_V1\0";
const VRF_EPOCH_TAG: &[u8] = b"AURA_CONSENSUS_EPOCH_RAND_V1\0";
const PROPOSAL_WIRE_MAGIC: &[u8; 4] = b"APR2";
const VOTE_WIRE_MAGIC: &[u8; 4] = b"AVOT";

/// Errors returned by VRF proving, verification, leader election, and networked BFT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VrfError {
    /// `chain_id` must not be empty.
    EmptyChainId,
    /// `epoch_randomness` must not be all-zero for `epoch > 0` (grinding protection).
    ZeroEpochRandomness,
    /// Boundary QC failed verification or had no signatures.
    InvalidBoundaryQc,
    /// Validator address does not match `address_from_public_key(public_key)`.
    ValidatorAddressMismatch,
    /// BLS VRF proof signature failed verification.
    InvalidProofSignature,
    /// Recomputed VRF output hash did not match `proof.output`.
    OutputMismatch,
    /// Validator set has no active stake.
    EmptyValidatorSet,
    /// Claiming validator is not in the active validator set.
    UnknownValidator(Address),
    /// Claiming validator is not the stake-weighted VRF leader for this round.
    NotElectedLeader {
        /// Validator that claimed leadership.
        claimed: Address,
        /// Deterministically elected leader for `(epoch, view, round)`.
        expected: Address,
    },
    /// Wire envelope failed decoding or validation.
    InvalidEnvelope(&'static str),
    /// P2P gossip layer rejected a message.
    GossipRejected(String),
    /// Vote or QC failed cryptographic verification.
    InvalidConsensusVote,
}

impl fmt::Display for VrfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChainId => f.write_str("VRF chain_id must not be empty"),
            Self::ZeroEpochRandomness => {
                f.write_str("VRF epoch_randomness must not be zero for epoch > 0")
            }
            Self::InvalidBoundaryQc => f.write_str("invalid boundary QC for epoch randomness"),
            Self::ValidatorAddressMismatch => {
                f.write_str("VRF proof validator address does not match public key")
            }
            Self::InvalidProofSignature => f.write_str("VRF BLS proof signature invalid"),
            Self::OutputMismatch => f.write_str("VRF output hash mismatch"),
            Self::EmptyValidatorSet => f.write_str("validator set has no active stake"),
            Self::UnknownValidator(addr) => write!(f, "unknown or inactive validator {addr}"),
            Self::NotElectedLeader { claimed, expected } => write!(
                f,
                "validator {claimed} is not the VRF leader (expected {expected})"
            ),
            Self::InvalidEnvelope(reason) => write!(f, "invalid consensus envelope: {reason}"),
            Self::GossipRejected(reason) => write!(f, "gossip rejected message: {reason}"),
            Self::InvalidConsensusVote => f.write_str("invalid consensus vote or QC"),
        }
    }
}

impl std::error::Error for VrfError {}

/// Leader-selection input transcript bound to epoch randomness, view, and round.
/// The boundary QC signer subset can bias that randomness; it is not proven
/// un-grindable or unpredictable.
///
/// Block contents and transaction hashes are deliberately excluded from
/// [`VrfInput`] so a proposer cannot grind transactions or timestamps to bias
/// leader election in future rounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VrfInput {
    /// Chain identifier for domain separation.
    pub chain_id: String,
    /// Consensus epoch number.
    pub epoch: u64,
    /// Epoch randomness derived from the previous epoch's boundary QC.
    pub epoch_randomness: Hash,
    /// Current BFT view number.
    pub view: u64,
    /// Current round number within the view.
    pub round: u64,
}

impl VrfInput {
    /// Construct a new [`VrfInput`].
    pub fn new(
        chain_id: impl Into<String>,
        epoch: u64,
        epoch_randomness: Hash,
        view: u64,
        round: u64,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            epoch,
            epoch_randomness,
            view,
            round,
        }
    }

    /// Validate that the VRF input satisfies grinding-protection invariants.
    pub fn validate(&self) -> Result<(), VrfError> {
        if self.chain_id.is_empty() {
            return Err(VrfError::EmptyChainId);
        }
        if self.epoch > 0 && self.epoch_randomness == Hash::ZERO {
            return Err(VrfError::ZeroEpochRandomness);
        }
        Ok(())
    }

    /// Canonical byte transcript signed by the VRF secret key.
    pub fn transcript_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(VRF_INPUT_TAG.len() + 8 + 32 + 8 + 8);
        out.extend_from_slice(VRF_INPUT_TAG);
        out.extend_from_slice(&self.epoch.to_le_bytes());
        out.extend_from_slice(self.epoch_randomness.as_bytes());
        out.extend_from_slice(&self.view.to_le_bytes());
        out.extend_from_slice(&self.round.to_le_bytes());
        out
    }
}

/// Derive deterministic epoch input from a membership-verified boundary QC.
///
/// This construction is potentially biasable via the included signer subset;
/// it is not a secure randomness beacon.
pub fn derive_epoch_randomness(
    prev_randomness: Hash,
    next_epoch: u64,
    boundary_qc: &QuorumCertificate,
    chain_id: &str,
    active: &BTreeSet<Address>,
) -> Result<Hash, VrfError> {
    if chain_id.is_empty() {
        return Err(VrfError::EmptyChainId);
    }
    if boundary_qc.signatures.is_empty()
        || boundary_qc.signatures.len() != boundary_qc.public_keys.len()
    {
        return Err(VrfError::InvalidBoundaryQc);
    }
    boundary_qc
        .verify_with_members(chain_id, Domain::Vote, active)
        .map_err(|_| VrfError::InvalidBoundaryQc)?;

    // Sort (pubkey, signature) pairs canonically so vote arrival order cannot
    // be manipulated to grind the resulting epoch randomness.
    let mut pairs: Vec<([u8; 48], [u8; 96])> = boundary_qc
        .public_keys
        .iter()
        .zip(boundary_qc.signatures.iter())
        .map(|(pk, sig)| (pk.to_bytes(), sig.to_bytes()))
        .collect();
    pairs.sort_by_key(|a| a.0);

    let mut buf = Vec::with_capacity(VRF_EPOCH_TAG.len() + 32 + 8 + 32 + pairs.len() * 144);
    buf.extend_from_slice(VRF_EPOCH_TAG);
    buf.extend_from_slice(prev_randomness.as_bytes());
    buf.extend_from_slice(&next_epoch.to_le_bytes());
    buf.extend_from_slice(boundary_qc.block_hash.as_bytes());
    for (pk_bytes, sig_bytes) in pairs {
        buf.extend_from_slice(&pk_bytes);
        buf.extend_from_slice(&sig_bytes);
    }
    Ok(hash_domain(chain_id, Domain::Genesis, &buf))
}

/// Publicly verifiable VRF proof and uniform 32-byte output produced by a validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VrfProof {
    /// Address of the validator that generated the VRF proof.
    pub validator: Address,
    /// Deterministic 96-byte BLS12-381 signature over the VRF transcript.
    pub signature: BlsSignature,
    /// Uniform 32-byte VRF output derived from `(public_key, signature, transcript)`.
    pub output: Hash,
}

fn compute_vrf_output(
    chain_id: &str,
    public_key: &BlsPublicKey,
    signature: &BlsSignature,
    transcript: &[u8],
) -> Hash {
    let mut buf = Vec::with_capacity(VRF_OUTPUT_TAG.len() + 48 + 96 + transcript.len());
    buf.extend_from_slice(VRF_OUTPUT_TAG);
    buf.extend_from_slice(&public_key.to_bytes());
    buf.extend_from_slice(&signature.to_bytes());
    buf.extend_from_slice(transcript);
    hash_domain(chain_id, Domain::Block, &buf)
}

/// Produce a deterministic [`VrfProof`] for `input` using `secret_key`.
pub fn prove_vrf(secret_key: &BlsSecretKey, input: &VrfInput) -> Result<VrfProof, VrfError> {
    input.validate()?;
    let public_key = secret_key.public_key();
    let validator = address_from_public_key(&public_key);
    let transcript = input.transcript_bytes();
    let signature = secret_key.sign(&input.chain_id, Domain::Block, &transcript);
    let output = compute_vrf_output(&input.chain_id, &public_key, &signature, &transcript);
    Ok(VrfProof {
        validator,
        signature,
        output,
    })
}

/// Publicly verify a [`VrfProof`] against `public_key` and `input`.
pub fn verify_vrf(
    public_key: &BlsPublicKey,
    input: &VrfInput,
    proof: &VrfProof,
) -> Result<(), VrfError> {
    input.validate()?;
    let expected_addr = address_from_public_key(public_key);
    if proof.validator != expected_addr {
        return Err(VrfError::ValidatorAddressMismatch);
    }
    let transcript = input.transcript_bytes();
    bls::verify(
        public_key,
        &input.chain_id,
        Domain::Block,
        &transcript,
        &proof.signature,
    )
    .map_err(|_| VrfError::InvalidProofSignature)?;

    let expected_output =
        compute_vrf_output(&input.chain_id, public_key, &proof.signature, &transcript);
    if proof.output != expected_output {
        return Err(VrfError::OutputMismatch);
    }
    Ok(())
}

/// Validator entry participating in stake-weighted VRF leader election.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VrfValidatorEntry {
    /// Validator address (`address_from_public_key(&public_key)`).
    pub address: Address,
    /// Validator BLS12-381 public key.
    pub public_key: BlsPublicKey,
    /// Active stake weight (`> 0`).
    pub stake: Stake,
}

/// Signed leader claim attaching a validator's [`VrfProof`] to a round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderClaim {
    /// The validator's VRF proof for the round.
    pub proof: VrfProof,
}

/// Stake-weighted VRF leader election table over the active validator set.
#[derive(Debug, Clone)]
pub struct VrfLeaderElection {
    validators: BTreeMap<Address, VrfValidatorEntry>,
    total_stake: Stake,
}

impl VrfLeaderElection {
    /// Build a stake-weighted VRF election table from validator entries.
    ///
    /// Entries with zero stake are ignored; returns [`VrfError::EmptyValidatorSet`]
    /// if the resulting active stake is zero.
    pub fn new(entries: Vec<VrfValidatorEntry>) -> Result<Self, VrfError> {
        let mut validators = BTreeMap::new();
        let mut total_stake = 0u64;
        for entry in entries {
            if entry.stake == 0 {
                continue;
            }
            if address_from_public_key(&entry.public_key) != entry.address {
                return Err(VrfError::ValidatorAddressMismatch);
            }
            total_stake = total_stake.saturating_add(entry.stake);
            validators.insert(entry.address, entry);
        }
        if validators.is_empty() || total_stake == 0 {
            return Err(VrfError::EmptyValidatorSet);
        }
        Ok(Self {
            validators,
            total_stake,
        })
    }

    /// Build from a [`ValidatorSet`] and a mapping of validator BLS public keys.
    pub fn from_validator_set(
        set: &ValidatorSet,
        public_keys: &BTreeMap<Address, BlsPublicKey>,
    ) -> Result<Self, VrfError> {
        let mut entries = Vec::new();
        for addr in set.active_addresses() {
            let Some(validator) = set.validator(&addr) else {
                continue;
            };
            let Some(pk) = public_keys.get(&addr) else {
                return Err(VrfError::UnknownValidator(addr));
            };
            entries.push(VrfValidatorEntry {
                address: addr,
                public_key: pk.clone(),
                stake: validator.stake,
            });
        }
        Self::new(entries)
    }

    /// Total active stake across all registered validators.
    pub fn total_stake(&self) -> Stake {
        self.total_stake
    }

    /// Number of active validators in the election table.
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// True when the election table has no validators.
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    /// Look up a validator's entry by address.
    pub fn get(&self, address: &Address) -> Option<&VrfValidatorEntry> {
        self.validators.get(address)
    }

    /// Deterministically select the stake-weighted VRF leader for `input`.
    ///
    /// Computes a uniform 64-bit VRF ticket from `hash_domain(chain_id, Domain::Block, transcript)`
    /// and maps it onto `[0, total_stake)` via `((draw as u128) * (total_stake as u128)) >> 64`.
    /// Each validator is chosen with probability strictly proportional to
    /// `stake_i / total_stake`.
    pub fn select_leader(&self, input: &VrfInput) -> Result<Address, VrfError> {
        input.validate()?;
        if self.validators.is_empty() || self.total_stake == 0 {
            return Err(VrfError::EmptyValidatorSet);
        }
        let transcript = input.transcript_bytes();
        let digest = hash_domain(&input.chain_id, Domain::Block, &transcript);
        let draw = u64::from_be_bytes(
            digest.as_bytes()[0..8]
                .try_into()
                .expect("8-byte slice from 32-byte digest"),
        );
        let target = (((draw as u128) * (self.total_stake as u128)) >> 64) as u64;

        let mut cumulative = 0u64;
        for (addr, entry) in &self.validators {
            cumulative = cumulative.saturating_add(entry.stake);
            if target < cumulative {
                return Ok(*addr);
            }
        }
        Ok(*self.validators.keys().next_back().expect("non-empty"))
    }

    /// Produce a [`LeaderClaim`] for `input` if `secret_key` belongs to the
    /// elected leader for `input`.
    pub fn prove_leader_claim(
        &self,
        secret_key: &BlsSecretKey,
        input: &VrfInput,
    ) -> Result<LeaderClaim, VrfError> {
        let proof = prove_vrf(secret_key, input)?;
        let expected = self.select_leader(input)?;
        if proof.validator != expected {
            return Err(VrfError::NotElectedLeader {
                claimed: proof.validator,
                expected,
            });
        }
        Ok(LeaderClaim { proof })
    }

    /// Publicly verify that `claim` is from an active validator, that its
    /// [`VrfProof`] is cryptographically valid for `input`, and that the
    /// validator is the stake-weighted VRF leader for `input`.
    pub fn verify_leader_claim(
        &self,
        claim: &LeaderClaim,
        input: &VrfInput,
    ) -> Result<(), VrfError> {
        let Some(entry) = self.validators.get(&claim.proof.validator) else {
            return Err(VrfError::UnknownValidator(claim.proof.validator));
        };
        let expected = self.select_leader(input)?;
        if claim.proof.validator != expected {
            return Err(VrfError::NotElectedLeader {
                claimed: claim.proof.validator,
                expected,
            });
        }
        verify_vrf(&entry.public_key, input, &claim.proof)
    }
}

/// Wire payload broadcast on [`GossipTopic::Proposal`] in B5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalEnvelope {
    /// Consensus epoch number.
    pub epoch: u64,
    /// Epoch randomness seed.
    pub epoch_randomness: Hash,
    /// Proposed finality block.
    pub block: FinalityBlock,
    /// Leader's VRF proof for `(epoch, epoch_randomness, block.view, block.round)`.
    pub vrf_proof: VrfProof,
    /// Leader BLS signature over epoch, randomness and the block commitment.
    pub proposal_signature: BlsSignature,
}

impl ProposalEnvelope {
    /// Encode to canonical wire bytes for [`GossipTopic::Proposal`].
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(PROPOSAL_WIRE_MAGIC);
        out.extend_from_slice(&self.epoch.to_le_bytes());
        out.extend_from_slice(self.epoch_randomness.as_bytes());
        out.extend_from_slice(&self.block.view.to_le_bytes());
        out.extend_from_slice(&self.block.round.to_le_bytes());
        out.extend_from_slice(self.block.parent_hash.as_bytes());
        out.extend_from_slice(&self.block.timestamp_ms.to_le_bytes());
        out.extend_from_slice(self.block.block_hash.as_bytes());
        out.extend_from_slice(self.vrf_proof.validator.as_bytes());
        out.extend_from_slice(&self.vrf_proof.signature.to_bytes());
        out.extend_from_slice(self.vrf_proof.output.as_bytes());
        out.extend_from_slice(&self.proposal_signature.to_bytes());
        out.extend_from_slice(&(self.block.transactions.len() as u32).to_le_bytes());
        for tx in &self.block.transactions {
            out.extend_from_slice(&(tx.len() as u32).to_le_bytes());
            out.extend_from_slice(tx);
        }
        out
    }

    /// Decode from canonical wire bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VrfError> {
        if bytes.len() < 4 + 8 + 32 + 8 + 8 + 32 + 8 + 32 + 20 + 96 + 32 + 96 + 4 {
            return Err(VrfError::InvalidEnvelope("proposal envelope truncated"));
        }
        if &bytes[0..4] != PROPOSAL_WIRE_MAGIC {
            return Err(VrfError::InvalidEnvelope("bad proposal magic"));
        }
        let mut pos = 4;
        let epoch = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let epoch_randomness = Hash::from_bytes(bytes[pos..pos + 32].try_into().unwrap());
        pos += 32;
        let view = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let round = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let parent_hash = Hash::from_bytes(bytes[pos..pos + 32].try_into().unwrap());
        pos += 32;
        let timestamp_ms = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let block_hash = Hash::from_bytes(bytes[pos..pos + 32].try_into().unwrap());
        pos += 32;
        let validator = Address::from_bytes(bytes[pos..pos + 20].try_into().unwrap());
        pos += 20;
        let signature = BlsSignature::from_bytes(&bytes[pos..pos + 96])
            .map_err(|_| VrfError::InvalidEnvelope("invalid VRF BLS signature bytes"))?;
        pos += 96;
        let output = Hash::from_bytes(bytes[pos..pos + 32].try_into().unwrap());
        pos += 32;
        let proposal_signature = BlsSignature::from_bytes(&bytes[pos..pos + 96])
            .map_err(|_| VrfError::InvalidEnvelope("invalid proposal BLS signature bytes"))?;
        pos += 96;
        let tx_count = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let mut transactions = Vec::with_capacity(tx_count.min(1024));
        for _ in 0..tx_count {
            if pos + 4 > bytes.len() {
                return Err(VrfError::InvalidEnvelope("proposal tx length truncated"));
            }
            let tx_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            if pos + tx_len > bytes.len() {
                return Err(VrfError::InvalidEnvelope("proposal tx payload truncated"));
            }
            transactions.push(bytes[pos..pos + tx_len].to_vec());
            pos += tx_len;
        }
        if pos != bytes.len() {
            return Err(VrfError::InvalidEnvelope("trailing bytes in proposal"));
        }

        // Recompute canonical block hash to prevent header/payload mismatch.
        let dummy_parent = FinalityBlock {
            view,
            round: 0,
            parent_hash,
            timestamp_ms: 0,
            block_hash: parent_hash,
            qc: None,
            transactions: Vec::new(),
        };
        let rebuilt = if transactions.is_empty() {
            FinalityBlock::propose_nil(view, round, &dummy_parent, timestamp_ms)
        } else {
            FinalityBlock::propose(view, round, &dummy_parent, timestamp_ms, transactions)
        };
        if rebuilt.block_hash != block_hash {
            return Err(VrfError::InvalidEnvelope("proposal block_hash mismatch"));
        }

        Ok(Self {
            epoch,
            epoch_randomness,
            block: rebuilt,
            vrf_proof: VrfProof {
                validator,
                signature,
                output,
            },
            proposal_signature,
        })
    }
}

fn proposal_signing_payload(epoch: u64, randomness: Hash, block: &FinalityBlock) -> Vec<u8> {
    let mut payload = Vec::with_capacity(96);
    payload.extend_from_slice(b"AURA_CONSENSUS_PROPOSAL_V2\0");
    payload.extend_from_slice(&epoch.to_le_bytes());
    payload.extend_from_slice(randomness.as_bytes());
    payload.extend_from_slice(&block.view.to_le_bytes());
    payload.extend_from_slice(&block.round.to_le_bytes());
    payload.extend_from_slice(block.block_hash.as_bytes());
    payload
}

/// Wire payload broadcast on [`GossipTopic::Vote`] in B5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteEnvelope {
    /// Signer's BLS12-381 public key.
    pub public_key: BlsPublicKey,
    /// Signed BLS consensus vote.
    pub vote: Vote,
}

impl VoteEnvelope {
    /// Encode to canonical wire bytes for [`GossipTopic::Vote`].
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + 48 + 8 + 8 + 32 + 20 + 1 + 96);
        out.extend_from_slice(VOTE_WIRE_MAGIC);
        out.extend_from_slice(&self.public_key.to_bytes());
        out.extend_from_slice(&self.vote.view.to_le_bytes());
        out.extend_from_slice(&self.vote.round.to_le_bytes());
        out.extend_from_slice(self.vote.block_hash.as_bytes());
        out.extend_from_slice(self.vote.validator.as_bytes());
        let vote_tag = match self.vote.vote_type {
            VoteType::Propose => 0u8,
            VoteType::Prevote => 1u8,
            VoteType::Precommit => 2u8,
        };
        out.push(vote_tag);
        out.extend_from_slice(&self.vote.signature.to_bytes());
        out
    }

    /// Decode from canonical wire bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VrfError> {
        if bytes.len() != 4 + 48 + 8 + 8 + 32 + 20 + 1 + 96 {
            return Err(VrfError::InvalidEnvelope("vote envelope wrong length"));
        }
        if &bytes[0..4] != VOTE_WIRE_MAGIC {
            return Err(VrfError::InvalidEnvelope("bad vote magic"));
        }
        let public_key = BlsPublicKey::from_bytes(&bytes[4..52])
            .map_err(|_| VrfError::InvalidEnvelope("invalid vote public key"))?;
        let view = u64::from_le_bytes(bytes[52..60].try_into().unwrap());
        let round = u64::from_le_bytes(bytes[60..68].try_into().unwrap());
        let block_hash = Hash::from_bytes(bytes[68..100].try_into().unwrap());
        let validator = Address::from_bytes(bytes[100..120].try_into().unwrap());
        let vote_type = match bytes[120] {
            0 => VoteType::Propose,
            1 => VoteType::Prevote,
            2 => VoteType::Precommit,
            _ => return Err(VrfError::InvalidEnvelope("invalid vote type byte")),
        };
        let signature = BlsSignature::from_bytes(&bytes[121..217])
            .map_err(|_| VrfError::InvalidEnvelope("invalid vote signature bytes"))?;
        Ok(Self {
            public_key,
            vote: Vote {
                view,
                round,
                block_hash,
                validator,
                vote_type,
                signature,
            },
        })
    }
}

/// One validator node connecting N1/N2 (`PeerIdentity` + `GossipEngine`) to
/// the B3–B5 consensus pipeline (`FinalityEngine` + `ValidatorSet` + `VrfLeaderElection`).
pub struct NetworkedValidatorNode {
    /// Chain identifier.
    pub chain_id: String,
    /// P2P identity (N1).
    pub p2p_identity: PeerIdentity,
    /// Gossipsub engine (N2).
    pub gossip: GossipEngine,
    /// Validator BLS secret key.
    pub bls_secret: BlsSecretKey,
    /// Validator BLS public key.
    pub bls_public: BlsPublicKey,
    /// Validator consensus address.
    pub address: Address,
    /// Validator set (B4).
    pub validator_set: ValidatorSet,
    /// Stake-weighted VRF leader election (B5).
    pub leader_election: VrfLeaderElection,
    /// BFT 3-chain finality engine (B3).
    pub finality: FinalityEngine,
    /// Current epoch randomness seed.
    pub epoch_randomness: Hash,
    /// Whether this validator is currently online in the network.
    pub online: bool,
    /// Per-(view, round, block_hash) vote aggregators.
    aggregators: BTreeMap<(u64, u64, Hash), QcAggregator>,
    voted: BTreeMap<(u64, u64), Hash>,
}

impl NetworkedValidatorNode {
    /// Construct a new networked validator node.
    pub fn new(
        chain_id: &str,
        p2p_seed: [u8; 32],
        bls_seed: [u8; 32],
        validator_set: ValidatorSet,
        leader_election: VrfLeaderElection,
        genesis_hash: Hash,
        epoch_randomness: Hash,
    ) -> Self {
        let p2p_identity = PeerIdentity::from_seed(p2p_seed);
        let gossip = GossipEngine::new(
            PeerIdentity::from_seed(p2p_seed),
            chain_id,
            PeerScoreParams {
                rate_window_ms: 1_000,
                max_messages_per_window: 10_000,
                ..PeerScoreParams::default()
            },
        );
        let bls_secret = BlsSecretKey::from_seed(&bls_seed);
        let bls_public = bls_secret.public_key();
        let address = address_from_public_key(&bls_public);
        let finality = FinalityEngine::new(chain_id, 0, genesis_hash);
        Self {
            chain_id: chain_id.to_string(),
            p2p_identity,
            gossip,
            bls_secret,
            bls_public,
            address,
            validator_set,
            leader_election,
            finality,
            epoch_randomness,
            online: true,
            aggregators: BTreeMap::new(),
            voted: BTreeMap::new(),
        }
    }

    /// Peer ID of this validator's N1 P2P identity.
    pub fn peer_id(&self) -> PeerId {
        self.p2p_identity.peer_id()
    }

    /// Build the [`VrfInput`] for this node's current `(epoch, view, round)`.
    pub fn current_vrf_input(&self) -> VrfInput {
        VrfInput::new(
            &self.chain_id,
            self.validator_set.epoch(),
            self.epoch_randomness,
            self.finality.view(),
            self.finality.round(),
        )
    }

    /// If this validator is the stake-weighted VRF leader for the current
    /// `(epoch, view, round)`, propose a block and publish a signed N2
    /// [`GossipMessage`] on [`GossipTopic::Proposal`].
    pub fn try_propose(
        &mut self,
        timestamp_ms: u64,
        transactions: Vec<Vec<u8>>,
    ) -> Result<Option<GossipMessage>, VrfError> {
        if !self.online {
            return Ok(None);
        }
        let input = self.current_vrf_input();
        let leader = self.leader_election.select_leader(&input)?;
        if leader != self.address {
            return Ok(None);
        }
        let claim = self
            .leader_election
            .prove_leader_claim(&self.bls_secret, &input)?;
        let block = self.finality.propose_at(timestamp_ms, transactions);
        let proposal_signature = self.bls_secret.sign(
            &self.chain_id,
            Domain::Block,
            &proposal_signing_payload(input.epoch, input.epoch_randomness, &block),
        );
        let envelope = ProposalEnvelope {
            epoch: input.epoch,
            epoch_randomness: input.epoch_randomness,
            block,
            vrf_proof: claim.proof,
            proposal_signature,
        };
        let (msg, _) = self
            .gossip
            .publish(GossipTopic::Proposal, envelope.to_bytes())
            .map_err(|e| VrfError::GossipRejected(e.to_string()))?;
        Ok(Some(msg))
    }

    /// Process an incoming N2 [`GossipMessage`] on [`GossipTopic::Proposal`]:
    /// verifies the N2 Ed25519 signature, deduplicates via `SeenCache`,
    /// verifies the leader's [`VrfProof`] via [`VrfLeaderElection::verify_leader_claim`],
    /// records the block, and returns a signed [`GossipTopic::Vote`] message.
    pub fn on_proposal_gossip(
        &mut self,
        from_peer: PeerId,
        msg: &GossipMessage,
        now_ms: u64,
    ) -> Result<Option<GossipMessage>, VrfError> {
        if !self.online {
            return Ok(None);
        }
        if from_peer != self.peer_id() {
            match self.gossip.ingest(from_peer, msg.clone(), now_ms) {
                GossipOutcome::Accepted { .. } => {}
                GossipOutcome::Duplicate { .. } => return Ok(None),
                GossipOutcome::Rejected { error, .. } => {
                    return Err(VrfError::GossipRejected(error.to_string()));
                }
            }
        }
        let env = ProposalEnvelope::from_bytes(&msg.payload)?;
        let input = VrfInput::new(
            &self.chain_id,
            env.epoch,
            env.epoch_randomness,
            env.block.view,
            env.block.round,
        );
        if input != self.current_vrf_input() {
            return Err(VrfError::InvalidEnvelope(
                "unexpected epoch, randomness, view or round",
            ));
        }
        self.leader_election.verify_leader_claim(
            &LeaderClaim {
                proof: env.vrf_proof.clone(),
            },
            &input,
        )?;
        let leader = self
            .leader_election
            .validators
            .get(&env.vrf_proof.validator)
            .ok_or(VrfError::UnknownValidator(env.vrf_proof.validator))?;
        bls::verify(
            &leader.public_key,
            &self.chain_id,
            Domain::Block,
            &proposal_signing_payload(env.epoch, env.epoch_randomness, &env.block),
            &env.proposal_signature,
        )
        .map_err(|_| VrfError::InvalidEnvelope("proposal signature invalid"))?;
        let position = (env.block.view, env.block.round);
        if let Some(previous) = self.voted.get(&position) {
            return if *previous == env.block.block_hash {
                Ok(None)
            } else {
                Err(VrfError::InvalidEnvelope("conflicting proposal for round"))
            };
        }
        // A proposer has already advanced its own tip in try_propose; followers
        // must see a block extending their current tip.
        if env.block.block_hash != self.finality.tip.block_hash
            && env.block.parent_hash != self.finality.tip.block_hash
        {
            return Err(VrfError::InvalidEnvelope(
                "proposal does not extend local tip",
            ));
        }
        self.voted.insert(position, env.block.block_hash);
        self.finality.commit_rule.insert_block(env.block.clone());
        self.finality.tip = env.block.clone();

        let vote = self.finality.vote_for(
            &self.bls_secret,
            self.address,
            &env.block,
            VoteType::Precommit,
        );
        let vote_env = VoteEnvelope {
            public_key: self.bls_public.clone(),
            vote,
        };
        let (vote_msg, _) = self
            .gossip
            .publish(GossipTopic::Vote, vote_env.to_bytes())
            .map_err(|e| VrfError::GossipRejected(e.to_string()))?;
        Ok(Some(vote_msg))
    }

    /// Process an incoming N2 [`GossipMessage`] on [`GossipTopic::Vote`]:
    /// verifies the N2 Ed25519 signature, verifies the voter belongs to the
    /// active [`ValidatorSet`], cryptographically verifies the BLS vote
    /// signature (`finality::verify_vote`), aggregates into a 2f+1
    /// [`QuorumCertificate`], verifies the QC (`qc.verify`), and commits via
    /// the 3-chain rule.
    pub fn on_vote_gossip(
        &mut self,
        from_peer: PeerId,
        msg: &GossipMessage,
        now_ms: u64,
    ) -> Result<(Option<QuorumCertificate>, Vec<FinalizedBlock>), VrfError> {
        if !self.online {
            return Ok((None, Vec::new()));
        }
        if from_peer != self.peer_id() {
            match self.gossip.ingest(from_peer, msg.clone(), now_ms) {
                GossipOutcome::Accepted { .. } => {}
                GossipOutcome::Duplicate { .. } => return Ok((None, Vec::new())),
                GossipOutcome::Rejected { error, .. } => {
                    return Err(VrfError::GossipRejected(error.to_string()));
                }
            }
        }
        let env = VoteEnvelope::from_bytes(&msg.payload)?;
        if address_from_public_key(&env.public_key) != env.vote.validator {
            return Err(VrfError::ValidatorAddressMismatch);
        }
        if !self.validator_set.is_active(&env.vote.validator) {
            return Err(VrfError::UnknownValidator(env.vote.validator));
        }
        crate::finality::verify_vote(&self.chain_id, &env.vote, &env.public_key)
            .map_err(|_| VrfError::InvalidConsensusVote)?;

        let active = self.validator_set.active_addresses();
        let key = (env.vote.view, env.vote.round, env.vote.block_hash);
        let agg = self
            .aggregators
            .entry(key)
            .or_insert_with(|| QcAggregator::new(self.chain_id.clone(), active.clone()));
        if let Some(qc) = agg.add_vote(env.public_key, env.vote) {
            let finalized = self
                .finality
                .submit_qc(qc.clone(), &active)
                .map_err(|_| VrfError::InvalidConsensusVote)?;
            return Ok((Some(qc), finalized));
        }
        Ok((None, Vec::new()))
    }
}

/// Four-validator (`f = 1`, quorum = 3) networked BFT cluster connected over
/// N1/N2 (`PeerIdentity` + `GossipEngine`).
pub struct FourValidatorNetwork {
    /// The four validator nodes.
    pub nodes: Vec<NetworkedValidatorNode>,
}

impl FourValidatorNetwork {
    /// Create a 4-validator genesis network with `f = 1` (quorum threshold 3).
    pub fn new(chain_id: &str, epoch_length: u64) -> Self {
        let bls_seeds: [[u8; 32]; 4] = [[11u8; 32], [22u8; 32], [33u8; 32], [44u8; 32]];
        let p2p_seeds: [[u8; 32]; 4] = [[101u8; 32], [102u8; 32], [103u8; 32], [104u8; 32]];

        let mut genesis_validators = Vec::with_capacity(4);
        let mut entries = Vec::with_capacity(4);
        for bls_seed in &bls_seeds {
            let sk = BlsSecretKey::from_seed(bls_seed);
            let pk = sk.public_key();
            let addr = address_from_public_key(&pk);
            genesis_validators.push(Validator {
                address: addr,
                stake: DEFAULT_MIN_STAKE,
                status: ValidatorStatus::Active,
                epoch: 0,
            });
            entries.push(VrfValidatorEntry {
                address: addr,
                public_key: pk,
                stake: DEFAULT_MIN_STAKE,
            });
        }

        let validator_set = ValidatorSet::new(genesis_validators, epoch_length);
        let election = VrfLeaderElection::new(entries).expect("4 genesis validators valid");
        let genesis_hash = hash_domain(chain_id, Domain::Genesis, b"b5-genesis-anchor");
        let epoch_randomness = hash_domain(chain_id, Domain::Genesis, b"b5-epoch-0-seed");

        let mut nodes: Vec<NetworkedValidatorNode> = (0..4)
            .map(|i| {
                NetworkedValidatorNode::new(
                    chain_id,
                    p2p_seeds[i],
                    bls_seeds[i],
                    validator_set.clone(),
                    election.clone(),
                    genesis_hash,
                    epoch_randomness,
                )
            })
            .collect();

        // Connect full 4-validator mesh in N2 GossipEngine.
        let peer_ids: Vec<PeerId> = nodes.iter().map(|n| n.peer_id()).collect();
        for (i, node) in nodes.iter_mut().enumerate() {
            for (j, peer_id) in peer_ids.iter().enumerate() {
                if i != j {
                    node.gossip.register_peer(*peer_id, GossipTopic::ALL);
                }
            }
        }

        Self { nodes }
    }

    /// Set whether validator `index` is online (`true`) or down/partitioned (`false`).
    pub fn set_online(&mut self, index: usize, online: bool) {
        self.nodes[index].online = online;
    }

    /// Execute one consensus round across all online validators over N2 gossip.
    ///
    /// If the VRF-elected leader for the current `(view, round)` is offline,
    /// online validators trigger a timeout view-change until an online VRF
    /// leader proposes, then exchange signed `Proposal` and `Vote` gossip
    /// messages to form a verified 3-of-4 QC.
    pub fn run_round(
        &mut self,
        timestamp_ms: u64,
        transactions: Vec<Vec<u8>>,
    ) -> Result<QuorumCertificate, VrfError> {
        loop {
            let mut proposal_opt: Option<(PeerId, GossipMessage)> = None;
            for node in &mut self.nodes {
                if let Some(msg) = node.try_propose(timestamp_ms, transactions.clone())? {
                    proposal_opt = Some((node.peer_id(), msg));
                    break;
                }
            }

            let Some((leader_peer, proposal_msg)) = proposal_opt else {
                // Elected leader is offline: timeout expires and all online nodes
                // advance to the next view in lockstep, then re-elect via VRF.
                for node in &mut self.nodes {
                    if node.online {
                        node.finality.change_view();
                    }
                }
                continue;
            };

            let mut vote_msgs: Vec<(PeerId, GossipMessage)> = Vec::new();
            for node in &mut self.nodes {
                if !node.online {
                    continue;
                }
                if let Some(vote_msg) =
                    node.on_proposal_gossip(leader_peer, &proposal_msg, timestamp_ms)?
                {
                    vote_msgs.push((node.peer_id(), vote_msg));
                }
            }

            let mut formed_qc: Option<QuorumCertificate> = None;
            for (voter_peer, vote_msg) in &vote_msgs {
                for node in &mut self.nodes {
                    if !node.online {
                        continue;
                    }
                    let (maybe_qc, _) = node.on_vote_gossip(*voter_peer, vote_msg, timestamp_ms)?;
                    if let Some(qc) = maybe_qc {
                        formed_qc = Some(qc);
                    }
                }
            }

            let qc = formed_qc.ok_or(VrfError::InvalidConsensusVote)?;
            for node in &mut self.nodes {
                if node.online {
                    node.finality.change_round();
                }
            }
            return Ok(qc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = "auralite-b5-test";

    fn key_and_entry(seed_byte: u8, stake: Stake) -> (BlsSecretKey, VrfValidatorEntry) {
        let sk = BlsSecretKey::from_seed(&[seed_byte; 32]);
        let pk = sk.public_key();
        let address = address_from_public_key(&pk);
        (
            sk,
            VrfValidatorEntry {
                address,
                public_key: pk,
                stake,
            },
        )
    }

    #[test]
    fn vrf_prove_and_verify_roundtrip() {
        let (sk, entry) = key_and_entry(1, DEFAULT_MIN_STAKE);
        let input = VrfInput::new(CHAIN, 1, Hash::from_bytes([7u8; 32]), 0, 42);
        let proof = prove_vrf(&sk, &input).expect("prove_vrf succeeds");
        assert!(verify_vrf(&entry.public_key, &input, &proof).is_ok());
    }

    #[test]
    fn vrf_rejects_wrong_public_key() {
        let (sk1, _) = key_and_entry(1, DEFAULT_MIN_STAKE);
        let (_, entry2) = key_and_entry(2, DEFAULT_MIN_STAKE);
        let input = VrfInput::new(CHAIN, 1, Hash::from_bytes([7u8; 32]), 0, 42);
        let mut proof = prove_vrf(&sk1, &input).expect("prove_vrf succeeds");
        proof.validator = entry2.address;
        assert_eq!(
            verify_vrf(&entry2.public_key, &input, &proof),
            Err(VrfError::InvalidProofSignature)
        );
    }

    #[test]
    fn vrf_rejects_tampered_output_hash() {
        let (sk, entry) = key_and_entry(1, DEFAULT_MIN_STAKE);
        let input = VrfInput::new(CHAIN, 1, Hash::from_bytes([7u8; 32]), 0, 42);
        let mut proof = prove_vrf(&sk, &input).expect("prove_vrf succeeds");
        proof.output = Hash::from_bytes([0xFF; 32]);
        assert_eq!(
            verify_vrf(&entry.public_key, &input, &proof),
            Err(VrfError::OutputMismatch)
        );
    }

    #[test]
    fn grinding_protection_rejects_zero_epoch_randomness_after_genesis_epoch() {
        let (sk, _) = key_and_entry(1, DEFAULT_MIN_STAKE);
        let bad_input = VrfInput::new(CHAIN, 1, Hash::ZERO, 0, 0);
        assert_eq!(
            prove_vrf(&sk, &bad_input),
            Err(VrfError::ZeroEpochRandomness)
        );
    }

    #[test]
    fn epoch_randomness_rejects_unverified_boundary_qc() {
        let (sk1, entry1) = key_and_entry(1, DEFAULT_MIN_STAKE);
        let (sk2, _) = key_and_entry(2, DEFAULT_MIN_STAKE);
        let block_hash = Hash::from_bytes([0xAA; 32]);
        let active = BTreeSet::from([entry1.address]);
        let payload = [
            &0u64.to_le_bytes()[..],
            &1u64.to_le_bytes()[..],
            block_hash.as_bytes(),
            &[VoteType::Precommit as u8],
        ]
        .concat();
        // Sign with sk2 but attach entry1's public key -> forged QC.
        let forged_sig = sk2.sign(CHAIN, Domain::Vote, &payload);
        let forged_qc = QuorumCertificate {
            view: 0,
            round: 1,
            block_hash,
            vote_type: VoteType::Precommit,
            public_keys: vec![entry1.public_key],
            signatures: vec![forged_sig],
        };
        assert_eq!(
            derive_epoch_randomness(Hash::from_bytes([1u8; 32]), 1, &forged_qc, CHAIN, &active),
            Err(VrfError::InvalidBoundaryQc)
        );

        // Valid QC succeeds and is deterministic.
        let valid_sig = sk1.sign(CHAIN, Domain::Vote, &payload);
        let pk1 = sk1.public_key();
        let valid_qc = QuorumCertificate {
            view: 0,
            round: 1,
            block_hash,
            vote_type: VoteType::Precommit,
            public_keys: vec![pk1],
            signatures: vec![valid_sig],
        };
        let r1 = derive_epoch_randomness(Hash::from_bytes([1u8; 32]), 1, &valid_qc, CHAIN, &active)
            .expect("valid boundary QC derives epoch randomness");
        let r2 = derive_epoch_randomness(Hash::from_bytes([1u8; 32]), 1, &valid_qc, CHAIN, &active)
            .expect("deterministic");
        let larger_active: BTreeSet<_> = (1..=4)
            .map(|i| key_and_entry(i, DEFAULT_MIN_STAKE).1.address)
            .collect();
        assert_eq!(
            derive_epoch_randomness(
                Hash::from_bytes([1u8; 32]),
                1,
                &valid_qc,
                CHAIN,
                &larger_active
            ),
            Err(VrfError::InvalidBoundaryQc)
        );
        assert_eq!(r1, r2);
        assert_ne!(r1, Hash::ZERO);
    }

    #[test]
    fn leader_claim_rejects_non_elected_validator() {
        let (sk1, entry1) = key_and_entry(1, DEFAULT_MIN_STAKE);
        let (sk2, entry2) = key_and_entry(2, DEFAULT_MIN_STAKE);
        let election = VrfLeaderElection::new(vec![entry1.clone(), entry2.clone()]).unwrap();
        let input = VrfInput::new(CHAIN, 1, Hash::from_bytes([9u8; 32]), 0, 1);
        let elected = election.select_leader(&input).unwrap();

        let (winner_sk, loser_sk, loser_addr) = if elected == entry1.address {
            (&sk1, &sk2, entry2.address)
        } else {
            (&sk2, &sk1, entry1.address)
        };

        let valid_claim = election.prove_leader_claim(winner_sk, &input).unwrap();
        assert!(election.verify_leader_claim(&valid_claim, &input).is_ok());

        let loser_proof = prove_vrf(loser_sk, &input).unwrap();
        assert_eq!(
            election.verify_leader_claim(&LeaderClaim { proof: loser_proof }, &input),
            Err(VrfError::NotElectedLeader {
                claimed: loser_addr,
                expected: elected,
            })
        );
    }

    #[test]
    fn ten_thousand_rounds_proposer_distribution_within_confidence_interval() {
        // 4 validators with unequal stakes: 10%, 20%, 30%, 40% of total stake.
        let (_, v1) = key_and_entry(10, 1_000_000);
        let (_, v2) = key_and_entry(20, 2_000_000);
        let (_, v3) = key_and_entry(30, 3_000_000);
        let (_, v4) = key_and_entry(40, 4_000_000);
        let addr1 = v1.address;
        let addr2 = v2.address;
        let addr3 = v3.address;
        let addr4 = v4.address;

        let election = VrfLeaderElection::new(vec![v1, v2, v3, v4]).unwrap();
        assert_eq!(election.total_stake(), 10_000_000);

        const ROUNDS: u64 = 10_000;
        let epoch_rand = Hash::from_bytes([0x5A; 32]);
        let mut counts: BTreeMap<Address, u64> = BTreeMap::new();
        for round in 0..ROUNDS {
            let input = VrfInput::new(CHAIN, 1, epoch_rand, 0, round);
            let leader = election.select_leader(&input).unwrap();
            *counts.entry(leader).or_insert(0) += 1;
        }

        // For N = 10,000 Bernoulli trials with p in {0.10, 0.20, 0.30, 0.40},
        // sigma = sqrt(N * p * (1 - p)) <= 49.0; a 4-sigma confidence interval
        // is +/- 200 rounds (+/- 2.0%).
        let c1 = *counts.get(&addr1).unwrap_or(&0) as i64;
        let c2 = *counts.get(&addr2).unwrap_or(&0) as i64;
        let c3 = *counts.get(&addr3).unwrap_or(&0) as i64;
        let c4 = *counts.get(&addr4).unwrap_or(&0) as i64;
        assert_eq!(c1 + c2 + c3 + c4, ROUNDS as i64);
        assert!(
            (c1 - 1_000).abs() <= 200,
            "v1 (10% stake) got {c1}/10000, outside CI [800, 1200]"
        );
        assert!(
            (c2 - 2_000).abs() <= 200,
            "v2 (20% stake) got {c2}/10000, outside CI [1800, 2200]"
        );
        assert!(
            (c3 - 3_000).abs() <= 200,
            "v3 (30% stake) got {c3}/10000, outside CI [2800, 3200]"
        );
        assert!(
            (c4 - 4_000).abs() <= 200,
            "v4 (40% stake) got {c4}/10000, outside CI [3800, 4200]"
        );
    }

    #[test]
    fn networked_four_validators_finalize_with_one_offline() {
        let mut net = FourValidatorNetwork::new(CHAIN, 100);
        // Take validator 3 offline: remaining 3 of 4 validators still form 2f+1 = 3 quorum.
        net.set_online(3, false);

        // View changes restart the same-view 3-chain, so six successful
        // rounds need not finalize three blocks when the leader times out.
        for r in 0..20u64 {
            let qc = net
                .run_round(
                    1_700_000_000_000 + r * 100,
                    vec![format!("tx-{r}").into_bytes()],
                )
                .expect("3 of 4 online validators reach quorum");
            assert_eq!(qc.signatures.len(), 3);
        }

        let f0 = net.nodes[0].finality.commit_rule.finalized_count();
        assert!(f0 >= 3, "expected at least 3 finalized blocks, got {f0}");
        for idx in 0..3 {
            assert_eq!(
                net.nodes[idx].finality.commit_rule.finalized_count(),
                f0,
                "online validator {idx} matches finalized count"
            );
            assert_eq!(
                net.nodes[idx].finality.tip.block_hash, net.nodes[0].finality.tip.block_hash,
                "online validator {idx} matches tip block hash"
            );
        }
    }
    #[test]
    fn proposal_signature_binds_block_and_validator_refuses_equivocation() {
        let mut net = FourValidatorNetwork::new(CHAIN, 100);
        let input = net.nodes[0].current_vrf_input();
        let leader_address = net.nodes[0].leader_election.select_leader(&input).unwrap();
        let leader_idx = (0..4)
            .find(|i| net.nodes[*i].address == leader_address)
            .unwrap();
        let receiver_idx = (0..4).find(|i| *i != leader_idx).unwrap();
        let impostor_idx = (0..4)
            .find(|i| *i != leader_idx && *i != receiver_idx)
            .unwrap();
        let proposal = net.nodes[leader_idx]
            .try_propose(1_700_000_000_000, vec![b"original".to_vec()])
            .unwrap()
            .unwrap();
        let mut altered = ProposalEnvelope::from_bytes(&proposal.payload).unwrap();
        let parent = net.nodes[receiver_idx].finality.tip.clone();
        altered.block = FinalityBlock::propose(
            input.view,
            input.round,
            &parent,
            1_700_000_000_000,
            vec![b"changed".to_vec()],
        );
        let (forgery, _) = net.nodes[impostor_idx]
            .gossip
            .publish(GossipTopic::Proposal, altered.to_bytes())
            .unwrap();
        let impostor_peer = net.nodes[impostor_idx].peer_id();
        assert!(matches!(
            net.nodes[receiver_idx].on_proposal_gossip(impostor_peer, &forgery, 1_700_000_000_000),
            Err(VrfError::InvalidEnvelope("proposal signature invalid"))
        ));
        let leader_peer = net.nodes[leader_idx].peer_id();
        assert!(
            net.nodes[receiver_idx]
                .on_proposal_gossip(leader_peer, &proposal, 1_700_000_000_001)
                .unwrap()
                .is_some()
        );
        altered.proposal_signature = net.nodes[leader_idx].bls_secret.sign(
            CHAIN,
            Domain::Block,
            &proposal_signing_payload(altered.epoch, altered.epoch_randomness, &altered.block),
        );
        let (equivocation, _) = net.nodes[impostor_idx]
            .gossip
            .publish(GossipTopic::Proposal, altered.to_bytes())
            .unwrap();
        assert!(matches!(
            net.nodes[receiver_idx].on_proposal_gossip(
                impostor_peer,
                &equivocation,
                1_700_000_000_002
            ),
            Err(VrfError::InvalidEnvelope("conflicting proposal for round"))
        ));
    }
}
