//! T3 fees gate (ECONOMICS_PLAN T3).
//!
//! The gate is the literal phase gate: 10 000 congested blocks where the
//! base fee rises and falls **per the formula** at every step. Every
//! control below carries its named T3 message.

use aura_storage::econ::Amount;
use aura_storage::fees::{
    FeeError, T3_BASE_FEE_MAX_CHANGE_DENOMINATOR, T3_BLOCKS, T3_GAS_LIMIT, T3_GAS_TARGET,
    T3_INITIAL_BASE_FEE, T3_TIP_PER_GAS, check_gas, fee_breakdown, next_base_fee, run_scenario,
    scenario_digest, t3_demand,
};

/// Golden path digest of the pinned 10 000-block run, computed by
/// `tools/t3_fees_check.py`.
const GOLDEN_FEE_DIGEST: &str = "412f1f9e120ec829fb5d2ae65e4590da69771cd7b49bef83f29440d0d93bc124";

/// Independent re-derivation of the EIP-1559 formula. Kept separate from
/// the module so the mutation controls bite on the module only.
fn reference_next(parent: Amount, gas: u64) -> Amount {
    if gas == T3_GAS_TARGET {
        return parent;
    }
    let delta_gas = gas.abs_diff(T3_GAS_TARGET);
    let raw = parent * Amount::from(delta_gas)
        / Amount::from(T3_GAS_TARGET)
        / Amount::from(T3_BASE_FEE_MAX_CHANGE_DENOMINATOR);
    let delta = std::cmp::max(raw, 1);
    if gas > T3_GAS_TARGET {
        parent + delta
    } else {
        parent.saturating_sub(delta)
    }
}

#[test]
fn t3_ten_thousand_blocks_follow_the_eip1559_formula() {
    // THE gate: 10 000 congested blocks, base fee rises/falls per formula.
    let outcomes = run_scenario(T3_BLOCKS);
    assert_eq!(
        outcomes.len(),
        T3_BLOCKS as usize,
        "T3: the base fee must follow the EIP-1559 formula"
    );
    let mut previous_after = T3_INITIAL_BASE_FEE;
    for outcome in &outcomes {
        assert_eq!(
            outcome.base_before, previous_after,
            "T3: the base fee must follow the EIP-1559 formula"
        );
        assert_eq!(
            outcome.base_after,
            reference_next(outcome.base_before, outcome.gas_used),
            "T3: the base fee must follow the EIP-1559 formula"
        );
        previous_after = outcome.base_after;
    }
    assert_eq!(
        next_base_fee(10, 5, 0),
        Err(FeeError::ZeroTarget),
        "T3: the base fee must follow the EIP-1559 formula"
    );
}

#[test]
fn t3_congestion_raises_the_base_fee_and_slack_lowers_it() {
    let outcomes = run_scenario(T3_BLOCKS);
    for outcome in &outcomes {
        let segment = (outcome.index / 250) % 5;
        match segment {
            0 | 2 => assert!(
                outcome.base_after > outcome.base_before,
                "T3: congestion must raise the base fee and slack must lower it"
            ),
            1 | 3 => assert!(
                outcome.base_after < outcome.base_before,
                "T3: congestion must raise the base fee and slack must lower it"
            ),
            _ => assert_eq!(
                outcome.base_after, outcome.base_before,
                "T3: congestion must raise the base fee and slack must lower it"
            ),
        }
    }
}

#[test]
fn t3_the_base_part_is_burned() {
    for i in [0u32, 1, 250, 500, 1250, 5000, 9999] {
        let gas = t3_demand(i);
        let receipt = fee_breakdown(T3_INITIAL_BASE_FEE, T3_TIP_PER_GAS, gas);
        assert_eq!(
            receipt.burned,
            T3_INITIAL_BASE_FEE * Amount::from(gas),
            "T3: the base part must be burned"
        );
        assert_eq!(
            receipt.paid,
            receipt.burned + receipt.tips,
            "T3: the base part must be burned"
        );
    }
    let outcomes = run_scenario(T3_BLOCKS);
    let burned: Amount = outcomes.iter().map(|outcome| outcome.burned).sum();
    let expected: Amount = outcomes
        .iter()
        .map(|outcome| outcome.base_before * Amount::from(outcome.gas_used))
        .sum();
    assert_eq!(burned, expected, "T3: the base part must be burned");
    assert_eq!(
        burned, 2_039_212_976_242_596_352_500_000u128,
        "T3: the base part must be burned"
    );
}

#[test]
fn t3_the_tip_goes_to_the_validator() {
    for i in [0u32, 250, 500, 9999] {
        let gas = t3_demand(i);
        let receipt = fee_breakdown(T3_INITIAL_BASE_FEE, T3_TIP_PER_GAS, gas);
        assert_eq!(
            receipt.tips,
            T3_TIP_PER_GAS * Amount::from(gas),
            "T3: the tip must go to the validator"
        );
        assert!(
            receipt.tips < receipt.paid,
            "T3: the tip must go to the validator"
        );
    }
    let outcomes = run_scenario(T3_BLOCKS);
    let tips: Amount = outcomes.iter().map(|outcome| outcome.tips).sum();
    assert_eq!(
        tips, 150_000_000_000_000_000_000u128,
        "T3: the tip must go to the validator"
    );
}

#[test]
fn t3_a_block_must_not_exceed_the_elastic_gas_limit() {
    assert_eq!(
        check_gas(T3_GAS_LIMIT),
        Ok(()),
        "T3: a block must not exceed the elastic gas limit"
    );
    assert_eq!(
        check_gas(T3_GAS_LIMIT + 1),
        Err(FeeError::OverLimit {
            gas_used: T3_GAS_LIMIT + 1,
            limit: T3_GAS_LIMIT
        }),
        "T3: a block must not exceed the elastic gas limit"
    );
    assert_eq!(
        T3_GAS_LIMIT,
        T3_GAS_TARGET * 2,
        "T3: a block must not exceed the elastic gas limit"
    );
    for i in 0..T3_BLOCKS {
        check_gas(t3_demand(i)).expect("scenario gas stays within the elastic limit");
    }
}

#[test]
fn t3_reference_simulation_agrees_with_the_pinned_digest() {
    let outcomes = run_scenario(T3_BLOCKS);
    assert_eq!(
        scenario_digest(&outcomes),
        GOLDEN_FEE_DIGEST,
        "T3: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        outcomes.last().expect("nonempty").base_after,
        56_473,
        "T3: the reference simulation must agree with the pinned digest"
    );
    let peak = outcomes
        .iter()
        .map(|outcome| outcome.base_after)
        .max()
        .expect("nonempty");
    assert_eq!(
        peak, 2_993_325_499_252_235u128,
        "T3: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        outcomes[249].base_after, 2_192_756_374_619,
        "T3: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        outcomes[499].base_after, 783_284_026,
        "T3: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        t3_demand(0),
        18_750_000,
        "T3: the reference simulation must agree with the pinned digest"
    );
}

#[test]
fn t3_at_target_blocks_keep_the_base_fee_flat() {
    let base = T3_INITIAL_BASE_FEE;
    for _ in 0..128 {
        assert_eq!(
            next_base_fee(base, T3_GAS_TARGET, T3_GAS_TARGET),
            Ok(base),
            "T3: the base fee must follow the EIP-1559 formula"
        );
    }
    // One unit of demand off the target moves the fee by at least one
    // base unit (the EIP-1559 minimum delta).
    assert_eq!(
        next_base_fee(3, T3_GAS_TARGET + 1, T3_GAS_TARGET),
        Ok(4),
        "T3: the base fee must follow the EIP-1559 formula"
    );
    assert_eq!(
        next_base_fee(3, T3_GAS_TARGET - 1, T3_GAS_TARGET),
        Ok(2),
        "T3: the base fee must follow the EIP-1559 formula"
    );
}

#[test]
fn t3_splitting_a_block_preserves_burn_and_tips() {
    // Metamorphism: splitting one transaction into two (same base fee
    // and tip) preserves the burn, the tips and the total paid.
    for i in [0u32, 250, 500, 750, 9999] {
        let gas = t3_demand(i);
        let whole = fee_breakdown(T3_INITIAL_BASE_FEE, T3_TIP_PER_GAS, gas);
        let half = gas / 2;
        let rest = gas - half;
        let first = fee_breakdown(T3_INITIAL_BASE_FEE, T3_TIP_PER_GAS, half);
        let second = fee_breakdown(T3_INITIAL_BASE_FEE, T3_TIP_PER_GAS, rest);
        assert_eq!(
            first.burned + second.burned,
            whole.burned,
            "T3: the base part must be burned"
        );
        assert_eq!(
            first.tips + second.tips,
            whole.tips,
            "T3: the base part must be burned"
        );
        assert_eq!(
            first.paid + second.paid,
            whole.paid,
            "T3: the base part must be burned"
        );
    }
}

#[test]
fn t3_higher_demand_yields_a_higher_base_fee() {
    let mut high = T3_INITIAL_BASE_FEE;
    let mut mid = T3_INITIAL_BASE_FEE;
    let mut low = T3_INITIAL_BASE_FEE;
    for _ in 0..64 {
        high = next_base_fee(high, T3_GAS_TARGET + T3_GAS_TARGET / 4, T3_GAS_TARGET)
            .expect("the gas target is positive");
        mid = next_base_fee(mid, T3_GAS_TARGET, T3_GAS_TARGET).expect("the gas target is positive");
        low = next_base_fee(low, T3_GAS_TARGET - T3_GAS_TARGET / 4, T3_GAS_TARGET)
            .expect("the gas target is positive");
    }
    assert!(
        high > mid,
        "T3: congestion must raise the base fee and slack must lower it"
    );
    assert!(
        mid > low,
        "T3: congestion must raise the base fee and slack must lower it"
    );
}
