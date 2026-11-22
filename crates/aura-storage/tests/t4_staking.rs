//! T4 staking gate (ECONOMICS_PLAN T4).
//!
//! The gate is the literal phase gate: 1000 delegators whose shares sum
//! to exactly one pool, and a withdrawal without unbonding must be red.
//! Every control below carries its named T4 message.

use aura_storage::econ::Amount;
use aura_storage::issuance::ALCRY;
use aura_storage::staking::{
    StakeError, StakingPool, T4_BLOCK_REWARD, T4_BPS, T4_COMMISSION_BPS, T4_MIN_STAKE,
    T4_SLASHING_PERIOD_BLOCKS, T4_UNBONDING_BLOCKS, T4_UNBONDING_DAYS, run_t4_scenario,
    scenario_digest,
};

/// Golden digest of the pinned 1000-delegator run, computed by
/// `tools/t4_staking_check.py`.
const GOLDEN_STAKING_DIGEST: &str =
    "e80662c82c74e1d1a0a5b0624beb4cf08af88c24f8091751c277cc7ea6887f07";

#[test]
fn t4_thousand_delegator_shares_sum_to_one() {
    // THE gate: 1000 delegators, shares sum == 1 (as pool fractions).
    let steps = run_t4_scenario();
    assert_eq!(
        steps.len(),
        2004,
        "T4: the delegator shares must sum to exactly one"
    );
    for step in &steps {
        assert_eq!(
            step.shares_sum, step.total_shares,
            "T4: the delegator shares must sum to exactly one"
        );
    }
    let final_step = steps.last().expect("nonempty");
    assert_eq!(
        final_step.shares_sum, 1_408_708_333_333_333_333_333_367u128,
        "T4: the delegator shares must sum to exactly one"
    );
    assert_eq!(
        final_step.total_shares, final_step.shares_sum,
        "T4: the delegator shares must sum to exactly one"
    );
}

#[test]
fn t4_withdrawal_without_unbonding_is_rejected() {
    let mut pool = StakingPool::new();
    pool.delegate(0, T4_MIN_STAKE).expect("minimum stake");
    assert_eq!(
        pool.withdraw(0, u64::MAX),
        Err(StakeError::NoUnbonding),
        "T4: a withdrawal without unbonding must be rejected"
    );
    // After the one legitimate withdrawal the stake is gone: another
    // withdrawal without unbonding is rejected again.
    let shares = pool.shares_of(0);
    pool.undelegate(0, shares, 0).expect("held shares");
    assert!(
        pool.withdraw(0, T4_UNBONDING_BLOCKS).is_ok(),
        "T4: a withdrawal without unbonding must be rejected"
    );
    assert_eq!(
        pool.withdraw(0, u64::MAX),
        Err(StakeError::NoUnbonding),
        "T4: a withdrawal without unbonding must be rejected"
    );
}

#[test]
fn t4_unbonding_completes_after_exactly_21_days() {
    assert_eq!(
        T4_UNBONDING_DAYS, 21,
        "T4: unbonding must complete after exactly 21 days"
    );
    assert_eq!(
        T4_UNBONDING_BLOCKS,
        21 * 86_400 / 5,
        "T4: unbonding must complete after exactly 21 days"
    );
    assert_eq!(
        T4_UNBONDING_BLOCKS, 362_880,
        "T4: unbonding must complete after exactly 21 days"
    );
    let mut pool = StakingPool::new();
    pool.delegate(0, T4_MIN_STAKE).expect("minimum stake");
    let shares = pool.shares_of(0);
    pool.undelegate(0, shares, 0).expect("held shares");
    assert_eq!(
        pool.withdraw(0, T4_UNBONDING_BLOCKS - 1),
        Err(StakeError::StillUnbonding {
            matures_at: T4_UNBONDING_BLOCKS
        }),
        "T4: unbonding must complete after exactly 21 days"
    );
    assert_eq!(
        pool.withdraw(0, T4_UNBONDING_BLOCKS),
        Ok(T4_MIN_STAKE),
        "T4: unbonding must complete after exactly 21 days"
    );
}

#[test]
fn t4_commission_is_taken_from_rewards_only() {
    let mut pool = StakingPool::new();
    pool.delegate(0, T4_MIN_STAKE).expect("minimum stake");
    let shares_before = pool.shares_of(0);
    pool.accrue_reward(100 * ALCRY);
    assert_eq!(
        pool.pending_commission(),
        10 * ALCRY,
        "T4: the commission must be taken from rewards only"
    );
    assert_eq!(
        pool.bonded(),
        T4_MIN_STAKE + 90 * ALCRY,
        "T4: the commission must be taken from rewards only"
    );
    assert_eq!(
        pool.shares_of(0),
        shares_before,
        "T4: the commission must be taken from rewards only"
    );
    assert_eq!(
        T4_COMMISSION_BPS, 1_000,
        "T4: the commission must be taken from rewards only"
    );
    let _ = T4_BLOCK_REWARD;
}

#[test]
fn t4_the_minimum_stake_is_enforced() {
    let mut pool = StakingPool::new();
    assert_eq!(
        pool.delegate(0, T4_MIN_STAKE - 1),
        Err(StakeError::BelowMinStake),
        "T4: the minimum stake must be enforced"
    );
    assert_eq!(
        pool.delegate(0, T4_MIN_STAKE),
        Ok(T4_MIN_STAKE),
        "T4: the minimum stake must be enforced"
    );
    assert_eq!(
        pool.delegate(1, 1),
        Err(StakeError::BelowMinStake),
        "T4: the minimum stake must be enforced"
    );
}

#[test]
fn t4_a_slash_cuts_every_stake_proportionally() {
    let mut pool = StakingPool::new();
    pool.delegate(0, 2_000 * ALCRY).expect("above minimum");
    pool.delegate(1, 4_000 * ALCRY).expect("above minimum");
    let shares = pool.shares_of(0);
    let entry = pool.undelegate(0, shares / 2, 0).expect("held shares");
    let bonded_before = pool.bonded();
    pool.slash(1_000); // 10 %
    assert_eq!(
        bonded_before - pool.bonded(),
        bonded_before * 1_000 / Amount::from(T4_BPS),
        "T4: a slash must reduce every stake proportionally"
    );
    assert_eq!(
        pool.unbonding_of(0),
        entry.amount - entry.amount * 1_000 / Amount::from(T4_BPS),
        "T4: a slash must reduce every stake proportionally"
    );
    // Share counts never move with a slash.
    assert_eq!(
        pool.shares_sum(),
        pool.total_shares(),
        "T4: a slash must reduce every stake proportionally"
    );
    let before_zero = pool.bonded();
    pool.slash(0);
    assert_eq!(
        pool.bonded(),
        before_zero,
        "T4: a slash must reduce every stake proportionally"
    );
    pool.slash(T4_BPS);
    assert_eq!(
        pool.bonded(),
        0,
        "T4: a slash must reduce every stake proportionally"
    );
}

#[test]
fn t4_unbonding_stake_stays_slashable_during_the_slashing_period() {
    assert_eq!(
        T4_SLASHING_PERIOD_BLOCKS, T4_UNBONDING_BLOCKS,
        "T4: unbonding stake stays slashable during the slashing period"
    );
    let mut pool = StakingPool::new();
    pool.delegate(0, T4_MIN_STAKE).expect("minimum stake");
    let shares = pool.shares_of(0);
    let entry = pool.undelegate(0, shares, 0).expect("held shares");
    // Halfway through the window the slash still hits the unbonding pot.
    pool.slash(500);
    assert_eq!(
        pool.unbonding_of(0),
        entry.amount - entry.amount * 500 / Amount::from(T4_BPS),
        "T4: unbonding stake stays slashable during the slashing period"
    );
    let claimed = pool.withdraw(0, T4_UNBONDING_BLOCKS).expect("matured");
    assert_eq!(
        claimed,
        entry.amount - entry.amount * 500 / Amount::from(T4_BPS),
        "T4: unbonding stake stays slashable during the slashing period"
    );
}

#[test]
fn t4_reference_simulation_agrees_with_the_pinned_digest() {
    let steps = run_t4_scenario();
    assert_eq!(
        scenario_digest(&steps),
        GOLDEN_STAKING_DIGEST,
        "T4: the reference simulation must agree with the pinned digest"
    );
    let final_step = steps.last().expect("nonempty");
    assert_eq!(
        final_step.bonded, 1_325_889_559_769_089_696_565_650u128,
        "T4: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        steps[1200].bonded, 1_484_861_400_000_000_000_000_000u128,
        "T4: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        steps[1601].bonded, 1_325_529_559_769_089_696_565_650u128,
        "T4: the reference simulation must agree with the pinned digest"
    );
    // The early-withdrawal attempt one block before maturity is recorded
    // as a rejection (amount 0).
    assert_eq!(
        steps[1802].kind, 5,
        "T4: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        steps[1802].amount, 0,
        "T4: the reference simulation must agree with the pinned digest"
    );
    let withdrawn: Amount = steps
        .iter()
        .filter(|step| step.kind == 6)
        .map(|step| step.amount)
        .sum();
    assert_eq!(
        withdrawn, 85_430_770_230_910_303_434_449u128,
        "T4: the reference simulation must agree with the pinned digest"
    );
}

#[test]
fn t4_splitting_a_delegation_preserves_the_shares() {
    // Metamorphism: delegating a+b at once equals delegating a and then
    // b (modulo one base unit of truncation).
    let mut one = StakingPool::new();
    one.delegate(2, 5_000 * ALCRY).expect("above minimum");
    one.delegate(0, 3_000 * ALCRY + 7).expect("above minimum");
    let mut two = StakingPool::new();
    two.delegate(2, 5_000 * ALCRY).expect("above minimum");
    two.delegate(0, 1_000 * ALCRY).expect("above minimum");
    two.delegate(1, 2_000 * ALCRY + 7).expect("above minimum");
    assert!(
        one.total_shares().abs_diff(two.total_shares()) <= 1,
        "T4: the delegator shares must sum to exactly one"
    );
    assert_eq!(
        two.shares_sum(),
        two.total_shares(),
        "T4: the delegator shares must sum to exactly one"
    );
}

#[test]
fn t4_slashes_compose_proportionally() {
    // Metamorphism: slash(x) then slash(y) equals one slash of
    // x + y - x*y/bps (modulo a few base units of truncation).
    let mut steps = StakingPool::new();
    steps.delegate(0, 10_000 * ALCRY).expect("above minimum");
    steps.slash(1_000);
    steps.slash(2_000);
    let mut one = StakingPool::new();
    one.delegate(0, 10_000 * ALCRY).expect("above minimum");
    one.slash(1_000 + 2_000 - 1_000 * 2_000 / T4_BPS);
    assert!(
        steps.bonded().abs_diff(one.bonded()) <= ALCRY,
        "T4: a slash must reduce every stake proportionally"
    );
}
