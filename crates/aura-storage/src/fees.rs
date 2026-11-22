//! T3 fees: an EIP-1559-like `base_fee` + `tip` market with the base part
//! burned and an elastic gas target (ECONOMICS_PLAN T3).
//!
//! Every block sets the next base fee with the EIP-1559 formula (integer
//! arithmetic, minimum delta of one base unit off target). Transactions
//! pay `gas_used * (base_fee + tip_per_gas)`: the **base part is burned**
//! and only the tip goes to the validator. The block gas limit is
//! `T3_ELASTICITY × T3_GAS_TARGET` (elastic target).
//!
//! The pinned T3 demand scenario [`t3_demand`] covers 10 000 congested
//! blocks (bursts, lulls and exact-target blocks); [`run_scenario`] and
//! [`scenario_digest`] re-derive it deterministically and are cross-run
//! against the independent Python oracle (`tools/t3_fees_check.py`).

use crate::econ::Amount;

/// Initial base fee per gas, in `ALCRY` base units (D-T1: u128, 18 dec).
pub const T3_INITIAL_BASE_FEE: Amount = 1_000_000_000;

/// Elastic gas target per block.
pub const T3_GAS_TARGET: u64 = 15_000_000;

/// Elasticity multiplier: the block gas limit is `elasticity × target`.
pub const T3_ELASTICITY: u64 = 2;

/// Elastic block gas limit (never exceeded by a valid block).
pub const T3_GAS_LIMIT: u64 = T3_GAS_TARGET * T3_ELASTICITY;

/// EIP-1559 base fee max change denominator: at most ±12.5 % per block.
pub const T3_BASE_FEE_MAX_CHANGE_DENOMINATOR: u64 = 8;

/// Tip per gas paid to the validator, in `ALCRY` base units.
pub const T3_TIP_PER_GAS: Amount = 1_000_000_000;

/// Length of the pinned congested-blocks scenario.
pub const T3_BLOCKS: u32 = 10_000;

/// Errors raised by the fee market checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeeError {
    /// The gas target must be positive.
    ZeroTarget,
    /// A block tried to exceed the elastic gas limit.
    OverLimit {
        /// Gas used by the offending block.
        gas_used: u64,
        /// The elastic limit in force.
        limit: u64,
    },
}

/// Checks a block against the elastic gas limit.
pub fn check_gas(gas_used: u64) -> Result<(), FeeError> {
    if gas_used > T3_GAS_LIMIT {
        return Err(FeeError::OverLimit {
            gas_used,
            limit: T3_GAS_LIMIT,
        });
    }
    Ok(())
}

/// Next base fee given the parent base fee and the parent block's gas
/// usage — the EIP-1559 formula:
/// `base ± max(base * |gas_used - target| / target / 8, 1)`, unchanged
/// when `gas_used == target`, floored at zero on the way down.
pub fn next_base_fee(
    parent_base_fee: Amount,
    gas_used: u64,
    gas_target: u64,
) -> Result<Amount, FeeError> {
    if gas_target == 0 {
        return Err(FeeError::ZeroTarget);
    }
    if gas_used == gas_target {
        return Ok(parent_base_fee);
    }
    let target = Amount::from(gas_target);
    let delta_gas = Amount::from(gas_target.abs_diff(gas_used));
    let raw =
        parent_base_fee * delta_gas / target / Amount::from(T3_BASE_FEE_MAX_CHANGE_DENOMINATOR);
    let delta = std::cmp::max(raw, 1);
    if gas_used > gas_target {
        Ok(parent_base_fee + delta)
    } else {
        Ok(parent_base_fee.saturating_sub(delta))
    }
}

/// Fee split of one transaction: what is burned and what the validator
/// receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeReceipt {
    /// Burned base part: `gas_used * base_fee`.
    pub burned: Amount,
    /// Validator tip: `gas_used * tip_per_gas`.
    pub tips: Amount,
    /// Total paid by the sender: `burned + tips`.
    pub paid: Amount,
}

/// Splits the fee of one transaction into burned base part and tip.
pub fn fee_breakdown(base_fee: Amount, tip_per_gas: Amount, gas_used: u64) -> FeeReceipt {
    let gas = Amount::from(gas_used);
    let burned = base_fee * gas;
    let tips = tip_per_gas * gas;
    FeeReceipt {
        burned,
        tips,
        paid: burned + tips,
    }
}

/// One simulated block of the T3 scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockOutcome {
    /// Block index within the scenario.
    pub index: u32,
    /// Base fee at the start of the block.
    pub base_before: Amount,
    /// Gas used by the block (the pinned demand).
    pub gas_used: u64,
    /// Base fee for the next block (the formula applied).
    pub base_after: Amount,
    /// Base part burned by this block.
    pub burned: Amount,
    /// Tip paid to the validator by this block.
    pub tips: Amount,
}

/// The pinned T3 demand scenario: 250-block segments cycling +25 %,
/// −25 %, +50 %, −50 % and exactly the target — a congested network with
/// bursts, lulls and quiet blocks (mirrored by `sim/t3-fees.json`).
pub fn t3_demand(index: u32) -> u64 {
    match (index / 250) % 5 {
        0 => T3_GAS_TARGET + T3_GAS_TARGET / 4,
        1 => T3_GAS_TARGET - T3_GAS_TARGET / 4,
        2 => T3_GAS_TARGET + T3_GAS_TARGET / 2,
        3 => T3_GAS_TARGET - T3_GAS_TARGET / 2,
        _ => T3_GAS_TARGET,
    }
}

/// Runs the pinned scenario for `blocks` blocks from
/// [`T3_INITIAL_BASE_FEE`] with [`T3_TIP_PER_GAS`].
pub fn run_scenario(blocks: u32) -> Vec<BlockOutcome> {
    let mut base = T3_INITIAL_BASE_FEE;
    let mut outcomes = Vec::with_capacity(blocks as usize);
    for index in 0..blocks {
        let gas_used = t3_demand(index);
        check_gas(gas_used).expect("scenario gas stays within the elastic limit");
        let receipt = fee_breakdown(base, T3_TIP_PER_GAS, gas_used);
        let base_after =
            next_base_fee(base, gas_used, T3_GAS_TARGET).expect("the gas target is positive");
        outcomes.push(BlockOutcome {
            index,
            base_before: base,
            gas_used,
            base_after,
            burned: receipt.burned,
            tips: receipt.tips,
        });
        base = base_after;
    }
    outcomes
}

/// Digest of a scenario run: one canonical line per block
/// `index:base_before:gas_used:base_after:burned:tips`.
pub fn scenario_digest(outcomes: &[BlockOutcome]) -> String {
    let mut text = String::new();
    for outcome in outcomes {
        text.push_str(&format!(
            "{}:{}:{}:{}:{}:{}\n",
            outcome.index,
            outcome.base_before,
            outcome.gas_used,
            outcome.base_after,
            outcome.burned,
            outcome.tips
        ));
    }
    aura_state::sha256_bytes(text.as_bytes()).to_hex()
}
