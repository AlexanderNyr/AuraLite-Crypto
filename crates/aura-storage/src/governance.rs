//! T6 governance: proposal threshold, quorum, voting period, 48 h
//! timelock and a veto power with a sunset (ECONOMICS_PLAN T6).
//!
//! A proposal lifecycle: [`Governance::propose`] (the proposer must hold
//! at least [`T6_PROPOSAL_THRESHOLD`]) snapshots the voting power of
//! every voter; [`Governance::vote`] is open for
//! [`T6_VOTING_PERIOD_BLOCKS`] (7 days at 5 s) and counts the
//! **snapshot** power only — a flash-stake taken after the snapshot has
//! no voting power and its vote is rejected; [`Governance::finalize`]
//! applies the quorum ([`T6_QUORUM_BPS`] of the snapshot total) and the
//! strict majority `yes > no`; [`Governance::execute`] waits for the
//! **48 h timelock** ([`T6_TIMELOCK_BLOCKS`]) after the voting period
//! closes; [`Governance::veto`] is an emergency power that **sunsets**
//! at [`T6_VETO_SUNSET_BLOCK`] and can never be used afterwards.
//!
//! The pinned scenario [`run_t6_scenario`] drives 20 proposals — valid
//! ones pass, and the flash-stake attack is rejected; [`scenario_digest`]
//! is cross-run against the independent Python oracle
//! (`tools/t6_governance_check.py`).

use crate::econ::Amount;
use crate::issuance::ALCRY;

/// Proposal threshold: the proposer must hold at least this much stake.
pub const T6_PROPOSAL_THRESHOLD: Amount = 10_000 * ALCRY;

/// Quorum: minimum voted power (of the snapshot total), in basis points
/// (20 %).
pub const T6_QUORUM_BPS: u64 = 2_000;

/// Voting period: 7 days at 5 s per block.
pub const T6_VOTING_PERIOD_BLOCKS: u64 = 120_960;

/// Timelock between the voting close and execution: 48 hours at 5 s per
/// block.
pub const T6_TIMELOCK_BLOCKS: u64 = 34_560;

/// Block height at which the emergency veto power sunsets. After it the
/// veto can never be used again.
pub const T6_VETO_SUNSET_BLOCK: u64 = 1_521_000;

/// Basis points denominator.
pub const T6_BPS: u64 = 10_000;

/// Voting options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// In favour.
    Yes,
    /// Against.
    No,
    /// Present, counts for the quorum only.
    Abstain,
}

/// Proposal state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// Voting is open.
    Voting = 0,
    /// Quorum and majority reached; waiting for the timelock.
    Passed = 1,
    /// Quorum missed or majority against (or tie).
    Rejected = 2,
    /// Executed after the timelock.
    Executed = 3,
    /// Killed by the emergency veto before the sunset.
    Vetoed = 4,
}

/// Governance errors (numeric codes appear in the step log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovError {
    /// Proposer stake below [`T6_PROPOSAL_THRESHOLD`].
    BelowThreshold = 1,
    /// Vote outside `[created, created + voting period)`.
    VotingClosed = 2,
    /// Snapshot power is zero (a flash-stake attack).
    NoVotingPower = 3,
    /// This voter has already voted.
    AlreadyVoted = 4,
    /// Finalize before the voting period closed.
    StillVoting = 5,
    /// Finalize on an already final proposal.
    AlreadyFinalized = 6,
    /// Execute before the timelock elapsed.
    Timelock = 7,
    /// Execute or veto on a proposal that did not pass.
    NotPassed = 8,
    /// Execute on an already executed proposal.
    AlreadyExecuted = 9,
    /// Veto at or after [`T6_VETO_SUNSET_BLOCK`].
    VetoSunset = 10,
    /// Unknown proposal id.
    NoSuchProposal = 11,
}

/// One proposal with its power snapshot and tally.
#[derive(Debug, Clone)]
pub struct Proposal {
    /// Proposal id, counting from 1.
    pub id: u32,
    /// The proposer's voter id.
    pub proposer: u32,
    /// Block height the proposal was created (snapshot) at.
    pub created_at: u64,
    /// Voting power per voter, frozen at `created_at`.
    pub snapshot: Vec<Amount>,
    /// Tally: in favour.
    pub yes: Amount,
    /// Tally: against.
    pub no: Amount,
    /// Tally: abstain (quorum only).
    pub abstain: Amount,
    /// Per-voter "has voted" marks.
    pub voted: Vec<bool>,
    /// Current state.
    pub state: ProposalState,
}

/// Governance with a stake registry and proposals.
#[derive(Debug, Clone)]
pub struct Governance {
    stakes: Vec<Amount>,
    proposals: Vec<Proposal>,
}

impl Governance {
    /// New governance with the given stakes per voter id.
    pub fn new(stakes: Vec<Amount>) -> Self {
        Self {
            stakes,
            proposals: Vec::new(),
        }
    }

    /// Current stakes per voter id.
    pub fn stakes(&self) -> &[Amount] {
        &self.stakes
    }

    /// All proposals in creation order.
    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }

    /// One proposal by id.
    pub fn proposal(&self, id: u32) -> &Proposal {
        &self.proposals[id as usize - 1]
    }

    /// Create a proposal; the snapshot freezes the voting power.
    pub fn propose(&mut self, proposer: u32, height: u64) -> Result<u32, GovError> {
        if self.stakes[proposer as usize] < T6_PROPOSAL_THRESHOLD {
            return Err(GovError::BelowThreshold);
        }
        let id = self.proposals.len() as u32 + 1;
        self.proposals.push(Proposal {
            id,
            proposer,
            created_at: height,
            snapshot: self.stakes.clone(),
            yes: 0,
            no: 0,
            abstain: 0,
            voted: vec![false; self.stakes.len()],
            state: ProposalState::Voting,
        });
        Ok(id)
    }

    /// Cast one vote with the snapshot power of `voter`.
    pub fn vote(
        &mut self,
        proposal_id: u32,
        voter: u32,
        support: Support,
        height: u64,
    ) -> Result<(), GovError> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if height >= proposal.created_at + T6_VOTING_PERIOD_BLOCKS {
            return Err(GovError::VotingClosed);
        }
        let power = proposal.snapshot[voter as usize];
        if power == 0 {
            return Err(GovError::NoVotingPower);
        }
        if proposal.voted[voter as usize] {
            return Err(GovError::AlreadyVoted);
        }
        proposal.voted[voter as usize] = true;
        match support {
            Support::Yes => proposal.yes += power,
            Support::No => proposal.no += power,
            Support::Abstain => proposal.abstain += power,
        }
        Ok(())
    }

    /// Close the voting: quorum + strict majority decide.
    pub fn finalize(&mut self, proposal_id: u32, height: u64) -> Result<(), GovError> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if height < proposal.created_at + T6_VOTING_PERIOD_BLOCKS {
            return Err(GovError::StillVoting);
        }
        if proposal.state != ProposalState::Voting {
            return Err(GovError::AlreadyFinalized);
        }
        let voted = proposal.yes + proposal.no + proposal.abstain;
        let total: Amount = proposal.snapshot.iter().sum();
        let quorum = total * Amount::from(T6_QUORUM_BPS) / Amount::from(T6_BPS);
        let passed = voted >= quorum && proposal.yes > proposal.no;
        proposal.state = if passed {
            ProposalState::Passed
        } else {
            ProposalState::Rejected
        };
        Ok(())
    }

    /// Execute a passed proposal after the 48 h timelock.
    pub fn execute(&mut self, proposal_id: u32, height: u64) -> Result<(), GovError> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if proposal.state == ProposalState::Executed {
            return Err(GovError::AlreadyExecuted);
        }
        if proposal.state != ProposalState::Passed {
            return Err(GovError::NotPassed);
        }
        if height < proposal.created_at + T6_VOTING_PERIOD_BLOCKS + T6_TIMELOCK_BLOCKS {
            return Err(GovError::Timelock);
        }
        proposal.state = ProposalState::Executed;
        Ok(())
    }

    /// Emergency veto; sunsets at [`T6_VETO_SUNSET_BLOCK`].
    pub fn veto(&mut self, proposal_id: u32, height: u64) -> Result<(), GovError> {
        let proposal = &mut self.proposals[proposal_id as usize - 1];
        if proposal.state != ProposalState::Passed {
            return Err(GovError::NotPassed);
        }
        if height >= T6_VETO_SUNSET_BLOCK {
            return Err(GovError::VetoSunset);
        }
        proposal.state = ProposalState::Vetoed;
        Ok(())
    }

    /// Increase a voter's stake (the snapshot of existing proposals does
    /// not move).
    pub fn stake(&mut self, voter: u32, amount: Amount) {
        self.stakes[voter as usize] += amount;
    }

    /// Decrease a voter's stake.
    pub fn unstake(&mut self, voter: u32, amount: Amount) {
        self.stakes[voter as usize] -= amount;
    }
}

/// One action of the pinned scenario run (digest step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct T6Step {
    /// Step number, starting at 0.
    pub index: u32,
    /// 0 propose, 1 vote, 2 finalize, 3 execute, 4 veto, 5 stake,
    /// 6 unstake, 8 failed action, 9 final step.
    pub kind: u8,
    /// Proposal id (voter id for stake/unstake, 0 elsewhere).
    pub subject: u32,
    /// Block height of the action.
    pub height: u64,
    /// Tally: yes (new stake for stake/unstake; executed count on the
    /// final step).
    pub a: Amount,
    /// Tally: no (rejected count on the final step).
    pub b: Amount,
    /// Tally: abstain (vetoed count on the final step).
    pub c: Amount,
    /// Proposal state after the action (9 = n/a).
    pub state: u8,
    /// Error code for failed actions (0 otherwise).
    pub detail: u32,
}

/// Result of the pinned scenario run.
#[derive(Debug, Clone)]
pub struct T6Run {
    /// Digest steps of the run.
    pub steps: Vec<T6Step>,
    /// Final state of every created proposal, in id order.
    pub states: Vec<u8>,
    /// Number of executed proposals.
    pub executed: u32,
    /// Number of rejected proposals.
    pub rejected: u32,
    /// Number of vetoed proposals.
    pub vetoed: u32,
}

fn state_code(state: ProposalState) -> u8 {
    state as u8
}

/// The pinned T6 scenario: 20 proposals, valid ones pass, the
/// flash-stake attack is rejected.
pub fn run_t6_scenario() -> T6Run {
    let mut gov = Governance::new(vec![
        80_000 * ALCRY,
        50_000 * ALCRY,
        40_000 * ALCRY,
        20_000 * ALCRY,
        10_000 * ALCRY,
        0,
    ]);
    let mut steps: Vec<T6Step> = Vec::new();

    macro_rules! snap {
        ($kind:expr, $subject:expr, $height:expr, $a:expr, $b:expr, $c:expr, $state:expr, $detail:expr) => {
            steps.push(T6Step {
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
        ($proposer:expr, $height:expr) => {{
            match gov.propose($proposer, $height) {
                Ok(id) => {
                    let p = gov.proposal(id);
                    snap!(
                        0,
                        id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    snap!(8, 0, $height, 0, 0, 0, 9, err as u32);
                }
            }
        }};
    }
    macro_rules! do_vote {
        ($id:expr, $voter:expr, $support:expr, $height:expr) => {{
            match gov.vote($id, $voter, $support, $height) {
                Ok(()) => {
                    let p = gov.proposal($id);
                    snap!(
                        1,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = gov.proposal($id);
                    snap!(
                        8,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }
    macro_rules! do_finalize {
        ($id:expr, $height:expr) => {{
            match gov.finalize($id, $height) {
                Ok(()) => {
                    let p = gov.proposal($id);
                    snap!(
                        2,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = gov.proposal($id);
                    snap!(
                        8,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }
    macro_rules! do_execute {
        ($id:expr, $height:expr) => {{
            match gov.execute($id, $height) {
                Ok(()) => {
                    let p = gov.proposal($id);
                    snap!(
                        3,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = gov.proposal($id);
                    snap!(
                        8,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }
    macro_rules! do_veto {
        ($id:expr, $height:expr) => {{
            match gov.veto($id, $height) {
                Ok(()) => {
                    let p = gov.proposal($id);
                    snap!(
                        4,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        0
                    );
                }
                Err(err) => {
                    let p = gov.proposal($id);
                    snap!(
                        8,
                        $id,
                        $height,
                        p.yes,
                        p.no,
                        p.abstain,
                        state_code(p.state),
                        err as u32
                    );
                }
            }
        }};
    }

    // id1: valid pass.
    do_propose!(0, 200_000);
    do_vote!(1, 0, Support::Yes, 210_000);
    do_vote!(1, 1, Support::Yes, 211_000);
    do_vote!(1, 2, Support::Yes, 212_000);
    do_finalize!(1, 321_000);
    do_execute!(1, 355_520);
    // id2: majority against.
    do_propose!(0, 400_000);
    do_vote!(2, 2, Support::Yes, 410_000);
    do_vote!(2, 0, Support::No, 411_000);
    do_finalize!(2, 521_000);
    do_execute!(2, 522_000);
    // id3: quorum missed.
    do_propose!(0, 600_000);
    do_vote!(3, 3, Support::Yes, 610_000);
    do_finalize!(3, 721_000);
    // threshold fail, then id4: valid pass.
    do_propose!(5, 800_000);
    do_propose!(0, 800_001);
    do_vote!(4, 0, Support::Yes, 810_000);
    do_vote!(4, 1, Support::Yes, 811_000);
    do_finalize!(4, 921_000);
    do_execute!(4, 955_560);
    // id5: late vote rejected at the boundary.
    do_propose!(0, 1_000_000);
    do_vote!(5, 0, Support::Yes, 1_010_000);
    do_vote!(5, 1, Support::Yes, 1_020_000);
    do_vote!(5, 2, Support::Yes, 1_120_960);
    do_finalize!(5, 1_120_961);
    do_execute!(5, 1_155_520);
    // id6: timelock boundary.
    do_propose!(0, 1_200_000);
    do_vote!(6, 0, Support::Yes, 1_210_000);
    do_vote!(6, 1, Support::Yes, 1_220_000);
    do_finalize!(6, 1_320_961);
    do_execute!(6, 1_355_519);
    do_execute!(6, 1_355_520);
    // id7: veto one block before the sunset.
    do_propose!(0, 1_400_000);
    do_vote!(7, 0, Support::Yes, 1_410_000);
    do_vote!(7, 1, Support::Yes, 1_420_000);
    do_finalize!(7, 1_520_961);
    do_veto!(7, 1_520_999);
    do_execute!(7, 1_555_521);
    // id8: veto after the sunset is rejected.
    do_propose!(0, 1_600_000);
    do_vote!(8, 0, Support::Yes, 1_610_000);
    do_vote!(8, 1, Support::Yes, 1_620_000);
    do_finalize!(8, 1_720_961);
    do_veto!(8, 1_721_000);
    do_execute!(8, 1_755_520);
    // id9: flash-stake attack rejected.
    do_propose!(0, 1_800_000);
    gov.stake(5, 1_000_000 * ALCRY);
    snap!(5, 5, 1_850_000, gov.stakes()[5], 0, 0, 9, 0);
    do_vote!(9, 5, Support::Yes, 1_850_001);
    do_vote!(9, 0, Support::Yes, 1_860_000);
    do_vote!(9, 1, Support::Yes, 1_861_000);
    do_vote!(9, 2, Support::Yes, 1_862_000);
    do_finalize!(9, 1_920_961);
    do_execute!(9, 1_955_520);
    gov.unstake(5, 1_000_000 * ALCRY);
    snap!(6, 5, 1_960_000, gov.stakes()[5], 0, 0, 9, 0);
    // id10: double vote rejected.
    do_propose!(0, 2_000_000);
    do_vote!(10, 0, Support::Yes, 2_050_000);
    do_vote!(10, 0, Support::No, 2_050_001);
    do_vote!(10, 1, Support::Yes, 2_060_000);
    do_finalize!(10, 2_120_961);
    do_execute!(10, 2_155_520);
    // id11: abstain counts for the quorum.
    do_propose!(0, 2_200_000);
    do_vote!(11, 0, Support::Yes, 2_210_000);
    do_vote!(11, 1, Support::Abstain, 2_211_000);
    do_vote!(11, 2, Support::Abstain, 2_212_000);
    do_finalize!(11, 2_320_961);
    do_execute!(11, 2_355_520);
    // id12: quorum boundary (voted == quorum exactly).
    do_propose!(0, 2_400_000);
    do_vote!(12, 2, Support::Yes, 2_410_000);
    do_finalize!(12, 2_520_961);
    do_execute!(12, 2_555_520);
    // id13: execute at the exact timelock boundary.
    do_propose!(0, 2_600_000);
    do_vote!(13, 0, Support::Yes, 2_610_000);
    do_vote!(13, 1, Support::Yes, 2_611_000);
    do_finalize!(13, 2_720_961);
    do_execute!(13, 2_755_520);
    // id14: a late stake does not move the snapshot.
    do_propose!(0, 2_800_000);
    gov.stake(3, 20_000 * ALCRY);
    snap!(5, 3, 2_850_000, gov.stakes()[3], 0, 0, 9, 0);
    do_vote!(14, 3, Support::Yes, 2_850_001);
    do_vote!(14, 0, Support::Yes, 2_860_000);
    do_finalize!(14, 2_920_961);
    do_execute!(14, 2_955_520);
    // id15: tie is rejected.
    do_propose!(0, 3_000_000);
    do_vote!(15, 0, Support::Yes, 3_010_000);
    do_vote!(15, 4, Support::Yes, 3_011_000);
    do_vote!(15, 1, Support::No, 3_012_000);
    do_vote!(15, 2, Support::No, 3_013_000);
    do_finalize!(15, 3_120_961);
    // id16: threshold boundary (stake == threshold exactly).
    do_propose!(4, 3_200_000);
    do_vote!(16, 0, Support::Yes, 3_210_000);
    do_finalize!(16, 3_320_961);
    do_execute!(16, 3_355_520);
    // id17: no votes, quorum missed.
    do_propose!(0, 3_400_000);
    do_finalize!(17, 3_520_961);
    // id18: double finalize rejected.
    do_propose!(0, 3_600_000);
    do_vote!(18, 0, Support::Yes, 3_610_000);
    do_vote!(18, 1, Support::Yes, 3_611_000);
    do_finalize!(18, 3_720_961);
    do_finalize!(18, 3_720_962);
    do_execute!(18, 3_755_520);
    // id19: double execute rejected.
    do_propose!(0, 3_800_000);
    do_vote!(19, 0, Support::Yes, 3_810_000);
    do_vote!(19, 1, Support::Yes, 3_811_000);
    do_finalize!(19, 3_920_961);
    do_execute!(19, 3_955_520);
    do_execute!(19, 3_955_521);
    // id20: full lifecycle, all five powered voters vote.
    do_propose!(1, 4_000_000);
    do_vote!(20, 0, Support::Yes, 4_010_000);
    do_vote!(20, 1, Support::Yes, 4_011_000);
    do_vote!(20, 2, Support::Yes, 4_012_000);
    do_vote!(20, 3, Support::No, 4_013_000);
    do_vote!(20, 4, Support::Abstain, 4_014_000);
    do_finalize!(20, 4_120_961);
    do_execute!(20, 4_155_520);

    let states: Vec<u8> = gov
        .proposals()
        .iter()
        .map(|p| state_code(p.state))
        .collect();
    let executed = states.iter().filter(|&&s| s == 3).count() as u32;
    let rejected = states.iter().filter(|&&s| s == 2).count() as u32;
    let vetoed = states.iter().filter(|&&s| s == 4).count() as u32;
    snap!(
        9,
        0,
        0,
        executed as Amount,
        rejected as Amount,
        vetoed as Amount,
        9,
        0
    );
    T6Run {
        steps,
        states,
        executed,
        rejected,
        vetoed,
    }
}

/// Digest of the scenario step log (sha256, hex).
pub fn scenario_digest(steps: &[T6Step]) -> String {
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
