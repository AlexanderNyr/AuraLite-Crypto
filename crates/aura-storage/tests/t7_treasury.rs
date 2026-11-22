//! T7 gate tests: every payout has proposal + tx + report; a
//! withdrawal without a vote is red; the multisig threshold, the
//! overdraw guard and the double-execution guard hold (ECONOMICS_PLAN
//! T7: "every payout has proposal + tx + report; NC: withdrawal without
//! vote -> red").

use aura_storage::econ::Amount;
use aura_storage::issuance::ALCRY;
use aura_storage::treasury::{ProposalState, T7Error, Treasury, run_t7_scenario, scenario_digest};

const GOLDEN_TREASURY_DIGEST: &str =
    "719c93b244ff42ebf94538a5e9fb5b9e8f52eac07035abaf3c526e4c0a3231f7";

fn passed(treasury: &mut Treasury, amount: Amount, recipient: u32, purpose: &str) -> u32 {
    let id = treasury.propose(amount, recipient, purpose);
    treasury.vote(id, 0, true).expect("open vote");
    treasury.vote(id, 1, true).expect("open vote");
    treasury.vote(id, 2, true).expect("open vote");
    treasury.finalize(id).expect("voting open");
    id
}

#[test]
fn t7_every_payout_has_a_proposal_a_tx_and_a_report() {
    let run = run_t7_scenario();
    assert_eq!(
        run.payouts.len(),
        6,
        "T7: every payout must have a proposal, a tx and a report"
    );
    for (index, payout) in run.payouts.iter().enumerate() {
        let tx = (index + 1) as u32;
        assert_eq!(
            payout.tx_id, tx,
            "T7: every payout must have a proposal, a tx and a report"
        );
        let report = payout
            .report
            .as_ref()
            .expect("T7: every payout must have a proposal, a tx and a report");
        assert_eq!(
            report.tx_id, payout.tx_id,
            "T7: every payout must have a proposal, a tx and a report"
        );
        assert_eq!(
            report.proposal_id, payout.proposal_id,
            "T7: every payout must have a proposal, a tx and a report"
        );
        assert_eq!(
            report.amount, payout.amount,
            "T7: every payout must have a proposal, a tx and a report"
        );
        assert_eq!(
            report.recipient, payout.recipient,
            "T7: every payout must have a proposal, a tx and a report"
        );
        assert_eq!(
            report.purpose, payout.purpose,
            "T7: every payout must have a proposal, a tx and a report"
        );
        assert_eq!(
            run.states[payout.proposal_id as usize - 1],
            ProposalState::Executed as u8,
            "T7: every payout must have a proposal, a tx and a report"
        );
    }
}

#[test]
fn t7_a_withdrawal_without_a_vote_must_be_rejected() {
    let mut treasury = Treasury::new();
    let id = treasury.propose(30_000 * ALCRY, 3, "ops budget");
    assert_eq!(
        treasury.execute(id, 310_000),
        Err(T7Error::NoVote),
        "T7: a withdrawal without a vote must be rejected"
    );
    assert!(
        treasury.payouts().is_empty(),
        "T7: a withdrawal without a vote must be rejected"
    );
}

#[test]
fn t7_the_multisig_threshold_must_be_enforced() {
    let mut treasury = Treasury::new();
    let id = passed(&mut treasury, 80_000 * ALCRY, 4, "security audit");
    treasury.approve(id, 0).expect("first approval");
    assert_eq!(
        treasury.execute(id, 440_000),
        Err(T7Error::NotEnoughApprovals),
        "T7: the multisig threshold must be enforced"
    );
    treasury.approve(id, 2).expect("second approval");
    assert_eq!(
        treasury.execute(id, 460_000),
        Ok(1),
        "T7: the multisig threshold must be enforced"
    );
}

#[test]
fn t7_the_treasury_must_not_overdraw() {
    let mut treasury = Treasury::new();
    let id = passed(&mut treasury, 600_000 * ALCRY, 5, "ecosystem fund");
    treasury.approve(id, 0).expect("first approval");
    treasury.approve(id, 1).expect("second approval");
    assert_eq!(
        treasury.execute(id, 540_000),
        Err(T7Error::InsufficientFunds),
        "T7: the treasury must not overdraw"
    );
    assert!(
        treasury.balance() >= 0,
        "T7: the treasury must not overdraw"
    );
    // conservation over the pinned run
    let run = run_t7_scenario();
    assert!(run.final_balance >= 0, "T7: the treasury must not overdraw");
    assert_eq!(
        run.final_balance as Amount + run.total_paid,
        500_000 * ALCRY + run.total_inflows,
        "T7: the treasury must not overdraw"
    );
}

#[test]
fn t7_a_payout_must_not_execute_twice() {
    let mut treasury = Treasury::new();
    let id = passed(&mut treasury, 20_000 * ALCRY, 6, "grant tranche 1");
    treasury.approve(id, 0).expect("first approval");
    treasury.approve(id, 1).expect("second approval");
    treasury.execute(id, 640_000).expect("first execution");
    assert_eq!(
        treasury.execute(id, 641_000),
        Err(T7Error::AlreadyExecuted),
        "T7: a payout must not execute twice"
    );
    assert_eq!(
        treasury.payouts().len(),
        1,
        "T7: a payout must not execute twice"
    );
}

#[test]
fn t7_a_rejected_proposal_must_not_pay_out() {
    let mut treasury = Treasury::new();
    let id = treasury.propose(50_000 * ALCRY, 2, "docs translation");
    treasury.vote(id, 0, true).expect("open vote");
    treasury.vote(id, 1, false).expect("open vote");
    treasury.vote(id, 2, false).expect("open vote");
    treasury.finalize(id).expect("voting open");
    assert_eq!(
        treasury.proposal(id).state,
        ProposalState::Rejected,
        "T7: a rejected proposal must not pay out"
    );
    assert_eq!(
        treasury.execute(id, 230_000),
        Err(T7Error::Rejected),
        "T7: a rejected proposal must not pay out"
    );
    assert!(
        treasury.payouts().is_empty(),
        "T7: a rejected proposal must not pay out"
    );
}

#[test]
fn t7_reference_simulation_agrees_with_the_pinned_digest() {
    let run = run_t7_scenario();
    assert_eq!(
        run.steps.len(),
        82,
        "T7: the reference simulation must agree with the pinned digest"
    );
    let digest = scenario_digest(&run.steps);
    assert_eq!(
        digest, GOLDEN_TREASURY_DIGEST,
        "T7: the reference simulation must agree with the pinned digest"
    );
    let s0 = run.steps[0];
    assert_eq!(
        (s0.kind, s0.subject, s0.a, s0.b, s0.state),
        (0u8, 1u32, 100_000 * ALCRY, 1, 0u8),
        "T7: the reference simulation must agree with the pinned digest"
    );
    let s9 = run.steps[9];
    assert_eq!(
        (s9.kind, s9.subject, s9.a, s9.state),
        (4u8, 1u32, 1, 3u8),
        "T7: the reference simulation must agree with the pinned digest"
    );
    let s19 = run.steps[19];
    assert_eq!(
        (s19.kind, s19.subject, s19.detail),
        (8u8, 3u32, 1u32),
        "T7: a withdrawal without a vote must be rejected"
    );
    let s81 = run.steps[81];
    assert_eq!(
        (s81.kind, s81.a, s81.b, s81.c, s81.state),
        (9u8, 225_000 * ALCRY, 6, 475_000 * ALCRY, 9u8),
        "T7: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        (run.final_balance as Amount, run.total_paid),
        (225_000 * ALCRY, 475_000 * ALCRY),
        "T7: the treasury must not overdraw"
    );
}

#[test]
fn t7_reports_match_their_payouts() {
    let run = run_t7_scenario();
    for payout in &run.payouts {
        let report = payout
            .report
            .as_ref()
            .expect("T7: every payout must have a proposal, a tx and a report");
        assert_eq!(
            (
                report.tx_id,
                report.proposal_id,
                report.amount,
                report.recipient
            ),
            (
                payout.tx_id,
                payout.proposal_id,
                payout.amount,
                payout.recipient
            ),
            "T7: every payout must have a proposal, a tx and a report"
        );
    }
}

#[test]
fn t7_reordering_votes_preserves_the_tally() {
    let mut forward = Treasury::new();
    let a = forward.propose(10_000 * ALCRY, 1, "x");
    forward.vote(a, 0, true).expect("open");
    forward.vote(a, 1, false).expect("open");
    let mut reverse = Treasury::new();
    let b = reverse.propose(10_000 * ALCRY, 1, "x");
    reverse.vote(b, 1, false).expect("open");
    reverse.vote(b, 0, true).expect("open");
    assert_eq!(
        (forward.proposal(a).yes, forward.proposal(a).no),
        (reverse.proposal(b).yes, reverse.proposal(b).no),
        "T7: a rejected proposal must not pay out"
    );
}

#[test]
fn t7_splitting_a_payout_preserves_the_balance() {
    let mut whole = Treasury::new();
    let id = passed(&mut whole, 100_000 * ALCRY, 1, "whole");
    whole.approve(id, 0).expect("first");
    whole.approve(id, 1).expect("second");
    whole.execute(id, 1).expect("paid");
    let mut halves = Treasury::new();
    for purpose in ["half a", "half b"] {
        let id = passed(&mut halves, 50_000 * ALCRY, 1, purpose);
        halves.approve(id, 0).expect("first");
        halves.approve(id, 1).expect("second");
        halves.execute(id, 1).expect("paid");
    }
    assert_eq!(
        whole.balance(),
        halves.balance(),
        "T7: the treasury must not overdraw"
    );
}
