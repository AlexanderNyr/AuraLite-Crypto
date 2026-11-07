//! B9 — On-chain governance: proposal → vote → timelock → runtime upgrade (WASM),
//! and emergency pause.
//!
//! Implements the B9 requirements from `CONSENSUS_PLAN.md`:
//! - **Proposal submission (`GovernanceProposal`, `ProposalPayload`):** deposits
//!   at least `min_proposer_deposit`, validates payload invariants (including
//!   WASM bytecode magic `\0asm\x01\x00\x00\x00`, `BLAKE3` code hash, and strictly
//!   increasing protocol version `to_version > from_version`), and opens the
//!   voting window `[submit_height, submit_height + voting_period_blocks]`.
//! - **Stake-weighted BLS voting (`GovernanceVote`, `GovernanceVoteChoice`):**
//!   active validators sign domain-separated votes (`Domain::Vote`) supporting
//!   `Yes`, `No`, `Abstain`, or `NoWithVeto`.
//! - **Tally & timelock queue (`TallyResult`, `ProposalStatus::Timelocked`):**
//!   enforces `veto_threshold_bps`, `quorum_bps`, and `pass_threshold_bps`
//!   supermajority; passing proposals enter a mandatory timelock queue for at
//!   least `timelock_delay_blocks` before execution.
//! - **Hot WASM runtime upgrade (`ActiveRuntimeState`, `HotUpgradableTestnet`):**
//!   activates the new WASM runtime and protocol version at the scheduled height
//!   across all testnet validators without a manual validator restart.
//! - **Emergency pause (`EmergencyPauseState`):** `2f+1` BLS `QuorumCertificate`
//!   activates or lifts an emergency pause that halts governance execution until
//!   explicitly resumed.

use std::collections::BTreeMap;
use std::fmt;

use aura_crypto::blake3_hash;
use aura_types::{Address, ChainId, Domain, Hash};

use crate::bls::{BlsPublicKey, BlsSecretKey, BlsSignature, verify};
use crate::finality::FinalityEngine;
use crate::qc::{QcAggregator, QuorumCertificate, VoteType, quorum_threshold};
use crate::validator_set::{Validator, ValidatorSet, ValidatorStatus};

/// Canonical WebAssembly binary magic (`\0asm`) + version 1 (`0x01 0x00 0x00 0x00`).
pub const WASM_MAGIC_AND_VERSION: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

/// Maximum allowed runtime WASM payload size in bytes (2 MiB).
pub const MAX_RUNTIME_WASM_BYTES: usize = 2 * 1024 * 1024;

/// Default minimum proposer deposit (`100_000` smallest units).
pub const DEFAULT_MIN_PROPOSER_DEPOSIT: u128 = 100_000;

/// Default voting window length in blocks (`10` blocks).
pub const DEFAULT_VOTING_PERIOD_BLOCKS: u64 = 10;

/// Default mandatory timelock delay in blocks after voting ends (`5` blocks).
pub const DEFAULT_TIMELOCK_DELAY_BLOCKS: u64 = 5;

/// Default minimum quorum in basis points (`3_334` bps = 33.34% of active stake).
pub const DEFAULT_QUORUM_BPS: u16 = 3_334;

/// Default `Yes` supermajority threshold in basis points (`6_667` bps = 66.67% of non-abstain stake).
pub const DEFAULT_PASS_THRESHOLD_BPS: u16 = 6_667;

/// Default `NoWithVeto` threshold in basis points (`3_334` bps = 33.34% of total cast stake).
pub const DEFAULT_VETO_THRESHOLD_BPS: u16 = 3_334;

/// Basis points denominator (`10_000` = 100%).
pub const BPS_DENOMINATOR: u16 = 10_000;

/// On-chain governance parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceParams {
    /// Minimum deposit required to submit a proposal.
    pub min_proposer_deposit: u128,
    /// Duration of the voting phase in blocks.
    pub voting_period_blocks: u64,
    /// Mandatory delay in blocks between tally pass and execution.
    pub timelock_delay_blocks: u64,
    /// Minimum participation quorum in basis points of total active stake.
    pub quorum_bps: u16,
    /// Minimum `Yes` share in basis points of `(Yes + No + NoWithVeto)` stake.
    pub pass_threshold_bps: u16,
    /// `NoWithVeto` share in basis points of total cast stake that vetoes a proposal.
    pub veto_threshold_bps: u16,
}

impl Default for GovernanceParams {
    fn default() -> Self {
        Self {
            min_proposer_deposit: DEFAULT_MIN_PROPOSER_DEPOSIT,
            voting_period_blocks: DEFAULT_VOTING_PERIOD_BLOCKS,
            timelock_delay_blocks: DEFAULT_TIMELOCK_DELAY_BLOCKS,
            quorum_bps: DEFAULT_QUORUM_BPS,
            pass_threshold_bps: DEFAULT_PASS_THRESHOLD_BPS,
            veto_threshold_bps: DEFAULT_VETO_THRESHOLD_BPS,
        }
    }
}

impl GovernanceParams {
    /// Validate that all governance parameters are within sane bounds.
    pub fn validate(&self) -> Result<(), GovernanceError> {
        if self.voting_period_blocks == 0
            || self.timelock_delay_blocks == 0
            || self.quorum_bps == 0
            || self.quorum_bps > BPS_DENOMINATOR
            || self.pass_threshold_bps == 0
            || self.pass_threshold_bps > BPS_DENOMINATOR
            || self.veto_threshold_bps == 0
            || self.veto_threshold_bps > BPS_DENOMINATOR
        {
            return Err(GovernanceError::InvalidGovernanceParams);
        }
        Ok(())
    }
}

/// Errors returned by on-chain governance operations (`B9`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceError {
    /// Governance parameters are invalid (zero period or bps > 10,000).
    InvalidGovernanceParams,
    /// Proposer deposit is below `min_proposer_deposit`.
    InsufficientDeposit {
        /// Provided deposit.
        provided: u128,
        /// Required minimum deposit.
        required: u128,
    },
    /// Runtime upgrade target version does not strictly advance current version.
    InvalidUpgradeVersion {
        /// Current or declared `from_version`.
        from_version: u32,
        /// Requested `to_version`.
        to_version: u32,
    },
    /// WASM runtime bytecode is empty, exceeds `MAX_RUNTIME_WASM_BYTES`, or lacks `\0asm\x01\0\0\0`.
    InvalidWasmBytecode,
    /// Declared `wasm_code_hash` does not match `BLAKE3(wasm_bytecode)`.
    WasmCodeHashMismatch {
        /// Declared code hash in proposal.
        declared: Hash,
        /// Computed `BLAKE3` code hash.
        computed: Hash,
    },
    /// Referenced proposal ID does not exist.
    ProposalNotFound(u64),
    /// Proposal is not in the expected status for the requested transition.
    InvalidProposalStatus {
        /// Proposal ID.
        proposal_id: u64,
        /// Current status name.
        status: &'static str,
    },
    /// Vote was cast outside the proposal's `[submit_height, voting_end_height]` window.
    VotingWindowClosed {
        /// Current block height.
        current_height: u64,
        /// Last height at which votes are accepted.
        voting_end_height: u64,
    },
    /// Tally was requested before `voting_end_height` elapsed.
    VotingStillOpen {
        /// Current block height.
        current_height: u64,
        /// Height at which voting closes.
        voting_end_height: u64,
    },
    /// Voter is not an active validator in the validator set.
    UnknownOrInactiveValidator(Address),
    /// Voter's BLS signature over the governance vote failed verification.
    InvalidVoteSignature(Address),
    /// Timelock delay has not yet elapsed for a passed proposal.
    TimelockNotExpired {
        /// Current block height.
        current_height: u64,
        /// Earliest block height at which execution is allowed.
        earliest_execution_height: u64,
    },
    /// Governance execution is currently halted by an active emergency pause.
    EmergencyPaused {
        /// Reason recorded when emergency pause was activated.
        reason: String,
    },
    /// Emergency pause/resume QC failed `2f+1` validator verification.
    InvalidEmergencyQc,
}

impl fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGovernanceParams => write!(f, "invalid governance parameters"),
            Self::InsufficientDeposit { provided, required } => {
                write!(
                    f,
                    "insufficient proposal deposit: provided {provided}, required {required}"
                )
            }
            Self::InvalidUpgradeVersion {
                from_version,
                to_version,
            } => write!(
                f,
                "invalid upgrade version transition: {from_version} -> {to_version}"
            ),
            Self::InvalidWasmBytecode => write!(f, "invalid WASM runtime bytecode"),
            Self::WasmCodeHashMismatch { declared, computed } => write!(
                f,
                "WASM code hash mismatch: declared {declared}, computed {computed}"
            ),
            Self::ProposalNotFound(id) => write!(f, "governance proposal {id} not found"),
            Self::InvalidProposalStatus {
                proposal_id,
                status,
            } => write!(
                f,
                "proposal {proposal_id} has invalid status '{status}' for this operation"
            ),
            Self::VotingWindowClosed {
                current_height,
                voting_end_height,
            } => write!(
                f,
                "voting window closed at height {voting_end_height} (current {current_height})"
            ),
            Self::VotingStillOpen {
                current_height,
                voting_end_height,
            } => write!(
                f,
                "voting still open until height {voting_end_height} (current {current_height})"
            ),
            Self::UnknownOrInactiveValidator(addr) => {
                write!(f, "validator {addr} is unknown or inactive")
            }
            Self::InvalidVoteSignature(addr) => {
                write!(f, "invalid governance vote signature from {addr}")
            }
            Self::TimelockNotExpired {
                current_height,
                earliest_execution_height,
            } => write!(
                f,
                "timelock not expired: current height {current_height} < {earliest_execution_height}"
            ),
            Self::EmergencyPaused { reason } => {
                write!(
                    f,
                    "governance execution halted by emergency pause: {reason}"
                )
            }
            Self::InvalidEmergencyQc => write!(f, "invalid 2f+1 QC for emergency pause/resume"),
        }
    }
}

impl std::error::Error for GovernanceError {}

/// Executable payload carried by a [`GovernanceProposal`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalPayload {
    /// Hot WebAssembly runtime upgrade without a manual validator restart.
    RuntimeWasmUpgrade {
        /// Expected current protocol version.
        from_version: u32,
        /// Target protocol version (`to_version > from_version`).
        to_version: u32,
        /// Deterministic WASM module bytecode (`\0asm\x01\0\0\0...`).
        wasm_bytecode: Vec<u8>,
        /// Pinned `BLAKE3` digest of `wasm_bytecode`.
        wasm_code_hash: Hash,
        /// Minimum block height at which the upgrade may activate.
        activation_height: u64,
    },
    /// On-chain governance parameter update.
    ParameterChange {
        /// New governance parameter values.
        updated_params: GovernanceParams,
    },
}

impl ProposalPayload {
    /// Validate payload integrity against the current active protocol version.
    pub fn validate(&self, current_protocol_version: u32) -> Result<(), GovernanceError> {
        match self {
            Self::RuntimeWasmUpgrade {
                from_version,
                to_version,
                wasm_bytecode,
                wasm_code_hash,
                ..
            } => {
                if *from_version != current_protocol_version || *to_version <= *from_version {
                    return Err(GovernanceError::InvalidUpgradeVersion {
                        from_version: *from_version,
                        to_version: *to_version,
                    });
                }
                if wasm_bytecode.len() < WASM_MAGIC_AND_VERSION.len()
                    || wasm_bytecode.len() > MAX_RUNTIME_WASM_BYTES
                    || wasm_bytecode[..8] != WASM_MAGIC_AND_VERSION
                {
                    return Err(GovernanceError::InvalidWasmBytecode);
                }
                let computed = blake3_hash(wasm_bytecode);
                if computed != *wasm_code_hash {
                    return Err(GovernanceError::WasmCodeHashMismatch {
                        declared: *wasm_code_hash,
                        computed,
                    });
                }
                Ok(())
            }
            Self::ParameterChange { updated_params } => updated_params.validate(),
        }
    }
}

/// Validator vote choice on a governance proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceVoteChoice {
    /// Approve the proposal.
    Yes,
    /// Oppose the proposal without vetoing.
    No,
    /// Contribute to quorum only without affecting the `Yes` / `No` ratio.
    Abstain,
    /// Oppose and vote to veto (burning proposer deposit if veto threshold is reached).
    NoWithVeto,
}

impl GovernanceVoteChoice {
    /// Canonical byte tag used in domain-separated vote signing.
    pub const fn tag(self) -> u8 {
        match self {
            Self::Yes => 1,
            Self::No => 2,
            Self::Abstain => 3,
            Self::NoWithVeto => 4,
        }
    }
}

/// Signed validator vote on a governance proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceVote {
    /// Target proposal ID.
    pub proposal_id: u64,
    /// Validator address.
    pub voter: Address,
    /// Vote choice (`Yes`, `No`, `Abstain`, `NoWithVeto`).
    pub choice: GovernanceVoteChoice,
    /// Height at which the vote was cast.
    pub cast_height: u64,
    /// BLS12-381 signature over `(chain_id, Domain::Vote, "b9-gov-vote" || proposal_id || voter || choice)`.
    pub signature: BlsSignature,
}

impl GovernanceVote {
    /// Construct and BLS-sign a governance vote.
    pub fn sign(
        sk: &BlsSecretKey,
        voter: Address,
        chain_id: &ChainId,
        proposal_id: u64,
        choice: GovernanceVoteChoice,
        cast_height: u64,
    ) -> Self {
        let msg = Self::signing_bytes(proposal_id, &voter, choice);
        let signature = sk.sign(chain_id.as_str(), Domain::Vote, &msg);
        Self {
            proposal_id,
            voter,
            choice,
            cast_height,
            signature,
        }
    }

    /// Verify the vote's BLS signature against `pk`.
    pub fn verify(&self, pk: &BlsPublicKey, chain_id: &ChainId) -> bool {
        let msg = Self::signing_bytes(self.proposal_id, &self.voter, self.choice);
        verify(pk, chain_id.as_str(), Domain::Vote, &msg, &self.signature).is_ok()
    }

    fn signing_bytes(proposal_id: u64, voter: &Address, choice: GovernanceVoteChoice) -> Vec<u8> {
        let mut buf = Vec::with_capacity(11 + 8 + 20 + 1);
        buf.extend_from_slice(b"b9-gov-vote");
        buf.extend_from_slice(&proposal_id.to_be_bytes());
        buf.extend_from_slice(voter.as_bytes());
        buf.push(choice.tag());
        buf
    }
}

/// Stake-weighted tally summary for a governance proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyResult {
    /// Total active validator stake at tally time.
    pub total_active_stake: u128,
    /// Stake voting `Yes`.
    pub yes_stake: u128,
    /// Stake voting `No`.
    pub no_stake: u128,
    /// Stake voting `Abstain`.
    pub abstain_stake: u128,
    /// Stake voting `NoWithVeto`.
    pub no_with_veto_stake: u128,
}

impl TallyResult {
    /// Total participating stake (`yes + no + abstain + no_with_veto`).
    pub const fn total_participating_stake(&self) -> u128 {
        self.yes_stake
            .saturating_add(self.no_stake)
            .saturating_add(self.abstain_stake)
            .saturating_add(self.no_with_veto_stake)
    }

    /// Total non-abstain stake (`yes + no + no_with_veto`).
    pub const fn non_abstain_stake(&self) -> u128 {
        self.yes_stake
            .saturating_add(self.no_stake)
            .saturating_add(self.no_with_veto_stake)
    }
}

/// Lifecycle state of an on-chain governance proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Open for validator votes until `voting_end_height`.
    Voting,
    /// Passed voting and queued in the mandatory timelock until `earliest_execution_height`.
    Timelocked {
        /// Earliest block height at which the proposal may be executed.
        earliest_execution_height: u64,
    },
    /// Executed at `executed_height` (deposit refunded).
    Executed {
        /// Block height at which the proposal was executed.
        executed_height: u64,
    },
    /// Rejected because total participating stake was below `quorum_bps` (deposit refunded).
    RejectedQuorum,
    /// Rejected because `Yes` stake was below `pass_threshold_bps` (deposit refunded).
    RejectedThreshold,
    /// Vetoed because `NoWithVeto` reached `veto_threshold_bps` (deposit burned).
    Vetoed,
}

impl ProposalStatus {
    /// Static status label for diagnostics and vector fixtures.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Voting => "voting",
            Self::Timelocked { .. } => "timelocked",
            Self::Executed { .. } => "executed",
            Self::RejectedQuorum => "rejected_quorum",
            Self::RejectedThreshold => "rejected_threshold",
            Self::Vetoed => "vetoed",
        }
    }
}

/// On-chain governance proposal record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceProposal {
    /// Monotonically increasing proposal ID (`1, 2, ...`).
    pub id: u64,
    /// Proposer address.
    pub proposer: Address,
    /// Escrowed deposit in smallest units.
    pub deposit: u128,
    /// Block height at which the proposal was submitted.
    pub submit_height: u64,
    /// Block height at which voting closes (inclusive).
    pub voting_end_height: u64,
    /// Proposal payload.
    pub payload: ProposalPayload,
    /// Current lifecycle status.
    pub status: ProposalStatus,
    /// Recorded votes by validator address.
    pub votes: BTreeMap<Address, GovernanceVote>,
    /// Finalized tally result once voting ends.
    pub tally: Option<TallyResult>,
}

/// Active WebAssembly runtime state managed by on-chain governance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuntimeState {
    /// Active protocol version (`1` at genesis, incremented on hot upgrades).
    pub protocol_version: u32,
    /// `BLAKE3` digest of the active runtime WASM bytecode.
    pub wasm_code_hash: Hash,
    /// Active runtime WASM bytecode.
    pub wasm_bytecode: Vec<u8>,
    /// Block height at which this runtime version became active.
    pub activated_at_height: u64,
}

/// Emergency pause state controlled by a `2f+1` validator `QuorumCertificate`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EmergencyPauseState {
    /// Whether emergency pause is currently active.
    pub paused: bool,
    /// Human-readable reason for the pause, if active.
    pub reason: Option<String>,
    /// Block height at which the pause was triggered.
    pub paused_at_height: Option<u64>,
    /// View of the `QuorumCertificate` that authorized the pause.
    pub authorizing_qc_view: Option<u64>,
}

/// Deterministic on-chain governance engine (`B9`).
#[derive(Debug, Clone)]
pub struct GovernanceEngine {
    chain_id: ChainId,
    params: GovernanceParams,
    next_proposal_id: u64,
    proposals: BTreeMap<u64, GovernanceProposal>,
    validator_pubkeys: BTreeMap<Address, BlsPublicKey>,
    runtime: ActiveRuntimeState,
    emergency_pause: EmergencyPauseState,
    refunded_deposits: BTreeMap<Address, u128>,
    burned_deposits: u128,
}

impl GovernanceEngine {
    /// Create a new governance engine initialized with a genesis WASM runtime (`v1`).
    pub fn new(
        chain_id: ChainId,
        params: GovernanceParams,
        validator_pubkeys: BTreeMap<Address, BlsPublicKey>,
        genesis_wasm: Vec<u8>,
    ) -> Result<Self, GovernanceError> {
        params.validate()?;
        if genesis_wasm.len() < WASM_MAGIC_AND_VERSION.len()
            || genesis_wasm.len() > MAX_RUNTIME_WASM_BYTES
            || genesis_wasm[..8] != WASM_MAGIC_AND_VERSION
        {
            return Err(GovernanceError::InvalidWasmBytecode);
        }
        let wasm_code_hash = blake3_hash(&genesis_wasm);
        Ok(Self {
            chain_id,
            params,
            next_proposal_id: 1,
            proposals: BTreeMap::new(),
            validator_pubkeys,
            runtime: ActiveRuntimeState {
                protocol_version: 1,
                wasm_code_hash,
                wasm_bytecode: genesis_wasm,
                activated_at_height: 0,
            },
            emergency_pause: EmergencyPauseState::default(),
            refunded_deposits: BTreeMap::new(),
            burned_deposits: 0,
        })
    }

    /// Current governance parameters.
    pub const fn params(&self) -> &GovernanceParams {
        &self.params
    }

    /// Current active WASM runtime state.
    pub const fn runtime(&self) -> &ActiveRuntimeState {
        &self.runtime
    }

    /// Current emergency pause state.
    pub const fn emergency_pause(&self) -> &EmergencyPauseState {
        &self.emergency_pause
    }

    /// Total proposer deposits burned due to `NoWithVeto`.
    pub const fn burned_deposits(&self) -> u128 {
        self.burned_deposits
    }

    /// Total deposit amount refunded to `proposer`.
    pub fn refunded_deposit_of(&self, proposer: &Address) -> u128 {
        self.refunded_deposits.get(proposer).copied().unwrap_or(0)
    }

    /// Look up a proposal by ID.
    pub fn proposal(&self, id: u64) -> Option<&GovernanceProposal> {
        self.proposals.get(&id)
    }

    /// Submit a new governance proposal at `current_height`.
    pub fn submit_proposal(
        &mut self,
        proposer: Address,
        deposit: u128,
        current_height: u64,
        payload: ProposalPayload,
    ) -> Result<u64, GovernanceError> {
        if self.emergency_pause.paused {
            return Err(GovernanceError::EmergencyPaused {
                reason: self
                    .emergency_pause
                    .reason
                    .clone()
                    .unwrap_or_else(|| "paused".to_string()),
            });
        }
        if deposit < self.params.min_proposer_deposit {
            return Err(GovernanceError::InsufficientDeposit {
                provided: deposit,
                required: self.params.min_proposer_deposit,
            });
        }
        payload.validate(self.runtime.protocol_version)?;

        let id = self.next_proposal_id;
        self.next_proposal_id = self.next_proposal_id.saturating_add(1);
        let voting_end_height = current_height.saturating_add(self.params.voting_period_blocks);

        self.proposals.insert(
            id,
            GovernanceProposal {
                id,
                proposer,
                deposit,
                submit_height: current_height,
                voting_end_height,
                payload,
                status: ProposalStatus::Voting,
                votes: BTreeMap::new(),
                tally: None,
            },
        );
        Ok(id)
    }

    /// Cast a BLS-signed validator vote on `vote.proposal_id`.
    pub fn cast_vote(
        &mut self,
        vote: GovernanceVote,
        validator_set: &ValidatorSet,
    ) -> Result<(), GovernanceError> {
        if !validator_set.is_active(&vote.voter) {
            return Err(GovernanceError::UnknownOrInactiveValidator(vote.voter));
        }
        let Some(pk) = self.validator_pubkeys.get(&vote.voter) else {
            return Err(GovernanceError::UnknownOrInactiveValidator(vote.voter));
        };
        if !vote.verify(pk, &self.chain_id) {
            return Err(GovernanceError::InvalidVoteSignature(vote.voter));
        }

        let proposal = self
            .proposals
            .get_mut(&vote.proposal_id)
            .ok_or(GovernanceError::ProposalNotFound(vote.proposal_id))?;

        if proposal.status != ProposalStatus::Voting {
            return Err(GovernanceError::InvalidProposalStatus {
                proposal_id: proposal.id,
                status: proposal.status.as_str(),
            });
        }
        if vote.cast_height < proposal.submit_height
            || vote.cast_height > proposal.voting_end_height
        {
            return Err(GovernanceError::VotingWindowClosed {
                current_height: vote.cast_height,
                voting_end_height: proposal.voting_end_height,
            });
        }

        proposal.votes.insert(vote.voter, vote);
        Ok(())
    }

    /// Finalize voting on `proposal_id` at `current_height` (must be `> voting_end_height`)
    /// and transition passing proposals into the mandatory timelock queue.
    pub fn finalize_voting(
        &mut self,
        proposal_id: u64,
        current_height: u64,
        validator_set: &ValidatorSet,
    ) -> Result<ProposalStatus, GovernanceError> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or(GovernanceError::ProposalNotFound(proposal_id))?;

        if proposal.status != ProposalStatus::Voting {
            return Err(GovernanceError::InvalidProposalStatus {
                proposal_id,
                status: proposal.status.as_str(),
            });
        }
        if current_height <= proposal.voting_end_height {
            return Err(GovernanceError::VotingStillOpen {
                current_height,
                voting_end_height: proposal.voting_end_height,
            });
        }

        let total_active_stake = validator_set.total_stake() as u128;
        let mut yes_stake = 0u128;
        let mut no_stake = 0u128;
        let mut abstain_stake = 0u128;
        let mut no_with_veto_stake = 0u128;

        for (voter, vote) in &proposal.votes {
            let stake = validator_set
                .validator(voter)
                .map(|v| v.stake as u128)
                .unwrap_or(0);
            match vote.choice {
                GovernanceVoteChoice::Yes => yes_stake = yes_stake.saturating_add(stake),
                GovernanceVoteChoice::No => no_stake = no_stake.saturating_add(stake),
                GovernanceVoteChoice::Abstain => {
                    abstain_stake = abstain_stake.saturating_add(stake)
                }
                GovernanceVoteChoice::NoWithVeto => {
                    no_with_veto_stake = no_with_veto_stake.saturating_add(stake)
                }
            }
        }

        let tally = TallyResult {
            total_active_stake,
            yes_stake,
            no_stake,
            abstain_stake,
            no_with_veto_stake,
        };
        let participating = tally.total_participating_stake();
        let non_abstain = tally.non_abstain_stake();
        proposal.tally = Some(tally);

        // 1. Veto check: if NoWithVeto / participating >= veto_threshold_bps -> Vetoed (burn deposit).
        if participating > 0
            && no_with_veto_stake.saturating_mul(BPS_DENOMINATOR as u128)
                >= participating.saturating_mul(self.params.veto_threshold_bps as u128)
        {
            proposal.status = ProposalStatus::Vetoed;
            self.burned_deposits = self.burned_deposits.saturating_add(proposal.deposit);
            return Ok(proposal.status.clone());
        }

        // 2. Quorum check: participating / total_active_stake >= quorum_bps.
        if total_active_stake == 0
            || participating.saturating_mul(BPS_DENOMINATOR as u128)
                < total_active_stake.saturating_mul(self.params.quorum_bps as u128)
        {
            proposal.status = ProposalStatus::RejectedQuorum;
            *self.refunded_deposits.entry(proposal.proposer).or_insert(0) += proposal.deposit;
            return Ok(proposal.status.clone());
        }

        // 3. Supermajority pass check: yes / non_abstain >= pass_threshold_bps.
        if non_abstain == 0
            || yes_stake.saturating_mul(BPS_DENOMINATOR as u128)
                < non_abstain.saturating_mul(self.params.pass_threshold_bps as u128)
        {
            proposal.status = ProposalStatus::RejectedThreshold;
            *self.refunded_deposits.entry(proposal.proposer).or_insert(0) += proposal.deposit;
            return Ok(proposal.status.clone());
        }

        // Passed: enter mandatory timelock queue.
        let min_timelock_height = current_height.saturating_add(self.params.timelock_delay_blocks);
        let earliest_execution_height = match &proposal.payload {
            ProposalPayload::RuntimeWasmUpgrade {
                activation_height, ..
            } => min_timelock_height.max(*activation_height),
            ProposalPayload::ParameterChange { .. } => min_timelock_height,
        };
        proposal.status = ProposalStatus::Timelocked {
            earliest_execution_height,
        };
        Ok(proposal.status.clone())
    }

    /// Execute a `Timelocked` proposal once `current_height >= earliest_execution_height`.
    pub fn execute_proposal(
        &mut self,
        proposal_id: u64,
        current_height: u64,
    ) -> Result<ProposalStatus, GovernanceError> {
        if self.emergency_pause.paused {
            return Err(GovernanceError::EmergencyPaused {
                reason: self
                    .emergency_pause
                    .reason
                    .clone()
                    .unwrap_or_else(|| "paused".to_string()),
            });
        }

        let proposal = self
            .proposals
            .get(&proposal_id)
            .ok_or(GovernanceError::ProposalNotFound(proposal_id))?
            .clone();

        let ProposalStatus::Timelocked {
            earliest_execution_height,
        } = proposal.status
        else {
            return Err(GovernanceError::InvalidProposalStatus {
                proposal_id,
                status: proposal.status.as_str(),
            });
        };

        if current_height < earliest_execution_height {
            return Err(GovernanceError::TimelockNotExpired {
                current_height,
                earliest_execution_height,
            });
        }

        // Re-validate payload before execution (protects against conflicting upgrades).
        proposal.payload.validate(self.runtime.protocol_version)?;

        match &proposal.payload {
            ProposalPayload::RuntimeWasmUpgrade {
                to_version,
                wasm_bytecode,
                wasm_code_hash,
                ..
            } => {
                self.runtime = ActiveRuntimeState {
                    protocol_version: *to_version,
                    wasm_code_hash: *wasm_code_hash,
                    wasm_bytecode: wasm_bytecode.clone(),
                    activated_at_height: current_height,
                };
            }
            ProposalPayload::ParameterChange { updated_params } => {
                self.params = updated_params.clone();
            }
        }

        let new_status = ProposalStatus::Executed {
            executed_height: current_height,
        };
        if let Some(stored) = self.proposals.get_mut(&proposal_id) {
            stored.status = new_status.clone();
        }
        *self.refunded_deposits.entry(proposal.proposer).or_insert(0) += proposal.deposit;
        Ok(new_status)
    }

    /// Execute all `Timelocked` proposals whose `earliest_execution_height <= current_height`.
    pub fn execute_ready_proposals(
        &mut self,
        current_height: u64,
    ) -> Result<Vec<u64>, GovernanceError> {
        if self.emergency_pause.paused {
            return Ok(Vec::new());
        }
        let ready_ids: Vec<u64> = self
            .proposals
            .values()
            .filter_map(|p| match p.status {
                ProposalStatus::Timelocked {
                    earliest_execution_height,
                } if current_height >= earliest_execution_height => Some(p.id),
                _ => None,
            })
            .collect();

        let mut executed = Vec::with_capacity(ready_ids.len());
        for id in ready_ids {
            self.execute_proposal(id, current_height)?;
            executed.push(id);
        }
        Ok(executed)
    }

    /// Trigger an emergency pause authorized by a `2f+1` validator `QuorumCertificate`.
    pub fn trigger_emergency_pause(
        &mut self,
        current_height: u64,
        reason: String,
        qc: &QuorumCertificate,
    ) -> Result<(), GovernanceError> {
        self.verify_emergency_qc(qc)?;
        self.emergency_pause = EmergencyPauseState {
            paused: true,
            reason: Some(reason),
            paused_at_height: Some(current_height),
            authorizing_qc_view: Some(qc.view),
        };
        Ok(())
    }

    /// Lift an active emergency pause authorized by a `2f+1` validator `QuorumCertificate`.
    pub fn lift_emergency_pause(&mut self, qc: &QuorumCertificate) -> Result<(), GovernanceError> {
        self.verify_emergency_qc(qc)?;
        self.emergency_pause = EmergencyPauseState {
            paused: false,
            reason: None,
            paused_at_height: None,
            authorizing_qc_view: Some(qc.view),
        };
        Ok(())
    }

    fn verify_emergency_qc(&self, qc: &QuorumCertificate) -> Result<(), GovernanceError> {
        let required = quorum_threshold(self.validator_pubkeys.len() as u64) as usize;
        if qc.len() < required || qc.verify(self.chain_id.as_str(), Domain::Vote).is_err() {
            return Err(GovernanceError::InvalidEmergencyQc);
        }
        for pk in &qc.public_keys {
            if !self.validator_pubkeys.values().any(|known| known == pk) {
                return Err(GovernanceError::InvalidEmergencyQc);
            }
        }
        Ok(())
    }
}

/// Build a canonical valid WebAssembly module containing a deterministic version banner.
pub fn build_runtime_wasm_artifact(version: u32, tag: &str) -> (Vec<u8>, Hash) {
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&WASM_MAGIC_AND_VERSION);
    // Custom section (id = 0) encoding the runtime version and tag.
    let payload = format!("aura-runtime-v{version}:{tag}");
    bytes.push(0x00);
    bytes.push(payload.len() as u8);
    bytes.extend_from_slice(payload.as_bytes());
    let code_hash = blake3_hash(&bytes);
    (bytes, code_hash)
}

/// Single validator node in a hot-upgradable testnet (`B9` test gate).
pub struct HotUpgradableValidatorNode {
    /// Validator address.
    pub address: Address,
    /// BLS secret key.
    pub bls_sk: BlsSecretKey,
    /// BLS public key.
    pub bls_pk: BlsPublicKey,
    /// Local 3-chain finality engine.
    pub finality: FinalityEngine,
    /// Local validator set state.
    pub validator_set: ValidatorSet,
    /// Local on-chain governance engine and active WASM runtime.
    pub governance: GovernanceEngine,
    /// Number of manual node restarts (must remain `0` across hot upgrades).
    pub restart_count: u64,
}

/// 4-validator testnet harness proving that an on-chain WASM runtime upgrade
/// activates at `activation_height` without any manual validator restart.
pub struct HotUpgradableTestnet {
    /// Chain identifier.
    pub chain_id: ChainId,
    /// The 4 validator nodes.
    pub nodes: Vec<HotUpgradableValidatorNode>,
    /// Current committed height.
    pub height: u64,
    /// Last finalized block hash.
    pub tip_hash: Hash,
}

impl HotUpgradableTestnet {
    /// Initialize a 4-validator testnet on genesis runtime `v1`.
    pub fn new(
        chain_id: ChainId,
        params: GovernanceParams,
        genesis_wasm: Vec<u8>,
    ) -> Result<Self, GovernanceError> {
        let mut keys = Vec::with_capacity(4);
        let mut pubkeys = BTreeMap::new();
        let mut genesis_validators = Vec::with_capacity(4);

        for i in 0..4u8 {
            let mut seed = [0u8; 32];
            seed[0] = 0xB9;
            seed[1] = i + 1;
            let sk = BlsSecretKey::from_seed(&seed);
            let pk = sk.public_key();
            let addr = crate::bls::address_from_public_key(&pk);
            pubkeys.insert(addr, pk.clone());
            genesis_validators.push(Validator {
                address: addr,
                stake: 10_000_000,
                status: ValidatorStatus::Active,
                epoch: 0,
            });
            keys.push((addr, sk, pk));
        }

        let vset = ValidatorSet::new(genesis_validators, 100);
        let genesis_hash = Hash::from_bytes([0x00; 32]);
        let mut nodes = Vec::with_capacity(4);
        for (addr, sk, pk) in keys {
            let gov = GovernanceEngine::new(
                chain_id.clone(),
                params.clone(),
                pubkeys.clone(),
                genesis_wasm.clone(),
            )?;
            nodes.push(HotUpgradableValidatorNode {
                address: addr,
                bls_sk: sk,
                bls_pk: pk,
                finality: FinalityEngine::new(chain_id.as_str(), 1, genesis_hash),
                validator_set: vset.clone(),
                governance: gov,
                restart_count: 0,
            });
        }

        Ok(Self {
            chain_id,
            nodes,
            height: 0,
            tip_hash: genesis_hash,
        })
    }

    /// Submit a governance proposal replicated across all validators in the testnet.
    pub fn submit_proposal(
        &mut self,
        proposer: Address,
        deposit: u128,
        payload: ProposalPayload,
    ) -> Result<u64, GovernanceError> {
        let mut id = 0;
        for node in &mut self.nodes {
            id =
                node.governance
                    .submit_proposal(proposer, deposit, self.height, payload.clone())?;
        }
        Ok(id)
    }

    /// Have validator `validator_idx` sign and broadcast a governance vote to all nodes.
    pub fn cast_validator_vote(
        &mut self,
        validator_idx: usize,
        proposal_id: u64,
        choice: GovernanceVoteChoice,
    ) -> Result<(), GovernanceError> {
        let voter = self.nodes[validator_idx].address;
        let vote = GovernanceVote::sign(
            &self.nodes[validator_idx].bls_sk,
            voter,
            &self.chain_id,
            proposal_id,
            choice,
            self.height,
        );
        for node in &mut self.nodes {
            let vset = node.validator_set.clone();
            node.governance.cast_vote(vote.clone(), &vset)?;
        }
        Ok(())
    }

    /// Finalize voting on `proposal_id` across all validator nodes.
    pub fn finalize_voting(&mut self, proposal_id: u64) -> Result<ProposalStatus, GovernanceError> {
        let mut status = ProposalStatus::Voting;
        for node in &mut self.nodes {
            let vset = node.validator_set.clone();
            status = node
                .governance
                .finalize_voting(proposal_id, self.height, &vset)?;
        }
        Ok(status)
    }

    /// Produce a block with a `2f+1` QC across all 4 validators, advance `height`,
    /// and automatically execute any timelocked governance upgrades scheduled for
    /// `<= height` without restarting any validator.
    pub fn produce_block(&mut self) -> Result<u64, GovernanceError> {
        self.height = self.height.saturating_add(1);
        let proto_ver = self.nodes[0].governance.runtime().protocol_version;
        let timestamp_ms = 1_700_000_000_000 + self.height * 1_000;
        let txs = vec![format!("h={}:v={proto_ver}", self.height).into_bytes()];

        // Node 0 proposes at the deterministic timestamp; all other nodes replay the same proposal.
        let proposed = self.nodes[0].finality.propose_at(timestamp_ms, txs.clone());
        let block_hash = proposed.block_hash;
        for node in self.nodes.iter_mut().skip(1) {
            let _ = node.finality.propose_at(timestamp_ms, txs.clone());
        }

        let active: std::collections::BTreeSet<Address> =
            self.nodes.iter().map(|n| n.address).collect();
        let votes: Vec<(BlsPublicKey, crate::qc::Vote)> = self
            .nodes
            .iter()
            .map(|n| {
                let v = n
                    .finality
                    .vote_for(&n.bls_sk, n.address, &proposed, VoteType::Precommit);
                (n.bls_pk.clone(), v)
            })
            .collect();

        for node in &mut self.nodes {
            let mut agg = QcAggregator::new(node.governance.chain_id.as_str(), active.clone());
            for (pk, vote) in &votes {
                let (qc_opt, _) =
                    node.finality
                        .aggregate_and_commit(&mut agg, pk.clone(), vote.clone());
                if qc_opt.is_some() {
                    break;
                }
            }
            node.finality.change_round();
            node.validator_set.advance_block();
            node.governance.execute_ready_proposals(self.height)?;
        }
        self.tip_hash = block_hash;
        Ok(self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_chain_id() -> ChainId {
        ChainId::new("aura-b9-test").expect("valid chain id")
    }

    fn setup_fixture() -> (
        ChainId,
        ValidatorSet,
        Vec<(Address, BlsSecretKey, BlsPublicKey)>,
        GovernanceEngine,
    ) {
        let chain_id = test_chain_id();
        let mut keys = Vec::new();
        let mut pubkeys = BTreeMap::new();
        let mut genesis_validators = Vec::new();
        for i in 0..4u8 {
            let mut seed = [0u8; 32];
            seed[0] = 0x90;
            seed[1] = i + 1;
            let sk = BlsSecretKey::from_seed(&seed);
            let pk = sk.public_key();
            let addr = crate::bls::address_from_public_key(&pk);
            pubkeys.insert(addr, pk.clone());
            genesis_validators.push(Validator {
                address: addr,
                stake: 10_000_000,
                status: ValidatorStatus::Active,
                epoch: 0,
            });
            keys.push((addr, sk, pk));
        }
        let vset = ValidatorSet::new(genesis_validators, 100);
        let (genesis_wasm, _) = build_runtime_wasm_artifact(1, "genesis");
        let gov = GovernanceEngine::new(
            chain_id.clone(),
            GovernanceParams::default(),
            pubkeys,
            genesis_wasm,
        )
        .expect("valid genesis governance engine");
        (chain_id, vset, keys, gov)
    }

    fn make_vote_qc(
        chain_id: &ChainId,
        keys: &[(Address, BlsSecretKey, BlsPublicKey)],
        signer_count: usize,
        view: u64,
        round: u64,
        block_hash: Hash,
    ) -> QuorumCertificate {
        let engine = FinalityEngine::new(chain_id.as_str(), view, block_hash);
        let dummy_block = crate::finality::FinalityBlock {
            view,
            round,
            parent_hash: block_hash,
            timestamp_ms: 0,
            block_hash,
            qc: None,
            transactions: Vec::new(),
        };
        let mut public_keys = Vec::new();
        let mut signatures = Vec::new();
        for (addr, sk, pk) in keys.iter().take(signer_count) {
            let vote = engine.vote_for(sk, *addr, &dummy_block, VoteType::Precommit);
            public_keys.push(pk.clone());
            signatures.push(vote.signature);
        }
        QuorumCertificate {
            view,
            round,
            block_hash,
            vote_type: VoteType::Precommit,
            public_keys,
            signatures,
        }
    }

    #[test]
    fn proposal_vote_timelock_and_wasm_runtime_upgrade_lifecycle() {
        let (chain_id, vset, keys, mut gov) = setup_fixture();
        assert_eq!(gov.runtime().protocol_version, 1);

        let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, "upgrade-v2");
        let prop_id = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                5,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2.clone(),
                    wasm_code_hash: hash_v2,
                    activation_height: 20,
                },
            )
            .expect("submit proposal");

        // 3 of 4 validators (75% stake) vote Yes at height 10.
        for (addr, sk, _) in keys.iter().take(3) {
            let vote =
                GovernanceVote::sign(sk, *addr, &chain_id, prop_id, GovernanceVoteChoice::Yes, 10);
            gov.cast_vote(vote, &vset).expect("valid vote");
        }

        // Finalize voting at height 16 (> voting_end_height = 15).
        let status = gov
            .finalize_voting(prop_id, 16, &vset)
            .expect("finalize voting");
        assert_eq!(
            status,
            ProposalStatus::Timelocked {
                earliest_execution_height: 21
            }
        );

        // Execute after timelock at height 21.
        let exec_status = gov
            .execute_proposal(prop_id, 21)
            .expect("execute after timelock");
        assert_eq!(
            exec_status,
            ProposalStatus::Executed {
                executed_height: 21
            }
        );
        assert_eq!(gov.runtime().protocol_version, 2);
        assert_eq!(gov.runtime().wasm_code_hash, hash_v2);
        assert_eq!(gov.runtime().wasm_bytecode, wasm_v2);
        assert_eq!(
            gov.refunded_deposit_of(&keys[0].0),
            DEFAULT_MIN_PROPOSER_DEPOSIT
        );
    }

    #[test]
    fn timelock_rejects_early_execution_before_delay_elapses() {
        let (chain_id, vset, keys, mut gov) = setup_fixture();
        let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, "timelock-check");
        let prop_id = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                1,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2,
                    wasm_code_hash: hash_v2,
                    activation_height: 15,
                },
            )
            .expect("submit proposal");

        for (addr, sk, _) in &keys {
            let vote =
                GovernanceVote::sign(sk, *addr, &chain_id, prop_id, GovernanceVoteChoice::Yes, 5);
            gov.cast_vote(vote, &vset).expect("vote");
        }

        // Finalize at height 12 -> earliest_execution_height = 12 + 5 = 17.
        let status = gov.finalize_voting(prop_id, 12, &vset).expect("tally");
        assert_eq!(
            status,
            ProposalStatus::Timelocked {
                earliest_execution_height: 17
            }
        );

        // Attempting execution at height 16 (< 17) must fail with TimelockNotExpired.
        let err = gov
            .execute_proposal(prop_id, 16)
            .expect_err("early execution must fail");
        assert_eq!(
            err,
            GovernanceError::TimelockNotExpired {
                current_height: 16,
                earliest_execution_height: 17,
            }
        );
        assert_eq!(gov.runtime().protocol_version, 1);
    }

    #[test]
    fn quorum_and_supermajority_thresholds_reject_under_voted_or_vetoed_proposals() {
        let (chain_id, vset, keys, mut gov) = setup_fixture();
        let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, "quorum-check");

        // Proposal 1: only 1 of 4 validators (25% < 33.34% quorum) votes Yes.
        let p1 = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                1,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2.clone(),
                    wasm_code_hash: hash_v2,
                    activation_height: 20,
                },
            )
            .expect("p1");
        let vote = GovernanceVote::sign(
            &keys[0].1,
            keys[0].0,
            &chain_id,
            p1,
            GovernanceVoteChoice::Yes,
            3,
        );
        gov.cast_vote(vote, &vset).expect("vote");
        let s1 = gov.finalize_voting(p1, 12, &vset).expect("tally p1");
        assert_eq!(s1, ProposalStatus::RejectedQuorum);

        // Proposal 2: 2 Yes + 2 No (50% Yes < 66.67% supermajority) -> RejectedThreshold.
        let p2 = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                12,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2.clone(),
                    wasm_code_hash: hash_v2,
                    activation_height: 30,
                },
            )
            .expect("p2");
        for (i, (addr, sk, _)) in keys.iter().enumerate() {
            let choice = if i < 2 {
                GovernanceVoteChoice::Yes
            } else {
                GovernanceVoteChoice::No
            };
            let v = GovernanceVote::sign(sk, *addr, &chain_id, p2, choice, 15);
            gov.cast_vote(v, &vset).expect("vote p2");
        }
        let s2 = gov.finalize_voting(p2, 23, &vset).expect("tally p2");
        assert_eq!(s2, ProposalStatus::RejectedThreshold);

        // Proposal 3: 2 Yes + 2 NoWithVeto (50% veto >= 33.34% veto threshold) -> Vetoed & deposit burned.
        let p3 = gov
            .submit_proposal(
                keys[1].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                25,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2,
                    wasm_code_hash: hash_v2,
                    activation_height: 45,
                },
            )
            .expect("p3");
        for (i, (addr, sk, _)) in keys.iter().enumerate() {
            let choice = if i < 2 {
                GovernanceVoteChoice::Yes
            } else {
                GovernanceVoteChoice::NoWithVeto
            };
            let v = GovernanceVote::sign(sk, *addr, &chain_id, p3, choice, 28);
            gov.cast_vote(v, &vset).expect("vote p3");
        }
        let s3 = gov.finalize_voting(p3, 36, &vset).expect("tally p3");
        assert_eq!(s3, ProposalStatus::Vetoed);
        assert_eq!(gov.burned_deposits(), DEFAULT_MIN_PROPOSER_DEPOSIT);
    }

    #[test]
    fn wasm_upgrade_rejects_invalid_magic_or_tampered_code_hash_or_version_downgrade() {
        let (_chain_id, _vset, keys, mut gov) = setup_fixture();
        let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, "valid");

        // 1. Version downgrade / non-increment (1 -> 1) must be rejected.
        let err_ver = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                1,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 1,
                    wasm_bytecode: wasm_v2.clone(),
                    wasm_code_hash: hash_v2,
                    activation_height: 20,
                },
            )
            .expect_err("same version must fail");
        assert_eq!(
            err_ver,
            GovernanceError::InvalidUpgradeVersion {
                from_version: 1,
                to_version: 1
            }
        );

        // 2. Bad WASM magic bytes must be rejected.
        let bad_wasm = b"not-a-wasm-binary".to_vec();
        let bad_hash = blake3_hash(&bad_wasm);
        let err_magic = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                1,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: bad_wasm,
                    wasm_code_hash: bad_hash,
                    activation_height: 20,
                },
            )
            .expect_err("bad magic must fail");
        assert_eq!(err_magic, GovernanceError::InvalidWasmBytecode);

        // 3. Mismatched code hash must be rejected.
        let wrong_hash = Hash::from_bytes([0xFF; 32]);
        let err_hash = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                1,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2,
                    wasm_code_hash: wrong_hash,
                    activation_height: 20,
                },
            )
            .expect_err("tampered code hash must fail");
        assert_eq!(
            err_hash,
            GovernanceError::WasmCodeHashMismatch {
                declared: wrong_hash,
                computed: hash_v2,
            }
        );
    }

    #[test]
    fn emergency_pause_requires_quorum_qc_and_halts_governance_execution_until_resumed() {
        let (chain_id, vset, keys, mut gov) = setup_fixture();
        let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, "pause-test");

        let prop_id = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                1,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2,
                    wasm_code_hash: hash_v2,
                    activation_height: 15,
                },
            )
            .expect("submit");
        for (addr, sk, _) in &keys {
            let v =
                GovernanceVote::sign(sk, *addr, &chain_id, prop_id, GovernanceVoteChoice::Yes, 4);
            gov.cast_vote(v, &vset).expect("vote");
        }
        gov.finalize_voting(prop_id, 12, &vset).expect("tally");

        // Sub-quorum QC (2 of 4 signers) must be rejected.
        let pause_hash = blake3_hash(b"emergency-pause-v1");
        let sub_qc = make_vote_qc(&chain_id, &keys, 2, 5, 1, pause_hash);
        assert_eq!(
            gov.trigger_emergency_pause(15, "sub-quorum".to_string(), &sub_qc),
            Err(GovernanceError::InvalidEmergencyQc)
        );

        // Valid 3-of-4 QC triggers emergency pause.
        let valid_qc = make_vote_qc(&chain_id, &keys, 3, 5, 1, pause_hash);
        gov.trigger_emergency_pause(15, "critical-bug-mitigation".to_string(), &valid_qc)
            .expect("emergency pause activated");
        assert!(gov.emergency_pause().paused);

        // While paused, executing a ready proposal at height 20 must fail with EmergencyPaused.
        let err = gov
            .execute_proposal(prop_id, 20)
            .expect_err("execution must fail while paused");
        assert_eq!(
            err,
            GovernanceError::EmergencyPaused {
                reason: "critical-bug-mitigation".to_string()
            }
        );

        // Lifting the emergency pause with a valid 2f+1 QC allows execution to proceed.
        gov.lift_emergency_pause(&valid_qc)
            .expect("lift emergency pause");
        assert!(!gov.emergency_pause().paused);
        let status = gov
            .execute_proposal(prop_id, 20)
            .expect("execute after resume");
        assert_eq!(
            status,
            ProposalStatus::Executed {
                executed_height: 20
            }
        );
        assert_eq!(gov.runtime().protocol_version, 2);
    }

    #[test]
    fn testnet_protocol_upgrade_activates_without_manual_validator_restart() {
        let chain_id = test_chain_id();
        let (genesis_wasm, genesis_code_hash) = build_runtime_wasm_artifact(1, "genesis-v1");
        let mut testnet =
            HotUpgradableTestnet::new(chain_id, GovernanceParams::default(), genesis_wasm)
                .expect("init testnet");

        // Produce blocks 1..=5 on protocol v1.
        for _ in 0..5 {
            testnet.produce_block().expect("produce v1 block");
        }
        for node in &testnet.nodes {
            assert_eq!(node.governance.runtime().protocol_version, 1);
            assert_eq!(node.governance.runtime().wasm_code_hash, genesis_code_hash);
            assert_eq!(node.restart_count, 0);
        }

        // Submit on-chain WASM runtime upgrade v1 -> v2 at height 5, scheduled for activation at height 25.
        let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, "hot-upgrade-v2");
        let proposer = testnet.nodes[0].address;
        let prop_id = testnet
            .submit_proposal(
                proposer,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2,
                    wasm_code_hash: hash_v2,
                    activation_height: 25,
                },
            )
            .expect("submit upgrade proposal");

        // All 4 validators vote Yes during the voting window (heights 6..=15).
        for _ in 0..3 {
            testnet
                .produce_block()
                .expect("produce block during voting");
        }
        for idx in 0..4 {
            testnet
                .cast_validator_vote(idx, prop_id, GovernanceVoteChoice::Yes)
                .expect("cast validator vote");
        }

        // Advance past voting_end_height (5 + 10 = 15) to height 16 and finalize voting.
        while testnet.height <= 15 {
            testnet.produce_block().expect("advance to voting end");
        }
        let status = testnet.finalize_voting(prop_id).expect("finalize voting");
        assert_eq!(
            status,
            ProposalStatus::Timelocked {
                earliest_execution_height: 25
            }
        );

        // Advance through blocks 17..=24 (still on v1 before activation_height = 25).
        while testnet.height < 24 {
            testnet.produce_block().expect("advance during timelock");
        }
        for node in &testnet.nodes {
            assert_eq!(node.governance.runtime().protocol_version, 1);
        }

        // Block 25 triggers automatic hot runtime upgrade to v2 across all 4 validators,
        // and blocks 25..=35 continue finalizing without any validator restart!
        while testnet.height < 35 {
            testnet.produce_block().expect("produce post-upgrade block");
        }
        for node in &testnet.nodes {
            assert_eq!(node.governance.runtime().protocol_version, 2);
            assert_eq!(node.governance.runtime().wasm_code_hash, hash_v2);
            assert_eq!(node.governance.runtime().activated_at_height, 25);
            assert_eq!(
                node.restart_count, 0,
                "no manual validator restart occurred"
            );
        }
    }
}
