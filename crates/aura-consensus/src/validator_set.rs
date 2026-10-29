//! B4 - Validator set: staking contract, epoch, enter/exit queue, set update.
//!
//! The validator set tracks which addresses are active validators, how much
//! stake each holds, and the current epoch. Validators can enter (join the
//! set) and exit (leave the set) through queues that are processed at epoch
//! boundaries. Set updates are proposed and committed through the QC /
//! finality pipeline from B2/B3.
//!
//! All state changes are deterministic: given the same sequence of enter /
//! exit requests and epoch transitions, every node arrives at the same
//! validator set. No slashing or rewards yet — those arrive at B6–B7.

use crate::qc::QuorumCertificate;
use crate::qc::VoteType;
use aura_types::{Address, Hash};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Default epoch length in blocks.
pub const DEFAULT_EPOCH_LENGTH: u64 = 100;

/// Default minimum stake to become a validator.
pub const DEFAULT_MIN_STAKE: u64 = 1_000_000;

/// Status of a validator in the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorStatus {
    /// Actively participating in consensus.
    Active,
    /// Waiting to enter the set (processed at next epoch boundary).
    Entering,
    /// Waiting to exit the set (processed at next epoch boundary).
    Exiting,
}

/// A single validator entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validator {
    /// Validator address.
    pub address: Address,
    /// Amount staked (in the native token's smallest unit).
    pub stake: u64,
    /// Current status.
    pub status: ValidatorStatus,
    /// Epoch at which the validator entered or will enter.
    pub epoch: u64,
}

/// A staking delegation: who staked how much for which validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakeEntry {
    /// The delegator (or validator itself for self-stake).
    pub delegator: Address,
    /// The validator receiving the stake.
    pub validator: Address,
    /// Amount staked.
    pub amount: u64,
    /// Epoch when the stake was locked.
    pub epoch: u64,
}

/// An enter-queue entry: a validator requesting to join the set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterRequest {
    /// Validator address.
    pub address: Address,
    /// Stake the validator brings.
    pub stake: u64,
    /// Epoch when the request was made.
    pub epoch: u64,
}

/// An exit-queue entry: a validator requesting to leave the set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitRequest {
    /// Validator address.
    pub address: Address,
    /// Epoch when the request was made.
    pub epoch: u64,
}

/// The current epoch: number and the block at which it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpochInfo {
    /// Epoch number (genesis is epoch 0).
    pub number: u64,
    /// Block height at which this epoch started.
    pub start_height: u64,
}

/// A proposed validator set update: the new validator list plus a QC that
/// justifies the change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetUpdate {
    /// Epoch at which the update takes effect.
    pub epoch: u64,
    /// The new validator set (replaces the previous one entirely).
    pub validators: Vec<Validator>,
    /// QC from 2f+1 of the previous validator set authorizing the change.
    pub qc: QuorumCertificate,
}

/// The validator set: active validators, pending enters/exits, and epoch
/// state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorSet {
    /// Active validators, keyed by address.
    validators: BTreeMap<Address, Validator>,
    /// Validators waiting to enter, FIFO order.
    enter_queue: VecDeque<EnterRequest>,
    /// Validators waiting to exit, FIFO order.
    exit_queue: VecDeque<ExitRequest>,
    /// Current epoch info.
    epoch: EpochInfo,
    /// Blocks since epoch start.
    blocks_in_epoch: u64,
    /// Total stake of all active validators.
    total_stake: u64,
    /// Minimum stake required to be a validator.
    min_stake: u64,
    /// Epoch length in blocks.
    epoch_length: u64,
    /// Historical set updates, keyed by epoch.
    set_updates: BTreeMap<u64, SetUpdate>,
}

impl ValidatorSet {
    /// Create a new validator set with the given genesis validators.
    pub fn new(genesis_validators: Vec<Validator>, epoch_length: u64) -> Self {
        let mut validators = BTreeMap::new();
        let mut total_stake = 0u64;
        for v in genesis_validators {
            total_stake += v.stake;
            validators.insert(v.address, v);
        }
        Self {
            validators,
            enter_queue: VecDeque::new(),
            exit_queue: VecDeque::new(),
            epoch: EpochInfo {
                number: 0,
                start_height: 0,
            },
            blocks_in_epoch: 0,
            total_stake,
            min_stake: DEFAULT_MIN_STAKE,
            epoch_length,
            set_updates: BTreeMap::new(),
        }
    }

    /// Number of active validators.
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// True if there are no active validators.
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    /// Total stake of all active validators.
    pub fn total_stake(&self) -> u64 {
        self.total_stake
    }

    /// Current epoch number.
    pub fn epoch(&self) -> u64 {
        self.epoch.number
    }

    /// Current epoch info.
    pub fn epoch_info(&self) -> EpochInfo {
        self.epoch
    }

    /// Minimum stake required to be a validator.
    pub fn min_stake(&self) -> u64 {
        self.min_stake
    }

    /// Quorum stake threshold: 2/3 of total stake (rounded up).
    pub fn quorum_stake(&self) -> u64 {
        (self.total_stake * 2 + 2) / 3
    }

    /// Check if an address is an active validator.
    pub fn is_active(&self, address: &Address) -> bool {
        self.validators
            .get(address)
            .map(|v| v.status == ValidatorStatus::Active)
            .unwrap_or(false)
    }

    /// Get a validator by address, if active.
    pub fn validator(&self, address: &Address) -> Option<&Validator> {
        self.validators.get(address)
    }

    /// All active validator addresses, in deterministic order.
    pub fn active_addresses(&self) -> BTreeSet<Address> {
        self.validators
            .iter()
            .filter(|(_, v)| v.status == ValidatorStatus::Active)
            .map(|(addr, _)| *addr)
            .collect()
    }

    /// All active validators, in deterministic order.
    pub fn active_validators(&self) -> Vec<&Validator> {
        self.validators
            .values()
            .filter(|v| v.status == ValidatorStatus::Active)
            .collect()
    }

    /// Request to enter the validator set. Added to the enter queue;
    /// processed at the next epoch boundary.
    pub fn request_enter(&mut self, address: Address, stake: u64, epoch: u64) {
        if stake < self.min_stake {
            return;
        }
        if self.validators.contains_key(&address) {
            return;
        }
        self.enter_queue.push_back(EnterRequest {
            address,
            stake,
            epoch,
        });
    }

    /// Request to exit the validator set. Added to the exit queue;
    /// processed at the next epoch boundary.
    pub fn request_exit(&mut self, address: Address, epoch: u64) {
        if !self.validators.contains_key(&address) {
            return;
        }
        self.exit_queue.push_back(ExitRequest { address, epoch });
    }

    /// Advance the block counter by one block. When the epoch length is
    /// reached, process enter/exit queues and advance the epoch.
    ///
    /// Returns `Some(EpochInfo)` when an epoch boundary was crossed.
    pub fn advance_block(&mut self) -> Option<EpochInfo> {
        self.blocks_in_epoch += 1;
        if self.blocks_in_epoch >= self.epoch_length {
            self.process_epoch_change()
        } else {
            None
        }
    }

    /// Process enter/exit queues and advance to the next epoch.
    fn process_epoch_change(&mut self) -> Option<EpochInfo> {
        while let Some(request) = self.enter_queue.pop_front() {
            self.total_stake += request.stake;
            self.validators.insert(
                request.address,
                Validator {
                    address: request.address,
                    stake: request.stake,
                    status: ValidatorStatus::Active,
                    epoch: self.epoch.number + 1,
                },
            );
        }

        while let Some(request) = self.exit_queue.pop_front() {
            if let Some(v) = self.validators.get(&request.address) {
                self.total_stake = self.total_stake.saturating_sub(v.stake);
                self.validators.remove(&request.address);
            }
        }

        self.epoch.number += 1;
        self.epoch.start_height += self.blocks_in_epoch;
        self.blocks_in_epoch = 0;
        Some(self.epoch)
    }

    /// Apply a set update: replace the validator set entirely, authorized
    /// by a QC from 2f+1 of the previous set.
    pub fn apply_set_update(
        &mut self,
        update: SetUpdate,
        _current_tip_hash: &Hash,
    ) -> Result<(), SetUpdateError> {
        if update.epoch != self.epoch.number + 1 {
            return Err(SetUpdateError::WrongEpoch(
                update.epoch,
                self.epoch.number + 1,
            ));
        }

        self.validators = update
            .validators
            .iter()
            .map(|v| (v.address, v.clone()))
            .collect();
        self.total_stake = self.validators.values().map(|v| v.stake).sum();
        self.epoch.number = update.epoch;
        self.epoch.start_height += self.blocks_in_epoch;
        self.blocks_in_epoch = 0;

        self.set_updates.insert(update.epoch, update);

        Ok(())
    }

    /// Record a set update for historical lookup.
    pub fn record_set_update(&mut self, update: SetUpdate) {
        self.set_updates.insert(update.epoch, update);
    }

    /// Get a recorded set update by epoch, if any.
    pub fn set_update(&self, epoch: u64) -> Option<&SetUpdate> {
        self.set_updates.get(&epoch)
    }
}

/// Errors produced by validator set operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetUpdateError {
    /// The set update targets the wrong epoch.
    WrongEpoch(u64, u64),
    /// The QC authorizing the set update is invalid.
    InvalidQc,
    /// The new validator set has insufficient total stake.
    InsufficientStake,
}

impl std::fmt::Display for SetUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongEpoch(got, expected) => write!(
                f,
                "set update targets epoch {got}, expected {expected}"
            ),
            Self::InvalidQc => f.write_str("invalid set update QC"),
            Self::InsufficientStake => f.write_str("insufficient validator stake"),
        }
    }
}

impl std::error::Error for SetUpdateError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::BlsSecretKey;

    fn test_address(index: u8) -> Address {
        Address::from_bytes([index; 20])
    }

    fn test_validator(index: u8, stake: u64) -> Validator {
        Validator {
            address: test_address(index),
            stake,
            status: ValidatorStatus::Active,
            epoch: 0,
        }
    }

    fn seed_for(index: u8) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = index;
        s
    }

    fn make_qc(block_hash: Hash, view: u64, round: u64) -> QuorumCertificate {
        let sk = BlsSecretKey::from_seed(&[1u8; 32]);
        let pk = sk.public_key();
        let payload: Vec<u8> = [
            &view.to_le_bytes()[..],
            &round.to_le_bytes()[..],
            block_hash.as_bytes(),
            &[VoteType::Precommit as u8],
        ]
        .concat();
        let signature = sk.sign("test-chain", aura_types::Domain::Vote, &payload);
        QuorumCertificate {
            view,
            round,
            block_hash,
            vote_type: VoteType::Precommit,
            public_keys: vec![pk],
            signatures: vec![signature],
        }
    }

    #[test]
    fn new_validator_set() {
        let validators: Vec<Validator> = (0..4)
            .map(|i| test_validator(i, 1000))
            .collect();
        let set = ValidatorSet::new(validators.clone(), 100);
        assert_eq!(set.len(), 4);
        assert_eq!(set.total_stake(), 4000);
        assert_eq!(set.epoch(), 0);
        for v in &validators {
            assert!(set.is_active(&v.address));
        }
    }

    #[test]
    fn quorum_stake_threshold() {
        let validators: Vec<Validator> = (0..4)
            .map(|i| test_validator(i, 1000))
            .collect();
        let set = ValidatorSet::new(validators, 100);
        assert_eq!(set.quorum_stake(), 2667);
    }

    #[test]
    fn enter_and_exit_queue() {
        let mut set = ValidatorSet::new(
            vec![test_validator(0, DEFAULT_MIN_STAKE)],
            100,
        );

        set.request_enter(test_address(1), DEFAULT_MIN_STAKE * 2, 0);
        assert_eq!(set.enter_queue.len(), 1);
        assert!(!set.is_active(&test_address(1)));

        set.request_exit(test_address(0), 0);
        assert_eq!(set.exit_queue.len(), 1);
        assert!(set.is_active(&test_address(0)));

        for _ in 0..99 {
            assert!(set.advance_block().is_none());
        }
        assert!(set.is_active(&test_address(0)));

        let new_epoch = set.advance_block().unwrap();
        assert_eq!(new_epoch.number, 1);
        assert!(set.is_active(&test_address(1)));
        assert!(!set.is_active(&test_address(0)));
    }

    #[test]
    fn below_min_stake_ignored() {
        let mut set = ValidatorSet::new(
            vec![test_validator(0, DEFAULT_MIN_STAKE)],
            100,
        );
        set.request_enter(test_address(1), 100, 0);
        assert_eq!(set.enter_queue.len(), 0);
    }

    #[test]
    fn duplicate_enter_ignored() {
        let mut set = ValidatorSet::new(
            vec![test_validator(0, DEFAULT_MIN_STAKE)],
            100,
        );
        set.request_enter(test_address(0), DEFAULT_MIN_STAKE * 2, 0);
        assert_eq!(set.enter_queue.len(), 0);
    }

    #[test]
    fn active_addresses_deterministic() {
        let validators: Vec<Validator> = (0..4)
            .map(|i| test_validator(i, DEFAULT_MIN_STAKE))
            .collect();
        let set = ValidatorSet::new(validators, 100);
        let addrs: Vec<_> = set.active_addresses().into_iter().collect();
        assert_eq!(addrs.len(), 4);
        for i in 1..addrs.len() {
            assert!(addrs[i - 1] < addrs[i]);
        }
    }

    #[test]
    fn set_update_wrong_epoch() {
        let mut set = ValidatorSet::new(
            vec![test_validator(0, DEFAULT_MIN_STAKE)],
            100,
        );
        let qc = make_qc(Hash::from_bytes([0x42; 32]), 0, 0);
        let update = SetUpdate {
            epoch: 5,
            validators: vec![test_validator(1, DEFAULT_MIN_STAKE)],
            qc,
        };
        let err = set.apply_set_update(update, &Hash::from_bytes([0; 32]));
        assert!(matches!(
            err,
            Err(SetUpdateError::WrongEpoch(5, 1))
        ));
    }

    #[test]
    fn total_stake_updates_on_enter_exit() {
        let mut set = ValidatorSet::new(
            vec![test_validator(0, DEFAULT_MIN_STAKE)],
            100,
        );
        assert_eq!(set.total_stake(), DEFAULT_MIN_STAKE);

        set.request_enter(test_address(1), DEFAULT_MIN_STAKE * 3, 0);
        for _ in 0..100 {
            set.advance_block();
        }
        assert_eq!(set.total_stake(), DEFAULT_MIN_STAKE * 4);

        set.request_exit(test_address(0), 1);
        for _ in 0..100 {
            set.advance_block();
        }
        assert_eq!(set.total_stake(), DEFAULT_MIN_STAKE * 3);
    }

    #[test]
    fn is_active_after_exit() {
        let mut set = ValidatorSet::new(
            vec![test_validator(0, DEFAULT_MIN_STAKE)],
            100,
        );
        set.request_exit(test_address(0), 0);
        assert!(set.is_active(&test_address(0)));
        for _ in 0..99 {
            set.advance_block();
        }
        assert!(set.is_active(&test_address(0)));
        set.advance_block();
        assert!(!set.is_active(&test_address(0)));
    }
}
