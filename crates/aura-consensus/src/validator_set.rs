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

use crate::qc::{QuorumCertificate, quorum_threshold};
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

/// Staking ledger managing validator self-stakes, delegations, and epoch-boundary
/// [`ValidatorSet`] transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StakingContract {
    /// Underlying active validator set and enter/exit queues.
    validator_set: ValidatorSet,
    /// Delegations indexed by `(delegator, validator)`.
    delegations: BTreeMap<(Address, Address), StakeEntry>,
}

impl StakingContract {
    /// Initialize a [`StakingContract`] from the genesis validator set and epoch length.
    pub fn new(genesis_validators: Vec<Validator>, epoch_length: u64) -> Self {
        let mut delegations = BTreeMap::new();
        for v in &genesis_validators {
            delegations.insert(
                (v.address, v.address),
                StakeEntry {
                    delegator: v.address,
                    validator: v.address,
                    amount: v.stake,
                    epoch: 0,
                },
            );
        }
        Self {
            validator_set: ValidatorSet::new(genesis_validators, epoch_length),
            delegations,
        }
    }

    /// Borrow the underlying [`ValidatorSet`].
    pub const fn validator_set(&self) -> &ValidatorSet {
        &self.validator_set
    }

    /// Mutably borrow the underlying [`ValidatorSet`].
    pub fn validator_set_mut(&mut self) -> &mut ValidatorSet {
        &mut self.validator_set
    }

    /// Stake `amount` to register `validator` in the enter queue for the next epoch.
    pub fn stake_and_enter(
        &mut self,
        validator: Address,
        amount: u64,
    ) -> Result<(), SetUpdateError> {
        if amount < self.validator_set.min_stake() {
            return Err(SetUpdateError::InsufficientStake);
        }
        let epoch = self.validator_set.epoch();
        self.delegations.insert(
            (validator, validator),
            StakeEntry {
                delegator: validator,
                validator,
                amount,
                epoch,
            },
        );
        self.validator_set.request_enter(validator, amount, epoch);
        Ok(())
    }

    /// Delegate `amount` from `delegator` to an existing or entering `validator`.
    pub fn delegate(&mut self, delegator: Address, validator: Address, amount: u64) {
        let epoch = self.validator_set.epoch();
        let entry = self
            .delegations
            .entry((delegator, validator))
            .or_insert(StakeEntry {
                delegator,
                validator,
                amount: 0,
                epoch,
            });
        entry.amount = entry.amount.saturating_add(amount);
    }

    /// Total delegated stake recorded for `(delegator, validator)`.
    pub fn delegated_amount(&self, delegator: &Address, validator: &Address) -> u64 {
        self.delegations
            .get(&(*delegator, *validator))
            .map(|e| e.amount)
            .unwrap_or(0)
    }
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
        let thirds = self.total_stake / 3;
        let remainder = self.total_stake % 3;
        thirds * 2
            + match remainder {
                0 => 0,
                1 => 1,
                _ => 2,
            }
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

    /// Slash a validator's active stake by `penalty_amount` and optionally jail/remove
    /// it from active duty (`B6`).
    pub fn slash_validator(&mut self, address: &Address, penalty_amount: u64, jail: bool) {
        if let Some(val) = self.validators.get_mut(address) {
            let deducted = penalty_amount.min(val.stake);
            val.stake = val.stake.saturating_sub(deducted);
            self.total_stake = self.total_stake.saturating_sub(deducted);
            if jail {
                self.total_stake = self.total_stake.saturating_sub(val.stake);
                val.status = ValidatorStatus::Exiting;
            }
        }
    }

    /// Request to enter the validator set. Added to the enter queue;
    /// processed at the next epoch boundary.
    pub fn request_enter(&mut self, address: Address, stake: u64, epoch: u64) {
        if stake < self.min_stake {
            return;
        }
        if self.validators.contains_key(&address)
            || self.enter_queue.iter().any(|r| r.address == address)
        {
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
        if !self.validators.contains_key(&address)
            || self.exit_queue.iter().any(|r| r.address == address)
        {
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
        current_tip_hash: &Hash,
    ) -> Result<(), SetUpdateError> {
        if update.epoch != self.epoch.number + 1 {
            return Err(SetUpdateError::WrongEpoch(
                update.epoch,
                self.epoch.number + 1,
            ));
        }
        if update.validators.is_empty()
            || update.validators.iter().any(|v| v.stake < self.min_stake)
        {
            return Err(SetUpdateError::InsufficientStake);
        }
        let required_sigs = quorum_threshold(self.validators.len().max(1) as u64) as usize;
        if update.qc.block_hash != *current_tip_hash
            || update.qc.signatures.len() < required_sigs
            || update.qc.signatures.len() != update.qc.public_keys.len()
        {
            return Err(SetUpdateError::InvalidQc);
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

    /// Verify the QC cryptographically under `chain_id` and apply the [`SetUpdate`].
    pub fn apply_verified_set_update(
        &mut self,
        update: SetUpdate,
        current_tip_hash: &Hash,
        chain_id: &str,
    ) -> Result<(), SetUpdateError> {
        update
            .qc
            .verify(chain_id, aura_types::Domain::Vote)
            .map_err(|_| SetUpdateError::InvalidQc)?;
        self.apply_set_update(update, current_tip_hash)
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
            Self::WrongEpoch(got, expected) => {
                write!(f, "set update targets epoch {got}, expected {expected}")
            }
            Self::InvalidQc => f.write_str("invalid set update QC"),
            Self::InsufficientStake => f.write_str("insufficient validator stake"),
        }
    }
}

impl std::error::Error for SetUpdateError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bls::{BlsSecretKey, address_from_public_key};
    use crate::finality::FinalityEngine;
    use crate::proposer::Proposer;
    use crate::qc::{QcAggregator, VoteType};

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
        let validators: Vec<Validator> = (0..4).map(|i| test_validator(i, 1000)).collect();
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
        let validators: Vec<Validator> = (0..4).map(|i| test_validator(i, 1000)).collect();
        let set = ValidatorSet::new(validators, 100);
        assert_eq!(set.quorum_stake(), 2667);
    }

    #[test]
    fn enter_and_exit_queue() {
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);

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
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);
        set.request_enter(test_address(1), 100, 0);
        assert_eq!(set.enter_queue.len(), 0);
    }

    #[test]
    fn duplicate_enter_ignored() {
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);
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
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);
        let qc = make_qc(Hash::from_bytes([0x42; 32]), 0, 0);
        let update = SetUpdate {
            epoch: 5,
            validators: vec![test_validator(1, DEFAULT_MIN_STAKE)],
            qc,
        };
        let err = set.apply_set_update(update, &Hash::from_bytes([0x42; 32]));
        assert!(matches!(err, Err(SetUpdateError::WrongEpoch(5, 1))));
    }

    #[test]
    fn set_update_rejects_insufficient_stake() {
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);
        let tip = Hash::from_bytes([0x42; 32]);
        let qc = make_qc(tip, 0, 0);
        let update = SetUpdate {
            epoch: 1,
            validators: vec![test_validator(1, DEFAULT_MIN_STAKE - 1)],
            qc,
        };
        let err = set.apply_set_update(update, &tip);
        assert_eq!(err, Err(SetUpdateError::InsufficientStake));
    }

    #[test]
    fn set_update_rejects_invalid_qc() {
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);
        let tip = Hash::from_bytes([0x42; 32]);
        let wrong_tip = Hash::from_bytes([0x99; 32]);
        let qc = make_qc(wrong_tip, 0, 0);
        let update = SetUpdate {
            epoch: 1,
            validators: vec![test_validator(1, DEFAULT_MIN_STAKE)],
            qc,
        };
        let err = set.apply_set_update(update, &tip);
        assert_eq!(err, Err(SetUpdateError::InvalidQc));
    }

    #[test]
    fn total_stake_updates_on_enter_exit() {
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);
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
        let mut set = ValidatorSet::new(vec![test_validator(0, DEFAULT_MIN_STAKE)], 100);
        set.request_exit(test_address(0), 0);
        assert!(set.is_active(&test_address(0)));
        for _ in 0..99 {
            set.advance_block();
        }
        assert!(set.is_active(&test_address(0)));
        set.advance_block();
        assert!(!set.is_active(&test_address(0)));
    }

    #[test]
    fn four_to_seven_validator_change_at_height_h_without_stopping_network() {
        const CHAIN: &str = "auralite-devnet-1";
        const HEIGHT_H: u64 = 50;

        let keys: Vec<BlsSecretKey> = (0..7u8)
            .map(|i| BlsSecretKey::from_seed(&[i + 1; 32]))
            .collect();
        let addrs: Vec<Address> = keys
            .iter()
            .map(|k| address_from_public_key(&k.public_key()))
            .collect();

        let genesis_validators: Vec<Validator> = (0..4)
            .map(|i| Validator {
                address: addrs[i],
                stake: DEFAULT_MIN_STAKE,
                status: ValidatorStatus::Active,
                epoch: 0,
            })
            .collect();

        let mut staking = StakingContract::new(genesis_validators, HEIGHT_H);
        assert_eq!(staking.validator_set().len(), 4);

        // Queue 3 additional validators (4, 5, 6) to enter at epoch boundary H = 50.
        for addr in &addrs[4..7] {
            staking
                .stake_and_enter(*addr, DEFAULT_MIN_STAKE * 2)
                .expect("valid stake enters queue");
        }

        let genesis_hash = Hash::from_bytes([0u8; 32]);
        let mut engine = FinalityEngine::new(CHAIN, 0, genesis_hash);

        // Run 100 continuous blocks across height H = 50 without stopping the network.
        for height in 1..=100u64 {
            let active_list: Vec<Address> = staking
                .validator_set()
                .active_addresses()
                .into_iter()
                .collect();
            let proposer_rot = Proposer::new(active_list);
            let _leader = proposer_rot.propose(height - 1);

            let block = engine.propose_at(
                1_700_000_000_000 + height * 100,
                vec![format!("tx-h-{height}").into_bytes()],
            );

            // Collect 2f+1 precommit votes from the active validator set (3 of 4 before H, 5 of 7 after H).
            let n_active = staking.validator_set().len() as u64;
            let needed = quorum_threshold(n_active) as usize;
            let mut agg = QcAggregator::new(quorum_threshold(n_active));
            let mut qc_opt = None;
            for (idx, addr) in addrs.iter().enumerate() {
                if !staking.validator_set().is_active(addr) {
                    continue;
                }
                let vote = engine.vote_for(&keys[idx], *addr, &block, VoteType::Precommit);
                let (maybe_qc, _newly_finalized) =
                    engine.aggregate_and_commit(&mut agg, keys[idx].public_key(), vote);
                if let Some(qc) = maybe_qc {
                    assert_eq!(qc.signatures.len(), needed);
                    qc_opt = Some(qc);
                    break;
                }
            }
            let qc = qc_opt.expect("2f+1 quorum reached");
            qc.verify(CHAIN, aura_types::Domain::Vote)
                .expect("QC verifies");
            engine.change_round();

            let crossed = staking.validator_set_mut().advance_block();
            if height < HEIGHT_H {
                assert!(crossed.is_none());
                assert_eq!(staking.validator_set().len(), 4);
            } else if height == HEIGHT_H {
                let info = crossed.expect("epoch boundary crossed at height H");
                assert_eq!(info.number, 1);
                assert_eq!(info.start_height, HEIGHT_H);
                assert_eq!(staking.validator_set().len(), 7);
            } else if height == 2 * HEIGHT_H {
                let info = crossed.expect("epoch boundary crossed at height 2*H");
                assert_eq!(info.number, 2);
                assert_eq!(staking.validator_set().len(), 7);
            } else {
                assert!(crossed.is_none());
                assert_eq!(staking.validator_set().len(), 7);
            }
        }

        // Verify the network finalized blocks continuously across the 4 -> 7 transition.
        assert!(engine.commit_rule.finalized_count() >= 95);
    }
}
