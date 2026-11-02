//! B7 — Consensus rewards: block inflation/reward emission, transaction fee
//! burn, stake delegation, validator commission, and the conservation
//! invariant `sum(balances) + burned == issued` (within 1 unit over 100,000
//! blocks).

use std::collections::BTreeMap;
use std::fmt;

use aura_types::Address;

/// Default block inflation reward (50,000 native sub-units per block).
pub const DEFAULT_BASE_BLOCK_REWARD: u64 = 50_000;

/// Default transaction fee burn share in basis points (5,000 bps = 50% burned).
pub const DEFAULT_FEE_BURN_BPS: u16 = 5_000;

/// Default proposer bonus share of distributable block rewards (1,000 bps = 10%).
pub const DEFAULT_PROPOSER_BONUS_BPS: u16 = 1_000;

/// Maximum valid basis points (100% = 10,000 bps).
pub const MAX_BPS: u16 = 10_000;

/// Errors returned by the B7 rewards and delegation subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewardError {
    /// Basis-point parameter exceeds `10_000` (100%).
    InvalidBps(&'static str, u16),
    /// Validator commission exceeds `10_000` bps.
    InvalidCommissionBps(u16),
    /// Referenced validator is not registered in the rewards engine.
    UnknownValidator(Address),
    /// No active stake exists in the validator set.
    ZeroTotalStake,
    /// Fee payer does not have enough balance to pay `tx_fees`.
    InsufficientFeePayerBalance {
        /// Fee payer address.
        payer: Address,
        /// Available balance.
        available: u128,
        /// Required transaction fee.
        required: u128,
    },
    /// Conservation invariant `sum(balances) + burned == issued` violated by more than 1 unit.
    SumInvariantViolation {
        /// Sum of all balances.
        sum_balances: u128,
        /// Total burned fees.
        burned: u128,
        /// Total issued supply (`initial_supply + total_minted`).
        issued: u128,
        /// Absolute discrepancy `|(sum_balances + burned) - issued|`.
        discrepancy: u128,
    },
}

impl fmt::Display for RewardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBps(field, bps) => write!(f, "invalid {field} bps: {bps} > 10000"),
            Self::InvalidCommissionBps(bps) => {
                write!(f, "invalid validator commission bps: {bps} > 10000")
            }
            Self::UnknownValidator(addr) => write!(f, "unknown validator {addr}"),
            Self::ZeroTotalStake => f.write_str("total active validator stake is zero"),
            Self::InsufficientFeePayerBalance {
                payer,
                available,
                required,
            } => write!(f, "fee payer {payer} has {available}, required {required}"),
            Self::SumInvariantViolation {
                sum_balances,
                burned,
                issued,
                discrepancy,
            } => write!(
                f,
                "sum invariant violated: sum_balances({sum_balances}) + burned({burned}) != issued({issued}), discrepancy={discrepancy}"
            ),
        }
    }
}

impl std::error::Error for RewardError {}

/// Protocol parameters governing B7 block rewards, fee burn, and proposer bonus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardParams {
    /// New tokens minted per block as inflation reward.
    pub base_block_reward: u64,
    /// Share of transaction fees burned in basis points (`0..=10_000`).
    pub fee_burn_bps: u16,
    /// Share of distributable reward allocated as proposer bonus (`0..=10_000`).
    pub proposer_bonus_bps: u16,
}

impl Default for RewardParams {
    fn default() -> Self {
        Self {
            base_block_reward: DEFAULT_BASE_BLOCK_REWARD,
            fee_burn_bps: DEFAULT_FEE_BURN_BPS,
            proposer_bonus_bps: DEFAULT_PROPOSER_BONUS_BPS,
        }
    }
}

impl RewardParams {
    /// Validate that all basis-point parameters are `<= 10_000`.
    pub fn validate(&self) -> Result<(), RewardError> {
        if self.fee_burn_bps > MAX_BPS {
            return Err(RewardError::InvalidBps("fee_burn_bps", self.fee_burn_bps));
        }
        if self.proposer_bonus_bps > MAX_BPS {
            return Err(RewardError::InvalidBps(
                "proposer_bonus_bps",
                self.proposer_bonus_bps,
            ));
        }
        Ok(())
    }
}

/// Per-validator stake, commission, and delegator table for B7 reward distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorRewardConfig {
    /// Validator address.
    pub address: Address,
    /// Validator self-bonded stake.
    pub self_stake: u64,
    /// Validator commission rate in basis points (`0..=10_000`).
    pub commission_bps: u16,
    /// Delegator stakes mapped by delegator address.
    pub delegations: BTreeMap<Address, u64>,
}

impl ValidatorRewardConfig {
    /// Construct a validator reward configuration and validate `commission_bps`.
    pub fn new(
        address: Address,
        self_stake: u64,
        commission_bps: u16,
    ) -> Result<Self, RewardError> {
        if commission_bps > MAX_BPS {
            return Err(RewardError::InvalidCommissionBps(commission_bps));
        }
        Ok(Self {
            address,
            self_stake,
            commission_bps,
            delegations: BTreeMap::new(),
        })
    }

    /// Total stake bonded to this validator (`self_stake + sum(delegations)`).
    pub fn total_stake(&self) -> u128 {
        let del_sum: u128 = self.delegations.values().map(|&x| x as u128).sum();
        (self.self_stake as u128) + del_sum
    }
}

/// Receipt summarizing reward emission, fee burn, and commission/delegator payouts for one block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRewardReceipt {
    /// Block height.
    pub height: u64,
    /// Block proposer address.
    pub proposer: Address,
    /// Inflation minted in this block (`base_block_reward`).
    pub minted_reward: u64,
    /// Total transaction fees paid in this block.
    pub total_tx_fees: u64,
    /// Portion of `total_tx_fees` permanently burned.
    pub burned_fees: u64,
    /// Portion distributed across proposer, validators, and delegators.
    pub distributed_reward: u64,
}

/// B7 Rewards engine tracking validator stakes, delegations, commissions,
/// account balances, burned fees, and total issued supply.
#[derive(Debug, Clone)]
pub struct RewardsEngine {
    params: RewardParams,
    validators: BTreeMap<Address, ValidatorRewardConfig>,
    balances: BTreeMap<Address, u128>,
    initial_supply: u128,
    total_minted: u128,
    total_burned: u128,
    blocks_processed: u64,
}

impl RewardsEngine {
    /// Initialize a [`RewardsEngine`] from `params`, `validators`, and any initial liquid
    /// `extra_balances` (e.g. transaction fee-payer accounts).
    pub fn new(
        params: RewardParams,
        validators: Vec<ValidatorRewardConfig>,
        extra_balances: BTreeMap<Address, u128>,
    ) -> Result<Self, RewardError> {
        params.validate()?;
        let mut val_map = BTreeMap::new();
        let mut balances = extra_balances;

        for v in validators {
            if v.commission_bps > MAX_BPS {
                return Err(RewardError::InvalidCommissionBps(v.commission_bps));
            }
            *balances.entry(v.address).or_insert(0) += v.self_stake as u128;
            for (&delegator, &amount) in &v.delegations {
                *balances.entry(delegator).or_insert(0) += amount as u128;
            }
            val_map.insert(v.address, v);
        }

        let initial_supply: u128 = balances.values().copied().sum();
        Ok(Self {
            params,
            validators: val_map,
            balances,
            initial_supply,
            total_minted: 0,
            total_burned: 0,
            blocks_processed: 0,
        })
    }

    /// Add a delegation from `delegator` to `validator`, funding `amount` into initial supply
    /// (or transferring from `delegator`'s existing balance if `from_existing_balance` is true).
    pub fn delegate(
        &mut self,
        delegator: Address,
        validator: Address,
        amount: u64,
    ) -> Result<(), RewardError> {
        let val = self
            .validators
            .get_mut(&validator)
            .ok_or(RewardError::UnknownValidator(validator))?;
        *val.delegations.entry(delegator).or_insert(0) += amount;
        *self.balances.entry(delegator).or_insert(0) += amount as u128;
        self.initial_supply += amount as u128;
        Ok(())
    }

    /// Current balance (staked + accumulated rewards) of `address`.
    pub fn balance_of(&self, address: &Address) -> u128 {
        self.balances.get(address).copied().unwrap_or(0)
    }

    /// Sum of all balances tracked in the engine (`sum(balances)`).
    pub fn sum_balances(&self) -> u128 {
        self.balances.values().copied().sum()
    }

    /// Cumulative transaction fees burned (`burned`).
    pub const fn total_burned(&self) -> u128 {
        self.total_burned
    }

    /// Cumulative block inflation minted (`total_minted`).
    pub const fn total_minted(&self) -> u128 {
        self.total_minted
    }

    /// Total tokens ever issued (`initial_supply + total_minted`).
    pub const fn total_issued(&self) -> u128 {
        self.initial_supply + self.total_minted
    }

    /// Number of blocks processed.
    pub const fn blocks_processed(&self) -> u64 {
        self.blocks_processed
    }

    /// Verify the B7 conservation invariant: `| (sum(balances) + burned) - issued | <= 1`.
    pub fn verify_sum_invariant(&self) -> Result<(), RewardError> {
        let sum_balances = self.sum_balances();
        let accounted = sum_balances.saturating_add(self.total_burned);
        let issued = self.total_issued();
        let discrepancy = accounted.abs_diff(issued);
        if discrepancy > 1 {
            return Err(RewardError::SumInvariantViolation {
                sum_balances,
                burned: self.total_burned,
                issued,
                discrepancy,
            });
        }
        Ok(())
    }

    /// Process block rewards and transaction fees for block `height` proposed by `proposer`.
    pub fn apply_block_rewards(
        &mut self,
        height: u64,
        proposer: Address,
        total_tx_fees: u64,
        fee_payer: Option<Address>,
    ) -> Result<BlockRewardReceipt, RewardError> {
        if !self.validators.contains_key(&proposer) {
            return Err(RewardError::UnknownValidator(proposer));
        }
        let total_stake: u128 = self.validators.values().map(|v| v.total_stake()).sum();
        if total_stake == 0 {
            return Err(RewardError::ZeroTotalStake);
        }

        // Deduct transaction fees from fee_payer (if fees > 0).
        if total_tx_fees > 0 {
            let payer = fee_payer.unwrap_or(proposer);
            let available = self.balance_of(&payer);
            if available < total_tx_fees as u128 {
                return Err(RewardError::InsufficientFeePayerBalance {
                    payer,
                    available,
                    required: total_tx_fees as u128,
                });
            }
            *self.balances.get_mut(&payer).unwrap() -= total_tx_fees as u128;
        }

        let minted = self.params.base_block_reward as u128;
        self.total_minted += minted;

        let fees = total_tx_fees as u128;
        let burned_fees = (fees * (self.params.fee_burn_bps as u128)) / (MAX_BPS as u128);
        self.total_burned += burned_fees;

        let distributable = minted + (fees - burned_fees);
        let proposer_bonus =
            (distributable * (self.params.proposer_bonus_bps as u128)) / (MAX_BPS as u128);
        let committee_pool = distributable - proposer_bonus;

        let mut distributed_so_far = 0u128;

        // 1. Distribute proposer bonus through the proposer's validator pool.
        distributed_so_far += self.distribute_to_validator(&proposer, proposer_bonus);

        // 2. Distribute committee pool across all validators proportionally to stake.
        let val_addrs: Vec<Address> = self.validators.keys().copied().collect();
        for addr in val_addrs {
            let v_stake = self.validators[&addr].total_stake();
            let v_share = (committee_pool * v_stake) / total_stake;
            distributed_so_far += self.distribute_to_validator(&addr, v_share);
        }

        // Route any integer-division remainder directly to the block proposer so
        // `sum(balances) + burned == issued` holds with zero drift.
        let remainder = distributable.saturating_sub(distributed_so_far);
        if remainder > 0 {
            *self.balances.entry(proposer).or_insert(0) += remainder;
        }

        self.blocks_processed += 1;

        Ok(BlockRewardReceipt {
            height,
            proposer,
            minted_reward: self.params.base_block_reward,
            total_tx_fees,
            burned_fees: burned_fees as u64,
            distributed_reward: distributable as u64,
        })
    }

    fn distribute_to_validator(&mut self, validator_addr: &Address, gross_reward: u128) -> u128 {
        if gross_reward == 0 {
            return 0;
        }
        let val = &self.validators[validator_addr];
        let v_total_stake = val.total_stake();
        if v_total_stake == 0 {
            return 0;
        }

        // Validator commission is taken first.
        let commission = (gross_reward * (val.commission_bps as u128)) / (MAX_BPS as u128);
        let delegator_pool = gross_reward - commission;

        let mut paid = commission;
        let self_share = (delegator_pool * (val.self_stake as u128)) / v_total_stake;
        paid += self_share;

        let mut del_payouts: Vec<(Address, u128)> = Vec::with_capacity(val.delegations.len());
        for (&del_addr, &del_stake) in &val.delegations {
            let share = (delegator_pool * (del_stake as u128)) / v_total_stake;
            paid += share;
            del_payouts.push((del_addr, share));
        }

        // Credit validator commission + self_share + any intra-validator rounding dust.
        let val_dust = gross_reward.saturating_sub(paid);
        *self.balances.entry(*validator_addr).or_insert(0) += commission + self_share + val_dust;

        for (del_addr, share) in del_payouts {
            if share > 0 {
                *self.balances.entry(del_addr).or_insert(0) += share;
            }
        }

        gross_reward
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_engine() -> (RewardsEngine, Vec<Address>, Vec<Address>, Address) {
        let v0 = Address::from_bytes([0x01; 20]);
        let v1 = Address::from_bytes([0x02; 20]);
        let v2 = Address::from_bytes([0x03; 20]);
        let v3 = Address::from_bytes([0x04; 20]);
        let d0 = Address::from_bytes([0x11; 20]);
        let d1 = Address::from_bytes([0x12; 20]);
        let fee_payer = Address::from_bytes([0xFE; 20]);

        let mut cfg0 = ValidatorRewardConfig::new(v0, 10_000_000, 1_000).unwrap(); // 10% commission
        cfg0.delegations.insert(d0, 5_000_000);
        cfg0.delegations.insert(d1, 15_000_000);

        let mut cfg1 = ValidatorRewardConfig::new(v1, 20_000_000, 500).unwrap(); // 5% commission
        cfg1.delegations.insert(d0, 10_000_000);

        let cfg2 = ValidatorRewardConfig::new(v2, 30_000_000, 2_000).unwrap(); // 20% commission
        let cfg3 = ValidatorRewardConfig::new(v3, 40_000_000, 0).unwrap(); // 0% commission

        let mut extra = BTreeMap::new();
        extra.insert(fee_payer, 5_000_000_000);

        let engine =
            RewardsEngine::new(RewardParams::default(), vec![cfg0, cfg1, cfg2, cfg3], extra)
                .unwrap();
        (engine, vec![v0, v1, v2, v3], vec![d0, d1], fee_payer)
    }

    #[test]
    fn hundred_thousand_blocks_sum_invariant_holds_within_one_unit() {
        let (mut engine, validators, _delegators, fee_payer) = make_test_engine();

        for height in 1..=100_000u64 {
            let proposer = validators[(height as usize) % validators.len()];
            let tx_fee = 1_000 + (height % 777);
            engine
                .apply_block_rewards(height, proposer, tx_fee, Some(fee_payer))
                .unwrap();
        }

        assert_eq!(engine.blocks_processed(), 100_000);
        engine.verify_sum_invariant().unwrap();
        let diff = (engine.sum_balances() + engine.total_burned()).abs_diff(engine.total_issued());
        assert!(diff <= 1, "discrepancy after 100,000 blocks was {diff}");
        assert_eq!(diff, 0);
    }

    #[test]
    fn fee_burn_deducts_and_accumulates_burned_fees() {
        let (mut engine, validators, _, fee_payer) = make_test_engine();
        let receipt = engine
            .apply_block_rewards(1, validators[0], 10_000, Some(fee_payer))
            .unwrap();
        // Default fee_burn_bps = 5,000 (50%): 5,000 burned, 5,000 added to 50,000 base reward = 55,000 distributed.
        assert_eq!(receipt.burned_fees, 5_000);
        assert_eq!(receipt.distributed_reward, 55_000);
        assert_eq!(engine.total_burned(), 5_000);
        engine.verify_sum_invariant().unwrap();
    }

    #[test]
    fn validator_commission_is_paid_before_delegator_split() {
        let v0 = Address::from_bytes([0x01; 20]);
        let d0 = Address::from_bytes([0x11; 20]);
        // Validator has 50% self-stake, delegator has 50% stake, validator commission = 20% (2,000 bps).
        let mut cfg = ValidatorRewardConfig::new(v0, 10_000_000, 2_000).unwrap();
        cfg.delegations.insert(d0, 10_000_000);

        let mut engine =
            RewardsEngine::new(RewardParams::default(), vec![cfg], BTreeMap::new()).unwrap();
        engine.apply_block_rewards(1, v0, 0, None).unwrap();

        // Total reward = 50,000.
        // Commission = 20% of 50,000 = 10,000.
        // Remaining 40,000 split 50/50 between v0 (20,000) and d0 (20,000).
        // So v0 gains 30,000 and d0 gains 20,000.
        assert_eq!(engine.balance_of(&v0) - 10_000_000, 30_000);
        assert_eq!(engine.balance_of(&d0) - 10_000_000, 20_000);
    }

    #[test]
    fn delegator_rewards_scale_proportionally_with_delegated_stake() {
        let v0 = Address::from_bytes([0x01; 20]);
        let d0 = Address::from_bytes([0x11; 20]);
        let d1 = Address::from_bytes([0x12; 20]);
        // 0% commission; d0 has 10M, d1 has 30M (1:3 ratio).
        let mut cfg = ValidatorRewardConfig::new(v0, 10_000_000, 0).unwrap();
        cfg.delegations.insert(d0, 10_000_000);
        cfg.delegations.insert(d1, 30_000_000);

        let mut engine =
            RewardsEngine::new(RewardParams::default(), vec![cfg], BTreeMap::new()).unwrap();
        engine.apply_block_rewards(1, v0, 0, None).unwrap();

        let gain_d0 = engine.balance_of(&d0) - 10_000_000;
        let gain_d1 = engine.balance_of(&d1) - 30_000_000;
        assert_eq!(gain_d0, 10_000);
        assert_eq!(gain_d1, 30_000);
        assert_eq!(gain_d1, gain_d0 * 3);
    }

    #[test]
    fn sum_invariant_detects_unbacked_mint_or_leak() {
        let (mut engine, validators, _, _) = make_test_engine();
        engine.verify_sum_invariant().unwrap();
        // Inject 5 unbacked units into validator 0's balance.
        *engine.balances.get_mut(&validators[0]).unwrap() += 5;
        assert!(matches!(
            engine.verify_sum_invariant(),
            Err(RewardError::SumInvariantViolation { discrepancy: 5, .. })
        ));
    }

    #[test]
    fn invalid_commission_rate_above_hundred_percent_is_rejected() {
        let v0 = Address::from_bytes([0x01; 20]);
        assert_eq!(
            ValidatorRewardConfig::new(v0, 10_000_000, 10_001),
            Err(RewardError::InvalidCommissionBps(10_001))
        );
    }
}
