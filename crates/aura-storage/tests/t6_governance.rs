//! T6 gate tests: 20 proposals with the gate "valid ones pass,
//! flash-stake attack rejected", threshold/quorum/voting-period/
//! timelock/veto-sunset boundaries and the pinned digest
//! (ECONOMICS_PLAN T6).

use aura_storage::econ::Amount;
use aura_storage::governance::{GovError, Governance, Support, run_t6_scenario, scenario_digest};
use aura_storage::issuance::ALCRY;

const GOLDEN_GOV_DIGEST: &str = "828b13cf7ca3abcaea65386f7c62e1e155f4d207cb000aceccd86397b5c729aa";

fn stakes() -> Vec<Amount> {
    vec![
        80_000 * ALCRY,
        50_000 * ALCRY,
        40_000 * ALCRY,
        20_000 * ALCRY,
        10_000 * ALCRY,
        0,
    ]
}

#[test]
fn t6_twenty_proposals_valid_ones_pass() {
    let run = run_t6_scenario();
    assert_eq!(run.states.len(), 20, "T6: a valid proposal must pass");
    // Executed = valid proposals pass; Rejected/Vetoed as designed.
    assert_eq!(run.executed, 15, "T6: a valid proposal must pass");
    assert_eq!(run.rejected, 4, "T6: a valid proposal must pass");
    assert_eq!(run.vetoed, 1, "T6: a valid proposal must pass");
    let expected: Vec<u8> = vec![3, 2, 2, 3, 3, 3, 4, 3, 3, 3, 3, 3, 3, 3, 2, 3, 2, 3, 3, 3];
    assert_eq!(run.states, expected, "T6: a valid proposal must pass");
}

#[test]
fn t6_a_flash_stake_attack_must_be_rejected() {
    let mut gov = Governance::new(stakes());
    gov.propose(0, 1_800_000)
        .expect("proposer holds the threshold");
    gov.stake(5, 1_000_000 * ALCRY);
    assert_eq!(
        gov.vote(1, 5, Support::Yes, 1_850_001),
        Err(GovError::NoVotingPower),
        "T6: a flash-stake attack must be rejected"
    );
    assert_eq!(
        gov.proposal(1).yes,
        0,
        "T6: a flash-stake attack must be rejected"
    );
}

#[test]
fn t6_a_proposal_below_the_threshold_must_be_rejected() {
    let mut gov = Governance::new(stakes());
    assert_eq!(
        gov.propose(5, 800_000),
        Err(GovError::BelowThreshold),
        "T6: a proposal below the threshold must be rejected"
    );
    // boundary: the stake equals the threshold exactly
    assert_eq!(
        gov.propose(4, 3_200_000),
        Ok(1),
        "T6: a proposal below the threshold must be rejected"
    );
}

#[test]
fn t6_a_vote_outside_the_voting_period_must_be_rejected() {
    let mut gov = Governance::new(stakes());
    gov.propose(0, 1_000_000).expect("threshold met");
    assert_eq!(
        gov.vote(1, 2, Support::Yes, 1_120_960),
        Err(GovError::VotingClosed),
        "T6: a vote outside the voting period must be rejected"
    );
    // boundary: the last valid block of the window
    assert_eq!(
        gov.vote(1, 2, Support::Yes, 1_120_959),
        Ok(()),
        "T6: a vote outside the voting period must be rejected"
    );
}

#[test]
fn t6_the_quorum_must_be_met_for_a_proposal_to_pass() {
    // quorum = 20 % of 200 000 ALCRY = 40 000 ALCRY exactly
    let mut gov = Governance::new(stakes());
    gov.propose(0, 600_000).expect("threshold met");
    gov.vote(1, 3, Support::Yes, 610_000).expect("in window");
    gov.finalize(1, 721_000).expect("window closed");
    assert_eq!(
        gov.proposal(1).state as u8,
        2,
        "T6: the quorum must be met for a proposal to pass"
    );
    // boundary: voted == quorum exactly passes
    let mut gov = Governance::new(stakes());
    gov.propose(0, 2_400_000).expect("threshold met");
    gov.vote(1, 2, Support::Yes, 2_410_000).expect("in window");
    gov.finalize(1, 2_520_961).expect("window closed");
    assert_eq!(
        gov.proposal(1).state as u8,
        1,
        "T6: the quorum must be met for a proposal to pass"
    );
}

#[test]
fn t6_execution_must_wait_for_the_timelock() {
    let mut gov = Governance::new(stakes());
    gov.propose(0, 1_200_000).expect("threshold met");
    gov.vote(1, 0, Support::Yes, 1_210_000).expect("in window");
    gov.vote(1, 1, Support::Yes, 1_220_000).expect("in window");
    gov.finalize(1, 1_320_961).expect("window closed");
    // boundary: one block before the timelock elapses
    assert_eq!(
        gov.execute(1, 1_355_519),
        Err(GovError::Timelock),
        "T6: execution must wait for the timelock"
    );
    assert_eq!(
        gov.execute(1, 1_355_520),
        Ok(()),
        "T6: execution must wait for the timelock"
    );
}

#[test]
fn t6_the_veto_power_must_sunset() {
    let mut gov = Governance::new(stakes());
    gov.propose(0, 1_400_000).expect("threshold met");
    gov.vote(1, 0, Support::Yes, 1_410_000).expect("in window");
    gov.vote(1, 1, Support::Yes, 1_420_000).expect("in window");
    gov.finalize(1, 1_520_961).expect("window closed");
    // boundary: one block before the sunset
    assert_eq!(
        gov.veto(1, 1_520_999),
        Ok(()),
        "T6: the veto power must sunset"
    );
    assert_eq!(
        gov.proposal(1).state as u8,
        4,
        "T6: the veto power must sunset"
    );
    let mut gov = Governance::new(stakes());
    gov.propose(0, 1_600_000).expect("threshold met");
    gov.vote(1, 0, Support::Yes, 1_610_000).expect("in window");
    gov.vote(1, 1, Support::Yes, 1_620_000).expect("in window");
    gov.finalize(1, 1_720_961).expect("window closed");
    // at and after the sunset the veto is gone
    assert_eq!(
        gov.veto(1, 1_521_000),
        Err(GovError::VetoSunset),
        "T6: the veto power must sunset"
    );
}

#[test]
fn t6_reference_simulation_agrees_with_the_pinned_digest() {
    let run = run_t6_scenario();
    assert_eq!(
        run.steps.len(),
        113,
        "T6: the reference simulation must agree with the pinned digest"
    );
    let digest = scenario_digest(&run.steps);
    assert_eq!(
        digest, GOLDEN_GOV_DIGEST,
        "T6: the reference simulation must agree with the pinned digest"
    );
    let s0 = run.steps[0];
    assert_eq!(
        (
            s0.kind, s0.subject, s0.height, s0.a, s0.b, s0.c, s0.state, s0.detail
        ),
        (0u8, 1u32, 200_000u64, 0, 0, 0, 0u8, 0u32),
        "T6: the reference simulation must agree with the pinned digest"
    );
    let s4 = run.steps[4];
    assert_eq!(
        (s4.kind, s4.subject, s4.height, s4.a, s4.state, s4.detail),
        (2u8, 1u32, 321_000u64, 170_000 * ALCRY, 1u8, 0u32),
        "T6: the reference simulation must agree with the pinned digest"
    );
    let s37 = run.steps[37];
    assert_eq!(
        (s37.kind, s37.subject, s37.height, s37.state, s37.detail),
        (8u8, 7u32, 1_555_521u64, 4u8, 8u32),
        "T6: the reference simulation must agree with the pinned digest"
    );
    let s112 = run.steps[112];
    assert_eq!(
        (s112.kind, s112.a, s112.b, s112.c, s112.state),
        (9u8, 15, 4, 1, 9u8),
        "T6: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        (run.executed, run.rejected, run.vetoed),
        (15, 4, 1),
        "T6: a valid proposal must pass"
    );
}

#[test]
fn t6_reordering_independent_votes_preserves_the_tally() {
    let mut forward = Governance::new(stakes());
    forward.propose(0, 0).expect("threshold met");
    forward.vote(1, 0, Support::Yes, 10).expect("in window");
    forward.vote(1, 1, Support::No, 20).expect("in window");
    let mut reverse = Governance::new(stakes());
    reverse.propose(0, 0).expect("threshold met");
    reverse.vote(1, 1, Support::No, 10).expect("in window");
    reverse.vote(1, 0, Support::Yes, 20).expect("in window");
    let a = forward.proposal(1);
    let b = reverse.proposal(1);
    assert_eq!(
        (a.yes, a.no, a.abstain),
        (b.yes, b.no, b.abstain),
        "T6: a valid proposal must pass"
    );
}

#[test]
fn t6_a_late_stake_does_not_change_a_snapshot() {
    let mut gov = Governance::new(stakes());
    gov.propose(0, 2_800_000).expect("threshold met");
    gov.stake(3, 20_000 * ALCRY);
    gov.vote(1, 3, Support::Yes, 2_850_001).expect("in window");
    assert_eq!(
        gov.proposal(1).yes,
        20_000 * ALCRY,
        "T6: a flash-stake attack must be rejected"
    );
}
