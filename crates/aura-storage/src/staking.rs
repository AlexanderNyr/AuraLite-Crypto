//! T4 staking: minimum stake, delegation with shares, validator
//! commission, 21-day unbonding and a slashing period
//! (ECONOMICS_PLAN T4).
//!
//! One validator pool with share-based delegations: a delegator holds
//! **shares** of the pool; the pool's `bonded` stake grows with rewards
//! (auto-compounding) and shrinks with slashes, so the share price
//! moves while the share counts do not. The delegator shares always sum
//! to exactly one whole pool (`Σ shares == total_shares`).
//!
//! * **min stake** — a delegation below [`T4_MIN_STAKE`] is rejected;
//! * **commission** — [`T4_COMMISSION_BPS`] of every reward goes to the
//!   validator (`pending_commission`); the rest compounds into the pool
//!   — the commission is never taken from the principal (D-T2: rewards
//!   are variable parameters, not promised income);
//! * **unbonding** — undelegated stake leaves the pool immediately and
//!   becomes withdrawable exactly [`T4_UNBONDING_BLOCKS`] later
//!   (21 days at [`T4_BLOCK_SECONDS`] per block); a withdrawal without
//!   unbonding is rejected;
//! * **slashing period** — bonded and unbonding (not yet withdrawn)
//!   stake stays slashable during the window
//!   ([`T4_SLASHING_PERIOD_BLOCKS`] = the unbonding period); a slash
//!   cuts every stake proportionally.
//!
//! The pinned scenario [`run_t4_scenario`] drives 1000 delegators,
//! rewards, two slashes, partial undelegations and matured withdrawals;
//! [`scenario_digest`] is cross-run against the independent Python
//! oracle (`tools/t4_staking_check.py`).

use crate::econ::Amount;
use crate::issuance::ALCRY;

/// Minimum stake accepted by [`StakingPool::delegate`].
pub const T4_MIN_STAKE: Amount = 1_000 * ALCRY;

/// Validator commission on rewards, in basis points (10 %).
pub const T4_COMMISSION_BPS: u64 = 1_000;

/// Basis points denominator.
pub const T4_BPS: u64 = 10_000;

/// Assumed block time in seconds.
pub const T4_BLOCK_SECONDS: u64 = 5;

/// Unbonding period in days.
pub const T4_UNBONDING_DAYS: u64 = 21;

/// Unbonding period in blocks: 21 days at 5 s per block.
pub const T4_UNBONDING_BLOCKS: u64 = T4_UNBONDING_DAYS * 86_400 / T4_BLOCK_SECONDS;

/// Slashing period in blocks — equal to the unbonding window: bonded and
/// not-yet-withdrawn stake is slashable for the whole period.
pub const T4_SLASHING_PERIOD_BLOCKS: u64 = T4_UNBONDING_BLOCKS;

/// Reward accrued to the pool per scenario step (variable parameter).
pub const T4_BLOCK_REWARD: Amount = 2 * ALCRY;

/// Errors raised by the staking checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StakeError {
    /// A delegation below [`T4_MIN_STAKE`].
    BelowMinStake,
    /// A withdrawal without any unbonding stake.
    NoUnbonding,
    /// A withdrawal before the unbonding period elapsed.
    StillUnbonding {
        /// The earliest maturity height of the owner's unbonding stake.
        matures_at: u64,
    },
    /// Undelegating more shares than the owner holds.
    InsufficientShares,
}

/// One unbonding entry: undelegated stake waiting out the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnbondingEntry {
    /// Delegator id.
    pub owner: u32,
    /// Amount leaving the pool at withdrawal.
    pub amount: Amount,
    /// Block height at which the amount becomes withdrawable.
    pub matures_at: u64,
}

/// Exact `a * b / d` with a 256-bit intermediate (floor). Panics on
/// `d == 0`; the quotient must fit in `u128`.
fn mul_div(a: u128, b: u128, d: u128) -> u128 {
    assert!(d > 0, "T4: division by zero");
    // 128x128 -> 256 schoolbook product as (hi, lo).
    let mask = u64::MAX as u128;
    let (a_lo, a_hi) = (a & mask, a >> 64);
    let (b_lo, b_hi) = (b & mask, b >> 64);
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let mid = (ll >> 64) + (lh & mask) + (hl & mask);
    let lo = (mid << 64) | (ll & mask);
    let hi = hh + (lh >> 64) + (hl >> 64) + (mid >> 64);
    if hi == 0 {
        return lo / d;
    }
    // Shift-subtract long division of the 256-bit numerator by `d`.
    let mut quotient: u128 = 0;
    let mut rem: u128 = 0;
    for i in (0..256).rev() {
        let bit = if i >= 128 {
            (hi >> (i - 128)) & 1
        } else {
            (lo >> i) & 1
        };
        let carry = rem >> 127;
        rem = (rem << 1) | bit;
        if carry != 0 || rem >= d {
            rem = rem.wrapping_sub(d);
            quotient = (quotient << 1) | 1;
        } else {
            quotient <<= 1;
        }
    }
    quotient
}

/// The staking pool of one validator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StakingPool {
    /// Total bonded stake (principal + compounded rewards − slashes).
    bonded: Amount,
    /// Sum of all delegator shares.
    total_shares: Amount,
    /// Validator commission earned (claimable by the validator).
    pending_commission: Amount,
    /// Shares per delegator id.
    delegations: Vec<Amount>,
    /// Unbonding queue.
    unbonding: Vec<UnbondingEntry>,
}

impl StakingPool {
    /// An empty pool.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total bonded stake.
    pub fn bonded(&self) -> Amount {
        self.bonded
    }

    /// Total shares.
    pub fn total_shares(&self) -> Amount {
        self.total_shares
    }

    /// Sum of all delegator shares — always equals [`Self::total_shares`].
    pub fn shares_sum(&self) -> Amount {
        self.delegations.iter().sum()
    }

    /// Validator commission earned.
    pub fn pending_commission(&self) -> Amount {
        self.pending_commission
    }

    /// Shares currently held by `owner`.
    pub fn shares_of(&self, owner: u32) -> Amount {
        self.delegations.get(owner as usize).copied().unwrap_or(0)
    }

    /// Unbonding amount of `owner` (matured or not).
    pub fn unbonding_of(&self, owner: u32) -> Amount {
        self.unbonding
            .iter()
            .filter(|entry| entry.owner == owner)
            .map(|entry| entry.amount)
            .sum()
    }

    /// Delegates `amount` (at least [`T4_MIN_STAKE`]) to the pool on
    /// behalf of `owner`; returns the shares minted.
    pub fn delegate(&mut self, owner: u32, amount: Amount) -> Result<Amount, StakeError> {
        if amount < T4_MIN_STAKE {
            return Err(StakeError::BelowMinStake);
        }
        let minted = if self.total_shares == 0 {
            amount
        } else {
            mul_div(amount, self.total_shares, self.bonded)
        };
        let slot = owner as usize;
        if slot >= self.delegations.len() {
            self.delegations.resize(slot + 1, 0);
        }
        self.delegations[slot] += minted;
        self.total_shares += minted;
        self.bonded += amount;
        Ok(minted)
    }

    /// Undelegates `shares` of `owner` at `now_block`: the proportional
    /// amount leaves the pool and enters the unbonding queue, withdrawable
    /// exactly [`T4_UNBONDING_BLOCKS`] later.
    pub fn undelegate(
        &mut self,
        owner: u32,
        shares: Amount,
        now_block: u64,
    ) -> Result<UnbondingEntry, StakeError> {
        let held = self.shares_of(owner);
        if shares == 0 || shares > held {
            return Err(StakeError::InsufficientShares);
        }
        let amount = mul_div(shares, self.bonded, self.total_shares);
        let slot = owner as usize;
        self.delegations[slot] -= shares;
        self.total_shares -= shares;
        self.bonded -= amount;
        let entry = UnbondingEntry {
            owner,
            amount,
            matures_at: now_block + T4_UNBONDING_BLOCKS,
        };
        self.unbonding.push(entry);
        Ok(entry)
    }

    /// Withdraws all matured unbonding stake of `owner` at `now_block`.
    /// Without any unbonding stake this is rejected ([`StakeError::NoUnbonding`]);
    /// before the window elapses it is rejected
    /// ([`StakeError::StillUnbonding`]) — a withdrawal needs unbonding.
    pub fn withdraw(&mut self, owner: u32, now_block: u64) -> Result<Amount, StakeError> {
        let mut claimed: Amount = 0;
        let mut kept: Vec<UnbondingEntry> = Vec::new();
        for entry in self.unbonding.drain(..) {
            if entry.owner == owner && entry.matures_at <= now_block {
                claimed += entry.amount;
            } else {
                kept.push(entry);
            }
        }
        self.unbonding = kept;
        if claimed == 0 {
            if let Some(entry) = self.unbonding.iter().find(|entry| entry.owner == owner) {
                return Err(StakeError::StillUnbonding {
                    matures_at: entry.matures_at,
                });
            }
            return Err(StakeError::NoUnbonding);
        }
        Ok(claimed)
    }

    /// Accrues a block reward: [`T4_COMMISSION_BPS`] of it is taken as
    /// the validator commission (from the reward only), the rest
    /// compounds into the bonded pool.
    pub fn accrue_reward(&mut self, reward: Amount) {
        let commission = reward * Amount::from(T4_COMMISSION_BPS) / Amount::from(T4_BPS);
        self.pending_commission += commission;
        self.bonded += reward - commission;
    }

    /// Slashes `rate_bps` of every stake proportionally: the bonded pool
    /// and all not-yet-withdrawn unbonding entries (the slashing period
    /// equals [`T4_SLASHING_PERIOD_BLOCKS`]).
    pub fn slash(&mut self, rate_bps: u64) {
        let bonded_cut = self.bonded * Amount::from(rate_bps) / Amount::from(T4_BPS);
        self.bonded -= bonded_cut;
        for entry in &mut self.unbonding {
            entry.amount -= entry.amount * Amount::from(rate_bps) / Amount::from(T4_BPS);
        }
    }
}

/// One step of the pinned T4 scenario (for the digest).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct T4Step {
    /// Step index.
    pub index: u32,
    /// Step kind: 1 delegate, 2 reward, 3 slash, 4 undelegate,
    /// 5 early-withdraw attempt, 6 withdraw, 9 final state.
    pub kind: u8,
    /// Amount of the step (delegation, reward, undelegation or
    /// withdrawal; 0 when the attempt was rejected).
    pub amount: Amount,
    /// Pool bonded stake after the step.
    pub bonded: Amount,
    /// Pool total shares after the step.
    pub total_shares: Amount,
    /// Sum of delegator shares after the step (== total shares).
    pub shares_sum: Amount,
}

fn record(steps: &mut Vec<T4Step>, kind: u8, amount: Amount, pool: &StakingPool) {
    let index = steps.len() as u32;
    steps.push(T4Step {
        index,
        kind,
        amount,
        bonded: pool.bonded(),
        total_shares: pool.total_shares(),
        shares_sum: pool.shares_sum(),
    });
}

/// The pinned T4 scenario: 1000 delegators (amounts 1000–1999 `ALCRY`),
/// 600 reward steps of [`T4_BLOCK_REWARD`], slashes of 100 and 500 bps,
/// partial undelegations of delegators 0–99 (half) and 100–199 (third),
/// one rejected early-withdrawal attempt and matured withdrawals after
/// exactly [`T4_UNBONDING_BLOCKS`].
pub fn run_t4_scenario() -> Vec<T4Step> {
    let mut pool = StakingPool::new();
    let mut steps: Vec<T4Step> = Vec::new();
    for i in 0u32..1000 {
        let amount = T4_MIN_STAKE + Amount::from(i) * T4_MIN_STAKE / 1_000;
        let minted = pool
            .delegate(i, amount)
            .expect("amount above the minimum stake");
        record(&mut steps, 1, minted, &pool);
    }
    for _ in 0..200 {
        pool.accrue_reward(T4_BLOCK_REWARD);
        record(&mut steps, 2, T4_BLOCK_REWARD, &pool);
    }
    pool.slash(100);
    record(&mut steps, 3, 100, &pool);
    for _ in 0..200 {
        pool.accrue_reward(T4_BLOCK_REWARD);
        record(&mut steps, 2, T4_BLOCK_REWARD, &pool);
    }
    for i in 0u32..100 {
        let half = pool.shares_of(i) / 2;
        let entry = pool
            .undelegate(i, half, 10_000)
            .expect("the delegator holds the shares");
        record(&mut steps, 4, entry.amount, &pool);
    }
    for i in 100u32..200 {
        let third = pool.shares_of(i) / 3;
        let entry = pool
            .undelegate(i, third, 20_000)
            .expect("the delegator holds the shares");
        record(&mut steps, 4, entry.amount, &pool);
    }
    pool.slash(500);
    record(&mut steps, 3, 500, &pool);
    for _ in 0..200 {
        pool.accrue_reward(T4_BLOCK_REWARD);
        record(&mut steps, 2, T4_BLOCK_REWARD, &pool);
    }
    // One block before maturity the withdrawal must be rejected.
    let early = pool
        .withdraw(0, 10_000 + T4_UNBONDING_BLOCKS - 1)
        .unwrap_or(0);
    record(&mut steps, 5, early, &pool);
    for i in 0u32..100 {
        let claimed = pool.withdraw(i, 10_000 + T4_UNBONDING_BLOCKS).unwrap_or(0);
        record(&mut steps, 6, claimed, &pool);
    }
    for i in 100u32..200 {
        let claimed = pool.withdraw(i, 20_000 + T4_UNBONDING_BLOCKS).unwrap_or(0);
        record(&mut steps, 6, claimed, &pool);
    }
    record(&mut steps, 9, 0, &pool);
    steps
}

/// Digest of a scenario run: one canonical line per step
/// `index:kind:amount:bonded:total_shares:shares_sum`.
pub fn scenario_digest(steps: &[T4Step]) -> String {
    let mut text = String::new();
    for step in steps {
        text.push_str(&format!(
            "{}:{}:{}:{}:{}:{}\n",
            step.index, step.kind, step.amount, step.bonded, step.total_shares, step.shares_sum
        ));
    }
    aura_state::sha256_bytes(text.as_bytes()).to_hex()
}
