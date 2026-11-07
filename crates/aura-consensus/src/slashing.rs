//! B6 — Slashing & Byzantine evidence: double-sign (proposal & vote),
//! surround-vote, amnesia-vote, invalid VRF leader claim, and extended
//! downtime detection, cryptographic evidence verification, stake penalties,
//! validator jailing/tombstoning, and reporter/treasury/burn distribution
//! (`D-B4`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use aura_crypto::blake3_hash;
use aura_types::{Address, ChainId, Domain, Hash};

use crate::bls::{self, BlsPublicKey, BlsSignature};
use crate::qc::{QuorumCertificate, Vote, VoteType};
use crate::signature::BlockSignature;
use crate::validator_set::ValidatorSet;
use crate::vrf_leader::{LeaderClaim, VrfInput, VrfLeaderElection};

/// Domain tag for B6 surround-vote checkpoint attestations.
pub const SURROUND_ATTESTATION_DOMAIN: &[u8] = b"AuraLite-Crypto/consensus-surround-v1\0";

/// Domain tag for B6 canonical `SlashingEvidence` IDs.
pub const SLASHING_EVIDENCE_ID_DOMAIN: &[u8] = b"AuraLite-Crypto/consensus-slashing-evidence-v1\0";

/// Default slash rate for double-signing a block proposal (10% = 1,000 bps).
pub const DEFAULT_DOUBLE_SIGN_PROPOSAL_BPS: u16 = 1_000;

/// Default slash rate for double-signing a consensus vote (10% = 1,000 bps).
pub const DEFAULT_DOUBLE_SIGN_VOTE_BPS: u16 = 1_000;

/// Default slash rate for a surround-vote violation (15% = 1,500 bps).
pub const DEFAULT_SURROUND_VOTE_BPS: u16 = 1_500;

/// Default slash rate for an amnesia-vote violation (5% = 500 bps).
pub const DEFAULT_AMNESIA_VOTE_BPS: u16 = 500;

/// Default slash rate for an invalid VRF leader claim (5% = 500 bps).
pub const DEFAULT_INVALID_VRF_LEADER_BPS: u16 = 500;

/// Default slash rate for extended validator downtime (1% = 100 bps).
pub const DEFAULT_EXTENDED_DOWNTIME_BPS: u16 = 100;

/// Default missed-blocks threshold within a 100-block liveness window.
pub const DEFAULT_MAX_MISSED_BLOCKS: u64 = 50;

/// Default reporter reward share of slashed stake (10% = 1,000 bps).
pub const DEFAULT_REPORTER_REWARD_BPS: u16 = 1_000;

/// Default protocol treasury share of slashed stake (40% = 4,000 bps; remaining 50% burned).
pub const DEFAULT_TREASURY_SHARE_BPS: u16 = 4_000;

/// The six slashable consensus attack types covered by B6 (`D-B4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SlashingAttackKind {
    /// Attack 1: Validator signed two different block proposals at the same `(view, round)`.
    DoubleSignProposal,
    /// Attack 2: Validator signed two different BLS votes of the same type at the same `(view, round)`.
    DoubleSignVote,
    /// Attack 3: Validator signed two checkpoint attestations where `(source_a < source_b && target_b < target_a)`.
    SurroundVote,
    /// Attack 4: Validator cast a precommit at `view_b > view_a` for a conflicting block without a解锁 QC at `>= view_a`.
    AmnesiaVote,
    /// Attack 5: Validator broadcast a signed leader claim for `(view, round)` when another validator was VRF-elected.
    InvalidVrfLeaderClaim,
    /// Attack 6: Validator missed `>= max_missed_blocks_per_window` blocks in a QC-certified liveness window.
    ExtendedDowntime,
}

/// Errors returned when validating or applying [`SlashingEvidence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashingError {
    /// Slashing subsystem is disabled (`NC: slashing off -> test red`).
    SlashingDisabled,
    /// Evidence has already been processed and applied (`evidence_id`).
    DuplicateEvidence(Hash),
    /// Accused address is not a registered validator.
    UnknownValidator(Address),
    /// Validator has already been tombstoned by an earlier severe offence.
    ValidatorAlreadyTombstoned(Address),
    /// Cryptographic signature or QC inside the evidence failed verification.
    InvalidEvidenceSignature(&'static str),
    /// Evidence statements do not constitute a protocol violation (e.g. identical block hashes).
    NotAnOffence(&'static str),
}

impl fmt::Display for SlashingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlashingDisabled => f.write_str("slashing engine is disabled"),
            Self::DuplicateEvidence(id) => write!(f, "duplicate slashing evidence {id}"),
            Self::UnknownValidator(addr) => write!(f, "unknown validator {addr}"),
            Self::ValidatorAlreadyTombstoned(addr) => {
                write!(f, "validator {addr} is already tombstoned")
            }
            Self::InvalidEvidenceSignature(msg) => {
                write!(f, "invalid slashing evidence signature: {msg}")
            }
            Self::NotAnOffence(msg) => write!(f, "slashing evidence is not an offence: {msg}"),
        }
    }
}

impl std::error::Error for SlashingError {}

/// BLS-signed checkpoint attestation `(source_epoch, target_epoch, target_hash)`
/// used to prove surround-vote violations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointAttestation {
    /// Validator address.
    pub validator: Address,
    /// Source finalized epoch.
    pub source_epoch: u64,
    /// Target checkpoint epoch (`> source_epoch`).
    pub target_epoch: u64,
    /// Target block hash.
    pub target_hash: Hash,
    /// BLS12-381 signature over `(chain_id, Domain::Vote, surround_transcript)`.
    pub signature: BlsSignature,
}

impl CheckpointAttestation {
    /// Canonical signing transcript for a checkpoint attestation.
    pub fn transcript(source_epoch: u64, target_epoch: u64, target_hash: &Hash) -> Vec<u8> {
        let mut buf = Vec::with_capacity(SURROUND_ATTESTATION_DOMAIN.len() + 48);
        buf.extend_from_slice(SURROUND_ATTESTATION_DOMAIN);
        buf.extend_from_slice(&source_epoch.to_le_bytes());
        buf.extend_from_slice(&target_epoch.to_le_bytes());
        buf.extend_from_slice(target_hash.as_bytes());
        buf
    }

    /// Verify this checkpoint attestation against `public_key` and `chain_id`.
    pub fn verify(&self, chain_id: &str, public_key: &BlsPublicKey) -> Result<(), SlashingError> {
        if self.target_epoch <= self.source_epoch {
            return Err(SlashingError::NotAnOffence(
                "checkpoint attestation target_epoch must exceed source_epoch",
            ));
        }
        let msg = Self::transcript(self.source_epoch, self.target_epoch, &self.target_hash);
        bls::verify(public_key, chain_id, Domain::Vote, &msg, &self.signature).map_err(|_| {
            SlashingError::InvalidEvidenceSignature("checkpoint attestation BLS signature failed")
        })
    }
}

/// Cryptographic evidence for one of the six B6 slashable attacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashingEvidence {
    /// Attack 1: Two conflicting signed block proposals at the same `(view, round)`.
    DoubleSignProposal {
        /// Consensus view.
        view: u64,
        /// Consensus round.
        round: u64,
        /// First signed block header signature.
        first: BlockSignature,
        /// Second conflicting signed block header signature.
        second: BlockSignature,
    },
    /// Attack 2: Two conflicting BLS votes of the same `VoteType` at the same `(view, round)`.
    DoubleSignVote {
        /// First signed vote.
        first: Vote,
        /// Second conflicting signed vote.
        second: Vote,
    },
    /// Attack 3: Outer checkpoint attestation surrounds inner checkpoint attestation
    /// (`outer.source_epoch < inner.source_epoch && inner.target_epoch < outer.target_epoch`).
    SurroundVote {
        /// Outer checkpoint attestation.
        outer: CheckpointAttestation,
        /// Inner checkpoint attestation surrounded by `outer`.
        inner: CheckpointAttestation,
    },
    /// Attack 4: Amnesia vote — validator precommits `earlier` at `view_a`, then precommits
    /// a conflicting `later` block at `view_b > view_a` when `highest_justify_view < view_a`.
    AmnesiaVote {
        /// Earlier locked precommit vote at `view_a`.
        earlier: Vote,
        /// Later conflicting precommit vote at `view_b > view_a`.
        later: Vote,
        /// Highest view of any justifying QC included by the validator (`< earlier.view`).
        highest_justify_view: u64,
    },
    /// Attack 5: Validator signed a `LeaderClaim` for `(view, round)` when it was not the
    /// stake-weighted VRF leader for `vrf_input`.
    InvalidVrfLeaderClaim {
        /// VRF round input.
        vrf_input: VrfInput,
        /// Invalid or unauthorized signed leader claim.
        claim: LeaderClaim,
    },
    /// Attack 6: Validator missed `missed_blocks` out of `window_blocks` (`>= max_missed_blocks`),
    /// certified by a `2f+1` `window_qc`.
    ExtendedDowntime {
        /// Accused offline validator.
        validator: Address,
        /// Epoch of the liveness window.
        epoch: u64,
        /// Number of blocks missed in the window.
        missed_blocks: u64,
        /// Total blocks in the window.
        window_blocks: u64,
        /// Quorum certificate at the end of the liveness window certifying the missed blocks.
        window_qc: QuorumCertificate,
    },
}

impl SlashingEvidence {
    /// Identify the attack category of this evidence.
    pub const fn kind(&self) -> SlashingAttackKind {
        match self {
            Self::DoubleSignProposal { .. } => SlashingAttackKind::DoubleSignProposal,
            Self::DoubleSignVote { .. } => SlashingAttackKind::DoubleSignVote,
            Self::SurroundVote { .. } => SlashingAttackKind::SurroundVote,
            Self::AmnesiaVote { .. } => SlashingAttackKind::AmnesiaVote,
            Self::InvalidVrfLeaderClaim { .. } => SlashingAttackKind::InvalidVrfLeaderClaim,
            Self::ExtendedDowntime { .. } => SlashingAttackKind::ExtendedDowntime,
        }
    }

    /// Return the accused validator's [`Address`].
    pub const fn offender(&self) -> Address {
        match self {
            Self::DoubleSignProposal { first, .. } => first.validator,
            Self::DoubleSignVote { first, .. } => first.validator,
            Self::SurroundVote { outer, .. } => outer.validator,
            Self::AmnesiaVote { earlier, .. } => earlier.validator,
            Self::InvalidVrfLeaderClaim { claim, .. } => claim.proof.validator,
            Self::ExtendedDowntime { validator, .. } => *validator,
        }
    }

    /// Compute the canonical replay-protection digest (`evidence_id`) for this evidence.
    pub fn evidence_id(&self) -> Hash {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(SLASHING_EVIDENCE_ID_DOMAIN);
        buf.extend_from_slice(self.offender().as_bytes());
        match self {
            Self::DoubleSignProposal {
                view,
                round,
                first,
                second,
            } => {
                buf.push(1);
                buf.extend_from_slice(&view.to_le_bytes());
                buf.extend_from_slice(&round.to_le_bytes());
                buf.extend_from_slice(first.block_hash.as_bytes());
                buf.extend_from_slice(second.block_hash.as_bytes());
            }
            Self::DoubleSignVote { first, second } => {
                buf.push(2);
                buf.extend_from_slice(&first.view.to_le_bytes());
                buf.extend_from_slice(&first.round.to_le_bytes());
                buf.extend_from_slice(first.block_hash.as_bytes());
                buf.extend_from_slice(second.block_hash.as_bytes());
            }
            Self::SurroundVote { outer, inner } => {
                buf.push(3);
                buf.extend_from_slice(&outer.source_epoch.to_le_bytes());
                buf.extend_from_slice(&outer.target_epoch.to_le_bytes());
                buf.extend_from_slice(&inner.source_epoch.to_le_bytes());
                buf.extend_from_slice(&inner.target_epoch.to_le_bytes());
                buf.extend_from_slice(outer.target_hash.as_bytes());
                buf.extend_from_slice(inner.target_hash.as_bytes());
            }
            Self::AmnesiaVote {
                earlier,
                later,
                highest_justify_view,
            } => {
                buf.push(4);
                buf.extend_from_slice(&earlier.view.to_le_bytes());
                buf.extend_from_slice(&later.view.to_le_bytes());
                buf.extend_from_slice(&highest_justify_view.to_le_bytes());
                buf.extend_from_slice(earlier.block_hash.as_bytes());
                buf.extend_from_slice(later.block_hash.as_bytes());
            }
            Self::InvalidVrfLeaderClaim { vrf_input, claim } => {
                buf.push(5);
                buf.extend_from_slice(&vrf_input.epoch.to_le_bytes());
                buf.extend_from_slice(&vrf_input.view.to_le_bytes());
                buf.extend_from_slice(&vrf_input.round.to_le_bytes());
                buf.extend_from_slice(claim.proof.output.as_bytes());
            }
            Self::ExtendedDowntime {
                epoch,
                missed_blocks,
                window_blocks,
                window_qc,
                ..
            } => {
                buf.push(6);
                buf.extend_from_slice(&epoch.to_le_bytes());
                buf.extend_from_slice(&missed_blocks.to_le_bytes());
                buf.extend_from_slice(&window_blocks.to_le_bytes());
                buf.extend_from_slice(window_qc.block_hash.as_bytes());
            }
        }
        blake3_hash(&buf)
    }
}

/// Configurable penalty rates and distribution shares for B6 slashing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashingParams {
    /// Master switch for slashing enforcement (`true` by default; `false` triggers `SlashingDisabled`).
    pub slashing_enabled: bool,
    /// Slash basis points for `DoubleSignProposal` (`1_000` = 10%).
    pub double_sign_proposal_bps: u16,
    /// Slash basis points for `DoubleSignVote` (`1_000` = 10%).
    pub double_sign_vote_bps: u16,
    /// Slash basis points for `SurroundVote` (`1_500` = 15%).
    pub surround_vote_bps: u16,
    /// Slash basis points for `AmnesiaVote` (`500` = 5%).
    pub amnesia_vote_bps: u16,
    /// Slash basis points for `InvalidVrfLeaderClaim` (`500` = 5%).
    pub invalid_vrf_leader_bps: u16,
    /// Slash basis points for `ExtendedDowntime` (`100` = 1%).
    pub extended_downtime_bps: u16,
    /// Minimum missed blocks in a window required to trigger `ExtendedDowntime` slashing.
    pub max_missed_blocks_per_window: u64,
    /// Share of slashed stake paid to the evidence reporter (`1_000` = 10%).
    pub reporter_reward_bps: u16,
    /// Share of slashed stake sent to the protocol treasury (`4_000` = 40%; remainder burned).
    pub treasury_share_bps: u16,
}

impl Default for SlashingParams {
    fn default() -> Self {
        Self {
            slashing_enabled: true,
            double_sign_proposal_bps: DEFAULT_DOUBLE_SIGN_PROPOSAL_BPS,
            double_sign_vote_bps: DEFAULT_DOUBLE_SIGN_VOTE_BPS,
            surround_vote_bps: DEFAULT_SURROUND_VOTE_BPS,
            amnesia_vote_bps: DEFAULT_AMNESIA_VOTE_BPS,
            invalid_vrf_leader_bps: DEFAULT_INVALID_VRF_LEADER_BPS,
            extended_downtime_bps: DEFAULT_EXTENDED_DOWNTIME_BPS,
            max_missed_blocks_per_window: DEFAULT_MAX_MISSED_BLOCKS,
            reporter_reward_bps: DEFAULT_REPORTER_REWARD_BPS,
            treasury_share_bps: DEFAULT_TREASURY_SHARE_BPS,
        }
    }
}

impl SlashingParams {
    /// Return `(slash_bps, tombstones_validator)` for `kind`.
    pub const fn policy_for(&self, kind: SlashingAttackKind) -> (u16, bool) {
        match kind {
            SlashingAttackKind::DoubleSignProposal => (self.double_sign_proposal_bps, true),
            SlashingAttackKind::DoubleSignVote => (self.double_sign_vote_bps, true),
            SlashingAttackKind::SurroundVote => (self.surround_vote_bps, true),
            SlashingAttackKind::AmnesiaVote => (self.amnesia_vote_bps, false),
            SlashingAttackKind::InvalidVrfLeaderClaim => (self.invalid_vrf_leader_bps, false),
            SlashingAttackKind::ExtendedDowntime => (self.extended_downtime_bps, false),
        }
    }
}

/// Breakdown of how a slashed stake penalty is distributed (`reporter + treasury + burned == total_slashed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PenaltyDistribution {
    /// Total stake deducted from the offending validator.
    pub total_slashed: u64,
    /// Whistleblower reward credited to `reporter`.
    pub reporter_reward: u64,
    /// Share credited to the protocol treasury.
    pub treasury_share: u64,
    /// Share permanently burned (`total_slashed - reporter_reward - treasury_share`).
    pub burned_amount: u64,
}

impl PenaltyDistribution {
    /// Split `total_slashed` deterministically according to `params`.
    pub const fn compute(total_slashed: u64, params: &SlashingParams) -> Self {
        let reporter_reward =
            ((total_slashed as u128) * (params.reporter_reward_bps as u128) / 10_000) as u64;
        let treasury_share =
            ((total_slashed as u128) * (params.treasury_share_bps as u128) / 10_000) as u64;
        let burned_amount = total_slashed
            .saturating_sub(reporter_reward)
            .saturating_sub(treasury_share);
        Self {
            total_slashed,
            reporter_reward,
            treasury_share,
            burned_amount,
        }
    }
}

/// Receipt emitted when [`SlashingEngine`] accepts evidence and applies a penalty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashingReceipt {
    /// Unique `evidence_id` of the processed evidence.
    pub evidence_id: Hash,
    /// Slashed validator address.
    pub offender: Address,
    /// Whistleblower reporter address.
    pub reporter: Address,
    /// Attack category.
    pub attack_kind: SlashingAttackKind,
    /// Validator stake before this slash.
    pub stake_before: u64,
    /// Validator stake remaining after this slash.
    pub stake_after: u64,
    /// Whether the validator is now jailed (excluded from proposer/committee duties).
    pub jailed: bool,
    /// Whether the validator is permanently tombstoned (cannot unjail or re-enter).
    pub tombstoned: bool,
    /// Exact distribution of the slashed stake.
    pub distribution: PenaltyDistribution,
}

/// Per-validator cryptographic keys and jailing state tracked by [`SlashingEngine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorSlashingRecord {
    /// Validator address.
    pub address: Address,
    /// 32-byte Ed25519 block-signing public key (`B1`).
    pub ed25519_pubkey: [u8; 32],
    /// BLS12-381 vote/VRF public key (`B2`/`B5`).
    pub bls_pubkey: BlsPublicKey,
    /// Current slashed stake tracked in consensus.
    pub current_stake: u64,
    /// True when the validator has been jailed.
    pub jailed: bool,
    /// True when the validator has been permanently tombstoned (double-sign / surround-vote).
    pub tombstoned: bool,
    /// Number of slashing offences applied against this validator.
    pub slash_count: usize,
}

/// B6 Slashing & evidence engine enforcing cryptographic verification of all 6
/// attack types, stake deduction, validator jailing/tombstoning, and
/// reporter/treasury/burn distribution (`D-B4`).
#[derive(Debug, Clone)]
pub struct SlashingEngine {
    chain_id: ChainId,
    params: SlashingParams,
    validators: BTreeMap<Address, ValidatorSlashingRecord>,
    processed_evidence: BTreeSet<Hash>,
    reporter_balances: BTreeMap<Address, u64>,
    treasury_balance: u64,
    total_burned: u64,
    receipts: Vec<SlashingReceipt>,
}

impl SlashingEngine {
    /// Create a new [`SlashingEngine`] for `chain_id` and `params`.
    pub fn new(
        chain_id: ChainId,
        params: SlashingParams,
        validator_records: Vec<ValidatorSlashingRecord>,
    ) -> Self {
        let validators = validator_records
            .into_iter()
            .map(|r| (r.address, r))
            .collect();
        Self {
            chain_id,
            params,
            validators,
            processed_evidence: BTreeSet::new(),
            reporter_balances: BTreeMap::new(),
            treasury_balance: 0,
            total_burned: 0,
            receipts: Vec::new(),
        }
    }

    /// Borrow a validator's slashing record.
    pub fn validator(&self, address: &Address) -> Option<&ValidatorSlashingRecord> {
        self.validators.get(address)
    }

    /// Borrow all emitted [`SlashingReceipt`]s in application order.
    pub fn receipts(&self) -> &[SlashingReceipt] {
        &self.receipts
    }

    /// Total reporter rewards credited to `reporter`.
    pub fn reporter_balance(&self, reporter: &Address) -> u64 {
        self.reporter_balances.get(reporter).copied().unwrap_or(0)
    }

    /// Total slashed stake credited to the protocol treasury.
    pub const fn treasury_balance(&self) -> u64 {
        self.treasury_balance
    }

    /// Total slashed stake permanently burned.
    pub const fn total_burned(&self) -> u64 {
        self.total_burned
    }

    /// Verify `evidence`, slash the offending validator's stake (and update `validator_set`
    /// if provided), jail/tombstone the offender, and distribute the penalty.
    pub fn submit_evidence(
        &mut self,
        evidence: &SlashingEvidence,
        reporter: Address,
        mut validator_set: Option<&mut ValidatorSet>,
    ) -> Result<SlashingReceipt, SlashingError> {
        if !self.params.slashing_enabled {
            return Err(SlashingError::SlashingDisabled);
        }

        let evidence_id = evidence.evidence_id();
        if self.processed_evidence.contains(&evidence_id) {
            return Err(SlashingError::DuplicateEvidence(evidence_id));
        }

        let offender = evidence.offender();
        let record = self
            .validators
            .get(&offender)
            .ok_or(SlashingError::UnknownValidator(offender))?
            .clone();

        if record.tombstoned {
            return Err(SlashingError::ValidatorAlreadyTombstoned(offender));
        }

        // Verify the cryptographic proof for the specific attack type.
        self.verify_evidence(evidence, &record)?;

        // Compute penalty and distribution.
        let kind = evidence.kind();
        let (slash_bps, tombstones) = self.params.policy_for(kind);
        let stake_before = record.current_stake;
        let penalty_amount = ((stake_before as u128) * (slash_bps as u128) / 10_000) as u64;
        let stake_after = stake_before.saturating_sub(penalty_amount);
        let distribution = PenaltyDistribution::compute(penalty_amount, &self.params);

        // Apply state transition.
        self.processed_evidence.insert(evidence_id);
        let entry = self.validators.get_mut(&offender).unwrap();
        entry.current_stake = stake_after;
        entry.jailed = true;
        entry.tombstoned |= tombstones;
        entry.slash_count += 1;

        if let Some(ref mut vs) = validator_set {
            vs.slash_validator(&offender, penalty_amount, entry.jailed);
        }

        *self.reporter_balances.entry(reporter).or_insert(0) += distribution.reporter_reward;
        self.treasury_balance = self
            .treasury_balance
            .saturating_add(distribution.treasury_share);
        self.total_burned = self.total_burned.saturating_add(distribution.burned_amount);

        let receipt = SlashingReceipt {
            evidence_id,
            offender,
            reporter,
            attack_kind: kind,
            stake_before,
            stake_after,
            jailed: entry.jailed,
            tombstoned: entry.tombstoned,
            distribution,
        };
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }

    fn verify_evidence(
        &self,
        evidence: &SlashingEvidence,
        record: &ValidatorSlashingRecord,
    ) -> Result<(), SlashingError> {
        match evidence {
            SlashingEvidence::DoubleSignProposal { first, second, .. } => {
                if first.validator != record.address || second.validator != record.address {
                    return Err(SlashingError::NotAnOffence(
                        "double-sign proposal validators do not match offender",
                    ));
                }
                if first.public_key != record.ed25519_pubkey
                    || second.public_key != record.ed25519_pubkey
                {
                    return Err(SlashingError::InvalidEvidenceSignature(
                        "double-sign proposal public key does not match registered validator key",
                    ));
                }
                if first.block_hash == second.block_hash {
                    return Err(SlashingError::NotAnOffence(
                        "double-sign proposals must sign two distinct block hashes",
                    ));
                }
                first.verify(self.chain_id.as_str()).map_err(|_| {
                    SlashingError::InvalidEvidenceSignature(
                        "first proposal signature failed verification",
                    )
                })?;
                second.verify(self.chain_id.as_str()).map_err(|_| {
                    SlashingError::InvalidEvidenceSignature(
                        "second proposal signature failed verification",
                    )
                })?;
                Ok(())
            }
            SlashingEvidence::DoubleSignVote { first, second } => {
                if first.validator != record.address || second.validator != record.address {
                    return Err(SlashingError::NotAnOffence(
                        "double-sign vote validators do not match offender",
                    ));
                }
                if first.view != second.view
                    || first.round != second.round
                    || first.vote_type != second.vote_type
                {
                    return Err(SlashingError::NotAnOffence(
                        "double-sign votes must share the same (view, round, vote_type)",
                    ));
                }
                if first.block_hash == second.block_hash {
                    return Err(SlashingError::NotAnOffence(
                        "double-sign votes must target two distinct block hashes",
                    ));
                }
                Self::verify_vote_sig(self.chain_id.as_str(), &record.bls_pubkey, first)?;
                Self::verify_vote_sig(self.chain_id.as_str(), &record.bls_pubkey, second)?;
                Ok(())
            }
            SlashingEvidence::SurroundVote { outer, inner } => {
                if outer.validator != record.address || inner.validator != record.address {
                    return Err(SlashingError::NotAnOffence(
                        "surround-vote validators do not match offender",
                    ));
                }
                if !(outer.source_epoch < inner.source_epoch
                    && inner.target_epoch < outer.target_epoch)
                {
                    return Err(SlashingError::NotAnOffence(
                        "attestation pair does not satisfy surround inequality (s_outer < s_inner && t_inner < t_outer)",
                    ));
                }
                outer.verify(self.chain_id.as_str(), &record.bls_pubkey)?;
                inner.verify(self.chain_id.as_str(), &record.bls_pubkey)?;
                Ok(())
            }
            SlashingEvidence::AmnesiaVote {
                earlier,
                later,
                highest_justify_view,
            } => {
                if earlier.validator != record.address || later.validator != record.address {
                    return Err(SlashingError::NotAnOffence(
                        "amnesia-vote validators do not match offender",
                    ));
                }
                if earlier.vote_type != VoteType::Precommit
                    || later.vote_type != VoteType::Precommit
                {
                    return Err(SlashingError::NotAnOffence(
                        "amnesia-vote requires two Precommit votes",
                    ));
                }
                if later.view <= earlier.view
                    || earlier.block_hash == later.block_hash
                    || *highest_justify_view >= earlier.view
                {
                    return Err(SlashingError::NotAnOffence(
                        "amnesia-vote requires later.view > earlier.view, distinct blocks, and highest_justify_view < earlier.view",
                    ));
                }
                Self::verify_vote_sig(self.chain_id.as_str(), &record.bls_pubkey, earlier)?;
                Self::verify_vote_sig(self.chain_id.as_str(), &record.bls_pubkey, later)?;
                Ok(())
            }
            SlashingEvidence::InvalidVrfLeaderClaim { vrf_input, claim } => {
                if claim.proof.validator != record.address {
                    return Err(SlashingError::NotAnOffence(
                        "leader claim validator does not match offender",
                    ));
                }
                let vrf_entries = self
                    .validators
                    .values()
                    .filter(|v| !v.jailed && v.current_stake > 0)
                    .map(|v| crate::vrf_leader::VrfValidatorEntry {
                        address: v.address,
                        public_key: v.bls_pubkey.clone(),
                        stake: v.current_stake,
                    })
                    .collect();
                let election = VrfLeaderElection::new(vrf_entries).map_err(|_| {
                    SlashingError::NotAnOffence("unable to form VRF election table")
                })?;
                let transcript = vrf_input.transcript_bytes();
                bls::verify(
                    &record.bls_pubkey,
                    self.chain_id.as_str(),
                    Domain::Block,
                    &transcript,
                    &claim.proof.signature,
                )
                .map_err(|_| {
                    SlashingError::InvalidEvidenceSignature(
                        "leader claim was not signed by the accused validator",
                    )
                })?;
                if election.verify_leader_claim(claim, vrf_input).is_ok() {
                    return Err(SlashingError::NotAnOffence(
                        "leader claim is valid and elected; not slashable",
                    ));
                }
                Ok(())
            }
            SlashingEvidence::ExtendedDowntime {
                missed_blocks,
                window_blocks,
                window_qc,
                ..
            } => {
                if *missed_blocks < self.params.max_missed_blocks_per_window
                    || *missed_blocks > *window_blocks
                {
                    return Err(SlashingError::NotAnOffence(
                        "missed_blocks is below max_missed_blocks_per_window threshold",
                    ));
                }
                window_qc
                    .verify(self.chain_id.as_str(), Domain::Vote)
                    .map_err(|_| {
                        SlashingError::InvalidEvidenceSignature(
                            "downtime window QC failed BLS verification",
                        )
                    })?;
                Ok(())
            }
        }
    }

    fn verify_vote_sig(
        chain_id: &str,
        public_key: &BlsPublicKey,
        vote: &Vote,
    ) -> Result<(), SlashingError> {
        let mut msg = Vec::with_capacity(49);
        msg.extend_from_slice(&vote.view.to_le_bytes());
        msg.extend_from_slice(&vote.round.to_le_bytes());
        msg.extend_from_slice(vote.block_hash.as_bytes());
        msg.push(vote.vote_type as u8);
        bls::verify(public_key, chain_id, Domain::Vote, &msg, &vote.signature)
            .map_err(|_| SlashingError::InvalidEvidenceSignature("vote BLS signature failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::BlsSecretKey;
    use crate::qc::QcAggregator;
    use crate::vrf_leader::prove_vrf;
    use aura_crypto::ed25519_public_key;

    fn test_chain_id() -> ChainId {
        ChainId::new("auralite-b6-unit").unwrap()
    }

    struct TestVal {
        ed_seed: [u8; 32],
        bls_sk: BlsSecretKey,
        record: ValidatorSlashingRecord,
    }

    fn make_validators(count: usize) -> Vec<TestVal> {
        (0..count)
            .map(|i| {
                let seed = [(i as u8) + 1; 32];
                let bls_sk = BlsSecretKey::from_seed(&seed);
                let bls_pk = bls_sk.public_key();
                let address = bls::address_from_public_key(&bls_pk);
                let ed_pk = ed25519_public_key(&seed);
                TestVal {
                    ed_seed: seed,
                    bls_sk,
                    record: ValidatorSlashingRecord {
                        address,
                        ed25519_pubkey: ed_pk,
                        bls_pubkey: bls_pk,
                        current_stake: 10_000_000,
                        jailed: false,
                        tombstoned: false,
                        slash_count: 0,
                    },
                }
            })
            .collect()
    }

    fn sign_vote(
        val: &TestVal,
        chain_id: &ChainId,
        view: u64,
        round: u64,
        block_hash: Hash,
        vote_type: VoteType,
    ) -> Vote {
        let mut msg = Vec::with_capacity(49);
        msg.extend_from_slice(&view.to_le_bytes());
        msg.extend_from_slice(&round.to_le_bytes());
        msg.extend_from_slice(block_hash.as_bytes());
        msg.push(vote_type as u8);
        let sig = val.bls_sk.sign(chain_id.as_str(), Domain::Vote, &msg);
        Vote {
            view,
            round,
            block_hash,
            validator: val.record.address,
            vote_type,
            signature: sig,
        }
    }

    fn sign_checkpoint(
        val: &TestVal,
        chain_id: &ChainId,
        source_epoch: u64,
        target_epoch: u64,
        target_hash: Hash,
    ) -> CheckpointAttestation {
        let msg = CheckpointAttestation::transcript(source_epoch, target_epoch, &target_hash);
        let signature = val.bls_sk.sign(chain_id.as_str(), Domain::Vote, &msg);
        CheckpointAttestation {
            validator: val.record.address,
            source_epoch,
            target_epoch,
            target_hash,
            signature,
        }
    }

    fn build_six_attack_evidences(vals: &[TestVal], chain_id: &ChainId) -> Vec<SlashingEvidence> {
        // 1. DoubleSignProposal by validator 0
        let p1 = BlockSignature::sign_block(
            &vals[0].ed_seed,
            vals[0].record.address,
            Hash::from_bytes([0x11; 32]),
            chain_id.as_str(),
        );
        let p2 = BlockSignature::sign_block(
            &vals[0].ed_seed,
            vals[0].record.address,
            Hash::from_bytes([0x22; 32]),
            chain_id.as_str(),
        );
        let ev1 = SlashingEvidence::DoubleSignProposal {
            view: 10,
            round: 1,
            first: p1,
            second: p2,
        };

        // 2. DoubleSignVote by validator 1
        let v1 = sign_vote(
            &vals[1],
            chain_id,
            10,
            1,
            Hash::from_bytes([0x33; 32]),
            VoteType::Precommit,
        );
        let v2 = sign_vote(
            &vals[1],
            chain_id,
            10,
            1,
            Hash::from_bytes([0x44; 32]),
            VoteType::Precommit,
        );
        let ev2 = SlashingEvidence::DoubleSignVote {
            first: v1,
            second: v2,
        };

        // 3. SurroundVote by validator 2 (outer 1..10 surrounds inner 3..7)
        let outer = sign_checkpoint(&vals[2], chain_id, 1, 10, Hash::from_bytes([0x55; 32]));
        let inner = sign_checkpoint(&vals[2], chain_id, 3, 7, Hash::from_bytes([0x66; 32]));
        let ev3 = SlashingEvidence::SurroundVote { outer, inner };

        // 4. AmnesiaVote by validator 3
        let earlier = sign_vote(
            &vals[3],
            chain_id,
            12,
            0,
            Hash::from_bytes([0x77; 32]),
            VoteType::Precommit,
        );
        let later = sign_vote(
            &vals[3],
            chain_id,
            15,
            0,
            Hash::from_bytes([0x88; 32]),
            VoteType::Precommit,
        );
        let ev4 = SlashingEvidence::AmnesiaVote {
            earlier,
            later,
            highest_justify_view: 10,
        };

        // 5. InvalidVrfLeaderClaim by validator 4 (forge output hash so verify_leader_claim fails)
        let vrf_input = VrfInput::new(chain_id.as_str(), 1, Hash::from_bytes([0x99; 32]), 20, 0);
        let mut proof = prove_vrf(&vals[4].bls_sk, &vrf_input).unwrap();
        proof.output = Hash::from_bytes([0xFE; 32]);
        let ev5 = SlashingEvidence::InvalidVrfLeaderClaim {
            vrf_input,
            claim: LeaderClaim { proof },
        };

        // 6. ExtendedDowntime by validator 5
        let qc_hash = Hash::from_bytes([0xCD; 32]);
        let mut agg = QcAggregator::with_signers(
            chain_id.as_str(),
            &vals[0..4]
                .iter()
                .map(|v| v.record.bls_pubkey.clone())
                .collect::<Vec<_>>(),
        );
        let mut window_qc = None;
        for signer in &vals[0..4] {
            let vote = sign_vote(signer, chain_id, 25, 0, qc_hash, VoteType::Precommit);
            if let Some(qc) = agg.add_vote(signer.record.bls_pubkey.clone(), vote) {
                window_qc = Some(qc);
            }
        }
        let ev6 = SlashingEvidence::ExtendedDowntime {
            validator: vals[5].record.address,
            epoch: 2,
            missed_blocks: 60,
            window_blocks: 100,
            window_qc: window_qc.unwrap(),
        };

        vec![ev1, ev2, ev3, ev4, ev5, ev6]
    }

    #[test]
    fn six_attack_types_are_accepted_and_apply_penalties() {
        let chain_id = test_chain_id();
        let vals = make_validators(6);
        let records = vals.iter().map(|v| v.record.clone()).collect();
        let mut engine = SlashingEngine::new(chain_id.clone(), SlashingParams::default(), records);
        let reporter = Address::from_bytes([0xAA; 20]);

        let evidences = build_six_attack_evidences(&vals, &chain_id);
        assert_eq!(evidences.len(), 6);

        let expected_slashed = [
            1_000_000u64,
            1_000_000,
            1_500_000,
            500_000,
            500_000,
            100_000,
        ];
        let expected_tombstone = [true, true, true, false, false, false];

        for (idx, ev) in evidences.iter().enumerate() {
            let receipt = engine.submit_evidence(ev, reporter, None).unwrap();
            assert_eq!(receipt.offender, vals[idx].record.address);
            assert_eq!(receipt.distribution.total_slashed, expected_slashed[idx]);
            assert_eq!(receipt.stake_after, 10_000_000 - expected_slashed[idx]);
            assert!(receipt.jailed);
            assert_eq!(receipt.tombstoned, expected_tombstone[idx]);
        }

        assert_eq!(engine.receipts().len(), 6);
        // Total slashed across the 6 attacks = 4,600,000 (10% reporter = 460,000; 40% treasury = 1,840,000; 50% burned = 2,300,000).
        assert_eq!(engine.reporter_balance(&reporter), 460_000);
        assert_eq!(engine.treasury_balance(), 1_840_000);
        assert_eq!(engine.total_burned(), 2_300_000);
    }

    #[test]
    fn double_sign_requires_distinct_blocks_and_valid_signatures() {
        let chain_id = test_chain_id();
        let vals = make_validators(4);
        let records = vals.iter().map(|v| v.record.clone()).collect();
        let mut engine = SlashingEngine::new(chain_id.clone(), SlashingParams::default(), records);
        let reporter = Address::from_bytes([0xBB; 20]);

        // Same block hash on both votes is NOT an offence.
        let same_hash = Hash::from_bytes([0x33; 32]);
        let v1 = sign_vote(&vals[0], &chain_id, 5, 0, same_hash, VoteType::Precommit);
        let v2 = sign_vote(&vals[0], &chain_id, 5, 0, same_hash, VoteType::Precommit);
        assert!(matches!(
            engine.submit_evidence(
                &SlashingEvidence::DoubleSignVote {
                    first: v1,
                    second: v2,
                },
                reporter,
                None
            ),
            Err(SlashingError::NotAnOffence(_))
        ));

        // Forged signature on second vote must be rejected.
        let va = sign_vote(
            &vals[0],
            &chain_id,
            5,
            0,
            Hash::from_bytes([0x33; 32]),
            VoteType::Precommit,
        );
        let mut vb = sign_vote(
            &vals[1],
            &chain_id,
            5,
            0,
            Hash::from_bytes([0x44; 32]),
            VoteType::Precommit,
        );
        vb.validator = vals[0].record.address;
        assert!(matches!(
            engine.submit_evidence(
                &SlashingEvidence::DoubleSignVote {
                    first: va,
                    second: vb,
                },
                reporter,
                None
            ),
            Err(SlashingError::InvalidEvidenceSignature(_))
        ));
    }

    #[test]
    fn surround_vote_requires_strict_outer_inner_span_and_valid_bls_signatures() {
        let chain_id = test_chain_id();
        let vals = make_validators(4);
        let records = vals.iter().map(|v| v.record.clone()).collect();
        let mut engine = SlashingEngine::new(chain_id.clone(), SlashingParams::default(), records);
        let reporter = Address::from_bytes([0xCC; 20]);

        // Non-surrounding epochs (1..5 and 6..10) must be rejected.
        let a = sign_checkpoint(&vals[0], &chain_id, 1, 5, Hash::from_bytes([0x11; 32]));
        let b = sign_checkpoint(&vals[0], &chain_id, 6, 10, Hash::from_bytes([0x22; 32]));
        assert!(matches!(
            engine.submit_evidence(
                &SlashingEvidence::SurroundVote { outer: a, inner: b },
                reporter,
                None
            ),
            Err(SlashingError::NotAnOffence(_))
        ));
    }

    #[test]
    fn downtime_slashing_requires_missed_blocks_threshold_and_valid_qc() {
        let chain_id = test_chain_id();
        let vals = make_validators(4);
        let records = vals.iter().map(|v| v.record.clone()).collect();
        let mut engine = SlashingEngine::new(chain_id.clone(), SlashingParams::default(), records);
        let reporter = Address::from_bytes([0xDD; 20]);

        let qc_hash = Hash::from_bytes([0xCD; 32]);
        let mut agg = QcAggregator::with_signers(
            chain_id.as_str(),
            &vals[0..3]
                .iter()
                .map(|v| v.record.bls_pubkey.clone())
                .collect::<Vec<_>>(),
        );
        let mut window_qc = None;
        for signer in &vals[0..3] {
            let vote = sign_vote(signer, &chain_id, 10, 0, qc_hash, VoteType::Precommit);
            if let Some(qc) = agg.add_vote(signer.record.bls_pubkey.clone(), vote) {
                window_qc = Some(qc);
            }
        }

        // Missed 10 < 50 threshold -> rejected.
        let below_threshold = SlashingEvidence::ExtendedDowntime {
            validator: vals[3].record.address,
            epoch: 1,
            missed_blocks: 10,
            window_blocks: 100,
            window_qc: window_qc.unwrap(),
        };
        assert!(matches!(
            engine.submit_evidence(&below_threshold, reporter, None),
            Err(SlashingError::NotAnOffence(_))
        ));
    }

    #[test]
    fn penalty_distribution_conserves_slashed_stake_across_reporter_treasury_and_burn() {
        let params = SlashingParams::default();
        let dist = PenaltyDistribution::compute(1_500_000, &params);
        assert_eq!(dist.reporter_reward, 150_000);
        assert_eq!(dist.treasury_share, 600_000);
        assert_eq!(dist.burned_amount, 750_000);
        assert_eq!(
            dist.reporter_reward + dist.treasury_share + dist.burned_amount,
            dist.total_slashed
        );
    }

    #[test]
    fn duplicate_evidence_replay_is_rejected() {
        let chain_id = test_chain_id();
        let vals = make_validators(6);
        let records = vals.iter().map(|v| v.record.clone()).collect();
        let mut engine = SlashingEngine::new(chain_id.clone(), SlashingParams::default(), records);
        let reporter = Address::from_bytes([0xEE; 20]);

        let ev = build_six_attack_evidences(&vals, &chain_id)
            .into_iter()
            .next()
            .unwrap();
        let first_receipt = engine.submit_evidence(&ev, reporter, None).unwrap();
        assert_eq!(
            engine.submit_evidence(&ev, reporter, None),
            Err(SlashingError::DuplicateEvidence(first_receipt.evidence_id))
        );
    }
}
