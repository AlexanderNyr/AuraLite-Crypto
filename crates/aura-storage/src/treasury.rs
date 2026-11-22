//! T7 treasury and grants: an on-chain treasury with an m-of-n multisig
//! (W4), governance votes for payouts, transparent reports and a grants
//! process (ECONOMICS_PLAN T7).
//!
//! Every payout must have **proposal + tx + report**: a payout proposal
//! is voted by the council ([`T7_COUNCIL`] members, strict majority),
//! signed by the multisig ([`T7_SIGNER_THRESHOLD`] of [`T7_SIGNERS`],
//! decision W4) and only then executed — producing a numbered **tx**
//! and a transparent [`Report`] that mirrors the payout. A withdrawal
//! without a passed vote is rejected ([`T7Error::NoVote`]); the treasury
//! cannot overdraw (the balance is a signed `i128` so an overdraw would
//! be visible); a payout cannot execute twice; a rejected proposal never
//! pays out.
//!
//! The pinned scenario [`run_t7_scenario`] drives 10 grant proposals
//! through the full process (6 payouts with tx+report, 2 rejections, 1
//! withdrawal-without-vote attempt, and the failure modes of the
//! multisig, the funds and the double-execution); [`scenario_digest`] is
//! cross-run against the independent Python oracle
//! (`tools/t7_treasury_check.py`).

use crate::econ::Amount;
use crate::issuance::ALCRY;

/// Council members voting on payout proposals.
pub const T7_COUNCIL: u32 = 5;

/// Multisig signers (decision W4).
pub const T7_SIGNERS: u32 = 3;

/// Multisig threshold: approvals required to execute a payout.
pub const T7_SIGNER_THRESHOLD: u32 = 2;

/// Initial treasury balance.
pub const T7_INITIAL_BALANCE: Amount = 500_000 * ALCRY;

/// Treasury errors (numeric codes appear in the step log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum T7Error {
    /// Execute without a passed vote (the literal gate control).
    NoVote = 1,
    /// Action on a rejected proposal.
    Rejected = 2,
    /// Execute on an already executed proposal.
    AlreadyExecuted = 3,
    /// Fewer than [`T7_SIGNER_THRESHOLD`] multisig approvals.
    NotEnoughApprovals = 4,
    /// Payout larger than the treasury balance.
    InsufficientFunds = 5,
    /// This council member has already voted.
    AlreadyVoted = 6,
    /// This signer has already approved.
    AlreadyApproved = 7,
    /// Vote or finalize on a non-voting proposal.
    NotVoting = 8,
    /// Approve on a proposal that did not pass.
    NotPassed = 9,
    /// Unknown proposal id.
    NoSuchProposal = 10,
}

/// Payout proposal state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// Council voting is open.
    Voting = 0,
    /// Majority reached; waiting for the multisig.
    Passed = 1,
    /// Majority not reached.
    Rejected = 2,
    /// Paid out (tx + report created).
    Executed = 3,
}

/// One payout proposal with its vote tally and multisig approvals.
#[derive(Debug, Clone)]
pub struct TxProposal {
    /// Proposal id, counting from 1.
    pub id: u32,
    /// Amount to pay out.
    pub amount: Amount,
    /// Recipient id.
    pub recipient: u32,
    /// Purpose of the grant (free text).
    pub purpose: String,
    /// Council votes in favour.
    pub yes: u32,
    /// Council votes against.
    pub no: u32,
    /// Per-member "has voted" marks.
    pub voted: Vec<bool>,
    /// Per-signer multisig approvals.
    pub approvals: Vec<bool>,
    /// Current state.
    pub state: ProposalState,
}

/// Transparent report mirroring one payout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Tx number of the payout.
    pub tx_id: u32,
    /// Authorizing proposal id.
    pub proposal_id: u32,
    /// Amount paid.
    pub amount: Amount,
    /// Recipient id.
    pub recipient: u32,
    /// Purpose of the grant.
    pub purpose: String,
    /// Block height of the payout.
    pub height: u64,
}

/// One executed payout with its tx number and report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payout {
    /// Tx number, counting from 1.
    pub tx_id: u32,
    /// Authorizing proposal id.
    pub proposal_id: u32,
    /// Amount paid.
    pub amount: Amount,
    /// Recipient id.
    pub recipient: u32,
    /// Purpose of the grant.
    pub purpose: String,
    /// The transparent report (`None` only under mutation).
    pub report: Option<Report>,
}

/// On-chain treasury with council votes and a multisig.
#[derive(Debug, Clone)]
pub struct Treasury {
    balance: i128,
    proposals: Vec<TxProposal>,
    payouts: Vec<Payout>,
}

impl Treasury {
    /// New treasury seeded with [`T7_INITIAL_BALANCE`].
    pub fn new() -> Self {
        Self {
            balance: T7_INITIAL_BALANCE as i128,
            proposals: Vec::new(),
            payouts: Vec::new(),
        }
    }

    /// Current balance (signed so an overdraw would be visible).
    pub fn balance(&self) -> i128 {
        self.balance
    }

    /// All payout proposals in creation order.
    pub fn proposals(&self) -> &[TxProposal] {
        &self.proposals
    }

    /// One proposal by id.
    pub fn proposal(&self, id: u32) -> &TxProposal {
        &self.proposals[id as usize - 1]
    }

    /// All executed payouts in tx order.
    pub fn payouts(&self) -> &[Payout] {
        &self.payouts
    }

    /// Top up the treasury (transparent inflow).
    pub fn fund(&mut self, amount: Amount) {
        self.balance += amount as i128;
    }

    /// Create a payout proposal.
    pub fn propose(&mut self, amount: Amount, recipient: u32, purpose: &str) -> u32 {
        let id = self.proposals.len() as u32 + 1;
        self.proposals.push(TxProposal {
            id,
            amount,
            recipient,
            purpose: purpose.to_string(),
            yes: 0,
            no: 0,
            voted: vec![false; T7_COUNCIL as usize],
            approvals: vec![false; T7_SIGNERS as usize],
            state: ProposalState::Voting,
        });
        id
    }

    /// Cast one council vote.
    pub fn vote(&mut self, proposal_id: u32, voter: u32, yes: bool) -> Result<(), T7Error> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if proposal.state != ProposalState::Voting {
            return Err(T7Error::NotVoting);
        }
        if proposal.voted[voter as usize] {
            return Err(T7Error::AlreadyVoted);
        }
        proposal.voted[voter as usize] = true;
        if yes {
            proposal.yes += 1;
        } else {
            proposal.no += 1;
        }
        Ok(())
    }

    /// Close the council vote: strict majority of [`T7_COUNCIL`] passes.
    pub fn finalize(&mut self, proposal_id: u32) -> Result<(), T7Error> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if proposal.state != ProposalState::Voting {
            return Err(T7Error::NotVoting);
        }
        proposal.state = if proposal.yes > T7_COUNCIL / 2 {
            ProposalState::Passed
        } else {
            ProposalState::Rejected
        };
        Ok(())
    }

    /// Multisig approval of a passed proposal (W4).
    pub fn approve(&mut self, proposal_id: u32, signer: u32) -> Result<(), T7Error> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if proposal.state != ProposalState::Passed {
            return Err(T7Error::NotPassed);
        }
        if proposal.approvals[signer as usize] {
            return Err(T7Error::AlreadyApproved);
        }
        proposal.approvals[signer as usize] = true;
        Ok(())
    }

    /// Execute a passed, multisig-approved payout: tx + report.
    pub fn execute(&mut self, proposal_id: u32, height: u64) -> Result<u32, T7Error> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if proposal.state == ProposalState::Executed {
            return Err(T7Error::AlreadyExecuted);
        }
        if proposal.state == ProposalState::Rejected {
            return Err(T7Error::Rejected);
        }
        if proposal.state == ProposalState::Voting {
            return Err(T7Error::NoVote);
        }
        let approvals = proposal.approvals.iter().filter(|&&a| a).count();
        if approvals < T7_SIGNER_THRESHOLD as usize {
            return Err(T7Error::NotEnoughApprovals);
        }
        if self.balance < proposal.amount as i128 {
            return Err(T7Error::InsufficientFunds);
        }
        self.balance -= proposal.amount as i128;
        let tx_id = self.payouts.len() as u32 + 1;
        proposal.state = ProposalState::Executed;
        let report = Report {
            tx_id,
            proposal_id,
            amount: proposal.amount,
            recipient: proposal.recipient,
            purpose: proposal.purpose.clone(),
            height,
        };
        self.payouts.push(Payout {
            tx_id,
            proposal_id,
            amount: proposal.amount,
            recipient: proposal.recipient,
            purpose: proposal.purpose.clone(),
            report: Some(report),
        });
        Ok(tx_id)
    }
}

impl Default for Treasury {
    fn default() -> Self {
        Self::new()
    }
}

/// One action of the pinned scenario run (digest step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct T7Step {
    /// Step number, starting at 0.
    pub index: u32,
    /// 0 propose, 1 vote, 2 finalize, 3 approve, 4 execute, 5 fund,
    /// 8 failed action, 9 final step.
    pub kind: u8,
    /// Proposal id (0 for fund/final).
    pub subject: u32,
    /// Block height of the action.
    pub height: u64,
    /// kind-specific: propose amount / vote yes / approve count / tx id
    /// / fund amount / final balance.
    pub a: Amount,
    /// kind-specific: propose recipient / vote no / execute amount /
    /// fund balance / final payout count.
    pub b: Amount,
    /// kind-specific: final total paid.
    pub c: Amount,
    /// Proposal state after the action (9 = n/a).
    pub state: u8,
    /// Error code for failed actions (0 otherwise).
    pub detail: u32,
}

/// Result of the pinned scenario run.
#[derive(Debug, Clone)]
pub struct T7Run {
    /// Digest steps of the run.
    pub steps: Vec<T7Step>,
    /// Final state of every created proposal, in id order.
    pub states: Vec<u8>,
    /// All executed payouts in tx order.
    pub payouts: Vec<Payout>,
    /// Final treasury balance.
    pub final_balance: i128,
    /// Sum of all payouts.
    pub total_paid: Amount,
    /// Sum of all inflows.
    pub total_inflows: Amount,
}

fn state_code(state: ProposalState) -> u8 {
    state as u8
}

/// The pinned T7 scenario: 10 grant proposals, every payout carries
/// proposal + tx + report, a withdrawal without a vote is rejected.
pub fn run_t7_scenario() -> T7Run {
    let mut treasury = Treasury::new();
    let mut steps: Vec<T7Step> = Vec::new();

    macro_rules! snap {
        ($kind:expr, $subject:expr, $height:expr, $a:expr, $b:expr, $c:expr, $state:expr, $detail:expr) => {
            steps.push(T7Step {
                index: steps.len() as u32,
                kind: $kind,
                subject: $subject,
                height: $height,
                a: $a,
                b: $b,
                c: $c,
                state: $state,
                detail: $detail,
            })
        };
    }
    macro_rules! do_propose {
        ($amount:expr, $recipient:expr, $purpose:expr, $height:expr) => {{
            let id = treasury.propose($amount, $recipient, $purpose);
            let p = treasury.proposal(id);
            snap!(
                0,
                id,
                $height,
                p.amount,
                Amount::from(p.recipient),
                0,
                state_code(p.state),
                0
            );
        }};
    }
    macro_rules! do_vote {
        ($id:expr, $voter:expr, $yes:expr, $height:expr) => {{
            match treasury.vote($id, $voter, $yes) {
                Ok(()) => {
                    let p = treasury.proposal($id);
                    snap!(
                        1,
                        $id,
                        $height,
                        Amount::from(p.yes),
                        Amount::from(p.no),
                        0,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = treasury.proposal($id);
                    snap!(
                        8,
                        $id,
                        $height,
                        Amount::from(p.yes),
                        Amount::from(p.no),
                        0,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }
    macro_rules! do_finalize {
        ($id:expr, $height:expr) => {{
            match treasury.finalize($id) {
                Ok(()) => {
                    let p = treasury.proposal($id);
                    snap!(
                        2,
                        $id,
                        $height,
                        Amount::from(p.yes),
                        Amount::from(p.no),
                        0,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = treasury.proposal($id);
                    snap!(
                        8,
                        $id,
                        $height,
                        Amount::from(p.yes),
                        Amount::from(p.no),
                        0,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }
    macro_rules! do_approve {
        ($id:expr, $signer:expr, $height:expr) => {{
            match treasury.approve($id, $signer) {
                Ok(()) => {
                    let p = treasury.proposal($id);
                    let count = p.approvals.iter().filter(|&&a| a).count();
                    snap!(
                        3,
                        $id,
                        $height,
                        count as Amount,
                        0,
                        0,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = treasury.proposal($id);
                    let count = p.approvals.iter().filter(|&&a| a).count();
                    snap!(
                        8,
                        $id,
                        $height,
                        count as Amount,
                        0,
                        0,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }
    macro_rules! do_execute {
        ($id:expr, $height:expr) => {{
            match treasury.execute($id, $height) {
                Ok(tx) => {
                    let p = treasury.proposal($id);
                    snap!(
                        4,
                        $id,
                        $height,
                        Amount::from(tx),
                        p.amount,
                        0,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = treasury.proposal($id);
                    snap!(
                        8,
                        $id,
                        $height,
                        0,
                        p.amount,
                        0,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }

    // P1: valid grant, full lifecycle.
    do_propose!(100_000 * ALCRY, 1, "validator tooling grant", 100_000);
    do_vote!(1, 0, true, 110_000);
    do_vote!(1, 1, true, 111_000);
    do_vote!(1, 2, true, 112_000);
    do_vote!(1, 3, false, 113_000);
    do_vote!(1, 4, false, 114_000);
    do_finalize!(1, 120_000);
    do_approve!(1, 0, 130_000);
    do_approve!(1, 1, 131_000);
    do_execute!(1, 140_000);
    // P2: rejected by the council.
    do_propose!(50_000 * ALCRY, 2, "docs translation", 200_000);
    do_vote!(2, 0, true, 210_000);
    do_vote!(2, 1, false, 211_000);
    do_vote!(2, 2, false, 212_000);
    do_vote!(2, 3, false, 213_000);
    do_vote!(2, 4, false, 214_000);
    do_finalize!(2, 220_000);
    do_execute!(2, 230_000);
    // P3: withdrawal without a vote — rejected (the literal gate).
    do_propose!(30_000 * ALCRY, 3, "ops budget", 300_000);
    do_execute!(3, 310_000);
    // P4: multisig threshold, then valid payout.
    do_propose!(80_000 * ALCRY, 4, "security audit", 400_000);
    do_vote!(4, 0, true, 410_000);
    do_vote!(4, 1, true, 411_000);
    do_vote!(4, 2, true, 412_000);
    do_vote!(4, 3, false, 413_000);
    do_vote!(4, 4, false, 414_000);
    do_finalize!(4, 420_000);
    do_approve!(4, 0, 430_000);
    do_execute!(4, 440_000);
    do_approve!(4, 2, 450_000);
    do_execute!(4, 460_000);
    // P5: insufficient funds.
    do_propose!(600_000 * ALCRY, 5, "ecosystem fund", 500_000);
    do_vote!(5, 0, true, 510_000);
    do_vote!(5, 1, true, 511_000);
    do_vote!(5, 2, true, 512_000);
    do_finalize!(5, 520_000);
    do_approve!(5, 0, 530_000);
    do_approve!(5, 1, 531_000);
    do_execute!(5, 540_000);
    // P6: double execution.
    do_propose!(20_000 * ALCRY, 6, "grant tranche 1", 600_000);
    do_vote!(6, 0, true, 610_000);
    do_vote!(6, 1, true, 611_000);
    do_vote!(6, 2, true, 612_000);
    do_finalize!(6, 620_000);
    do_approve!(6, 0, 630_000);
    do_approve!(6, 1, 631_000);
    do_execute!(6, 640_000);
    do_execute!(6, 641_000);
    // P7: double vote.
    do_propose!(10_000 * ALCRY, 7, "audit reserve", 700_000);
    do_vote!(7, 0, true, 710_000);
    do_vote!(7, 0, false, 711_000);
    do_vote!(7, 1, true, 712_000);
    do_vote!(7, 2, true, 713_000);
    do_finalize!(7, 720_000);
    do_approve!(7, 0, 730_000);
    do_approve!(7, 1, 731_000);
    do_execute!(7, 740_000);
    // P8: double approval.
    do_propose!(15_000 * ALCRY, 8, "docs", 800_000);
    do_vote!(8, 0, true, 810_000);
    do_vote!(8, 1, true, 811_000);
    do_vote!(8, 2, true, 812_000);
    do_finalize!(8, 820_000);
    do_approve!(8, 0, 830_000);
    do_approve!(8, 0, 831_000);
    do_approve!(8, 1, 832_000);
    do_execute!(8, 840_000);
    // P9: inflow then a large grant round.
    treasury.fund(200_000 * ALCRY);
    snap!(
        5,
        0,
        905_000,
        200_000 * ALCRY,
        treasury.balance() as Amount,
        0,
        9,
        0
    );
    do_propose!(250_000 * ALCRY, 9, "ecosystem round", 910_000);
    do_vote!(9, 0, true, 920_000);
    do_vote!(9, 1, true, 921_000);
    do_vote!(9, 2, true, 922_000);
    do_finalize!(9, 930_000);
    do_approve!(9, 0, 940_000);
    do_approve!(9, 1, 941_000);
    do_execute!(9, 950_000);
    // P10: council tie -> rejected.
    do_propose!(10_000 * ALCRY, 10, "tie grant", 1_000_000);
    do_vote!(10, 0, true, 1_010_000);
    do_vote!(10, 1, true, 1_011_000);
    do_vote!(10, 2, false, 1_012_000);
    do_vote!(10, 3, false, 1_013_000);
    do_finalize!(10, 1_020_000);

    let states: Vec<u8> = treasury
        .proposals()
        .iter()
        .map(|p| state_code(p.state))
        .collect();
    let total_paid: Amount = treasury.payouts().iter().map(|p| p.amount).sum();
    snap!(
        9,
        0,
        0,
        treasury.balance() as Amount,
        treasury.payouts().len() as Amount,
        total_paid,
        9,
        0
    );
    T7Run {
        final_balance: treasury.balance(),
        total_paid,
        total_inflows: 200_000 * ALCRY,
        payouts: treasury.payouts().to_vec(),
        states,
        steps,
    }
}

/// Digest of the scenario step log (sha256, hex).
pub fn scenario_digest(steps: &[T7Step]) -> String {
    let mut blob = String::new();
    for step in steps {
        blob.push_str(&format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}\n",
            step.index,
            step.kind,
            step.subject,
            step.height,
            step.a,
            step.b,
            step.c,
            step.state,
            step.detail
        ));
    }
    aura_state::sha256_bytes(blob.as_bytes()).to_hex()
}
