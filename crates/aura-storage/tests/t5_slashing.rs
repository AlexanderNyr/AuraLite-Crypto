//! T5 gate tests: seven scenarios with the invariants "penalty >=
//! damage" and "the insurance fund must never go negative", the
//! mass-slash cap, deferred penalties, pinned digest (ECONOMICS_PLAN
//! T5: "7 scenarios: penalty >= damage, fund never negative").

use aura_storage::econ::Amount;
use aura_storage::issuance::ALCRY;
use aura_storage::slashing::{
    InsuranceFund, T5_SCENARIO_NAMES, T5_SCENARIOS, run_t5_scenario, scenario_digest,
};

/// Pinned mass-slash cap: 5 % of the registered bonded base
/// (1 000 000 ALCRY). Hard-coded on purpose: the tests re-derive the
/// invariants from the pinned parameters, not from the source consts.
const PINNED_WINDOW_CAP: Amount = 50_000 * ALCRY;

const GOLDEN_STAKING_DIGEST: &str =
    "4cc9ea0319374c3c2c74bc88db6d7747b23f0e21ea637ae2270bc4c5707c9cef";

#[test]
fn t5_seven_scenarios_hold_both_invariants() {
    assert_eq!(T5_SCENARIOS, 7, "T5: seven scenarios must run");
    assert_eq!(
        T5_SCENARIO_NAMES.len() as u32,
        7,
        "T5: seven scenarios must run"
    );
    let run = run_t5_scenario();
    for id in 1..=7u32 {
        assert!(
            run.steps.iter().any(|s| s.scenario == id),
            "T5: seven scenarios must run (missing scenario {})",
            id
        );
    }
    for record in &run.records {
        assert!(
            record.penalty >= record.damage,
            "T5: the penalty must cover the damage"
        );
    }
    for step in &run.steps {
        assert!(
            step.balance >= 0,
            "T5: the insurance fund must never go negative"
        );
    }
}

#[test]
fn t5_the_penalty_must_cover_the_damage() {
    let mut fund = InsuranceFund::new();
    let record = *fund.report(100 * ALCRY, 0);
    assert_eq!(
        record.penalty,
        150 * ALCRY,
        "T5: the penalty must cover the damage"
    );
    assert!(
        record.penalty >= record.damage,
        "T5: the penalty must cover the damage"
    );
}

#[test]
fn t5_the_insurance_fund_must_never_go_negative() {
    let mut fund = InsuranceFund::new();
    // damage larger than fund + income: the coverage is clamped and the
    // remainder becomes a shortfall; the balance bottoms at zero.
    let record = *fund.report(300_000 * ALCRY, 0);
    assert_eq!(
        record.covered,
        225_000 * ALCRY,
        "T5: the insurance fund must never go negative"
    );
    assert_eq!(
        record.shortfall,
        75_000 * ALCRY,
        "T5: the insurance fund must never go negative"
    );
    assert!(
        fund.balance() >= 0,
        "T5: the insurance fund must never go negative"
    );
    assert_eq!(
        fund.balance(),
        0,
        "T5: the insurance fund must never go negative"
    );
}

#[test]
fn t5_the_mass_slash_cap_must_hold_in_every_window() {
    let run = run_t5_scenario();
    for step in &run.steps {
        assert!(
            step.window_applied <= PINNED_WINDOW_CAP,
            "T5: the mass-slash cap must hold in every window"
        );
    }
}

#[test]
fn t5_deferred_penalties_are_not_dropped() {
    let run = run_t5_scenario();
    assert_eq!(run.pending, 0, "T5: deferred penalties must not be dropped");
    assert_eq!(
        run.total_applied, run.total_penalty,
        "T5: deferred penalties must not be dropped"
    );
}

#[test]
fn t5_the_penalty_is_exactly_the_damage_times_the_pinned_parameter() {
    let run = run_t5_scenario();
    for record in &run.records {
        assert_eq!(
            record.penalty,
            record.damage * 15_000 / 10_000,
            "T5: the penalty must be exactly damage times the pinned parameter"
        );
    }
    // floor cases of scenario 5
    let margins: Vec<(Amount, Amount)> = run
        .records
        .iter()
        .filter(|r| r.scenario == 5)
        .map(|r| (r.damage, r.penalty))
        .collect();
    assert_eq!(
        margins,
        vec![
            (1_000_000_000_000_000_001, 1_500_000_000_000_000_001),
            (999_999_999_999_999_999, 1_499_999_999_999_999_998),
        ],
        "T5: the penalty must be exactly damage times the pinned parameter"
    );
}

#[test]
fn t5_the_insurance_income_is_the_fund_share_of_the_applied_penalty() {
    let run = run_t5_scenario();
    for record in &run.records {
        assert_eq!(
            record.income,
            record.applied * 5_000 / 10_000,
            "T5: the insurance income must be exactly the fund share of the applied penalty"
        );
    }
    assert_eq!(
        run.total_covered + run.total_shortfall,
        run.records.iter().map(|r| r.damage).sum::<Amount>(),
        "T5: the insurance income must be exactly the fund share of the applied penalty"
    );
}

#[test]
fn t5_reference_simulation_agrees_with_the_pinned_digest() {
    let run = run_t5_scenario();
    assert_eq!(
        run.steps.len(),
        87,
        "T5: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        run.records.len(),
        48,
        "T5: the reference simulation must agree with the pinned digest"
    );
    let digest = scenario_digest(&run.steps);
    assert_eq!(
        digest, GOLDEN_STAKING_DIGEST,
        "T5: the reference simulation must agree with the pinned digest"
    );
    let step0 = run.steps[0];
    assert_eq!(
        (
            step0.kind,
            step0.scenario,
            step0.balance,
            step0.pending,
            step0.total_applied,
            step0.total_penalty
        ),
        (
            0u8,
            1u32,
            199_975_000_000_000_000_000_000i128,
            0,
            150_000_000_000_000_000_000,
            150_000_000_000_000_000_000
        ),
        "T5: the reference simulation must agree with the pinned digest"
    );
    let step31 = run.steps[31];
    assert_eq!(
        (
            step31.kind,
            step31.scenario,
            step31.balance,
            step31.pending,
            step31.total_applied,
            step31.total_penalty
        ),
        (
            1u8,
            3u32,
            149_400_000_000_000_000_000_000i128,
            50_900_000_000_000_000_000_000,
            100_000_000_000_000_000_000_000,
            150_900_000_000_000_000_000_000
        ),
        "T5: the reference simulation must agree with the pinned digest"
    );
    let step43 = run.steps[43];
    assert_eq!(
        (step43.kind, step43.scenario, step43.balance, step43.pending),
        (0u8, 5u32, 129_849_749_999_999_999_999_999i128, 0),
        "T5: the reference simulation must agree with the pinned digest"
    );
    let step60 = run.steps[60];
    assert_eq!(
        (step60.kind, step60.scenario, step60.balance, step60.pending),
        (0u8, 7u32, 0i128, 1_075_000_000_000_000_000_000_000),
        "T5: the reference simulation must agree with the pinned digest"
    );
    let final_step = run.steps[86];
    assert_eq!(
        final_step.kind, 9,
        "T5: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        (final_step.balance, final_step.pending),
        (536_750_000_000_000_000_000_000i128, 0),
        "T5: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        (run.total_applied, run.total_penalty),
        (
            1_790_402_999_999_999_999_999_999,
            1_790_402_999_999_999_999_999_999
        ),
        "T5: deferred penalties must not be dropped"
    );
    assert_eq!(
        run.total_covered, 558_451_499_999_999_999_999_998,
        "T5: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        run.total_shortfall, 635_150_500_000_000_000_000_002,
        "T5: the reference simulation must agree with the pinned digest"
    );
}

#[test]
fn t5_splitting_a_report_preserves_the_totals() {
    let mut whole = InsuranceFund::new();
    whole.report(100 * ALCRY, 0);
    let mut halves = InsuranceFund::new();
    halves.report(50 * ALCRY, 0);
    halves.report(50 * ALCRY, 0);
    assert_eq!(
        (
            whole.balance(),
            whole.total_penalty(),
            whole.total_covered()
        ),
        (
            halves.balance(),
            halves.total_penalty(),
            halves.total_covered()
        ),
        "T5: the penalty must be exactly damage times the pinned parameter"
    );
}

#[test]
fn t5_reordering_independent_reports_preserves_the_balance() {
    let mut forward = InsuranceFund::new();
    forward.report(100 * ALCRY, 0);
    forward.report(200 * ALCRY, 0);
    let mut reverse = InsuranceFund::new();
    reverse.report(200 * ALCRY, 0);
    reverse.report(100 * ALCRY, 0);
    assert_eq!(
        (
            forward.balance(),
            forward.total_penalty(),
            forward.total_covered()
        ),
        (
            reverse.balance(),
            reverse.total_penalty(),
            reverse.total_covered()
        ),
        "T5: the insurance fund must never go negative"
    );
}
