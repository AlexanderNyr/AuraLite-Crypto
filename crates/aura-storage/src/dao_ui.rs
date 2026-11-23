//! E8 governance UI: the user-facing layer over the T6 [`Governance`]
//! DAO (ECOSYSTEM_PLAN E8 — "first DAO vote through the UI").
//!
//! [`DaoUi`] owns the [`Governance`] instance and exposes only the UI
//! action path: proposals, votes, finalization and execution are
//! reachable through the controller, every accepted vote produces a
//! [`UiReceipt`] in the action log, and [`DaoUi::render_html`] paints
//! the deterministic governance page (`docs/evidence/e8/
//! governance-ui.html` in the evidence tier).
//!
//! The pinned scenario [`run_e8_scenario`] opens proposal 1 and casts
//! **the first DAO vote through the UI** (voter 1, yes); its step log
//! is cross-run against the independent Python model
//! (`tools/e8_nft_check.py`) and must reproduce the golden scenario
//! digest.

use std::fmt;

use crate::econ::Amount;
use crate::governance::{GovError, Governance, ProposalState, Support};
use crate::issuance::ALCRY;
use crate::nft::E8Error;
use aura_state::sha256_bytes;

/// One accepted vote, as recorded by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiReceipt {
    /// Proposal the vote was cast on.
    pub proposal: u32,
    /// Voter id.
    pub voter: u32,
    /// Chosen support.
    pub support: Support,
    /// Snapshot power that was counted.
    pub power: Amount,
}

/// The governance UI controller: owns the DAO and its action log.
#[derive(Debug, Clone)]
pub struct DaoUi {
    gov: Governance,
    receipts: Vec<UiReceipt>,
    labels: Vec<String>,
}

fn map_gov(err: GovError, proposal: u32, voter: u32) -> E8Error {
    match err {
        GovError::BelowThreshold => E8Error::BelowProposalThreshold { proposer: voter },
        GovError::VotingClosed => E8Error::VotingClosed { proposal },
        GovError::NoVotingPower => E8Error::NoVotingPower { voter },
        GovError::AlreadyVoted => E8Error::VoteCast { proposal, voter },
        GovError::StillVoting => E8Error::VotingStillOpen { proposal },
        GovError::AlreadyFinalized => E8Error::AlreadyFinalized { proposal },
        GovError::Timelock => E8Error::TimelockNotReached { proposal },
        GovError::NotPassed => E8Error::NotPassed { proposal },
        GovError::AlreadyExecuted => E8Error::AlreadyExecuted { proposal },
        GovError::VetoSunset => E8Error::VetoSunset,
        GovError::NoSuchProposal => E8Error::ProposalMissing { id: proposal },
    }
}

impl DaoUi {
    /// New UI over a DAO with the given stakes and display labels.
    pub fn new(stakes: Vec<Amount>, labels: Vec<String>) -> Self {
        Self {
            gov: Governance::new(stakes),
            receipts: Vec::new(),
            labels,
        }
    }

    /// The wrapped DAO (read-only).
    pub fn gov(&self) -> &Governance {
        &self.gov
    }

    /// Open a proposal through the UI.
    pub fn propose(&mut self, proposer: u32, height: u64) -> Result<u32, E8Error> {
        self.gov
            .propose(proposer, height)
            .map_err(|e| map_gov(e, 0, proposer))
    }

    /// Cast a vote through the UI. The UI rejects a second vote by the
    /// same voter before it reaches the DAO, forwards accepted votes to
    /// [`Governance::vote`] and records a [`UiReceipt`].
    pub fn vote(
        &mut self,
        proposal: u32,
        voter: u32,
        support: Support,
        height: u64,
    ) -> Result<UiReceipt, E8Error> {
        if self
            .receipts
            .iter()
            .any(|r| r.proposal == proposal && r.voter == voter)
        {
            return Err(E8Error::VoteCast { proposal, voter });
        }
        if proposal == 0 || proposal as usize > self.gov.proposals().len() {
            return Err(E8Error::ProposalMissing { id: proposal });
        }
        let power = self.gov.proposal(proposal).snapshot[voter as usize];
        self.gov
            .vote(proposal, voter, support, height)
            .map_err(|e| map_gov(e, proposal, voter))?;
        let receipt = UiReceipt {
            proposal,
            voter,
            support,
            power,
        };
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }

    /// Close a proposal through the UI.
    pub fn finalize(&mut self, proposal: u32, height: u64) -> Result<(), E8Error> {
        if proposal == 0 || proposal as usize > self.gov.proposals().len() {
            return Err(E8Error::ProposalMissing { id: proposal });
        }
        self.gov
            .finalize(proposal, height)
            .map_err(|e| map_gov(e, proposal, 0))
    }

    /// Execute a passed proposal through the UI.
    pub fn execute(&mut self, proposal: u32, height: u64) -> Result<(), E8Error> {
        if proposal == 0 || proposal as usize > self.gov.proposals().len() {
            return Err(E8Error::ProposalMissing { id: proposal });
        }
        self.gov
            .execute(proposal, height)
            .map_err(|e| map_gov(e, proposal, 0))
    }

    /// Emergency veto through the UI (sunsets at the T6 sunset block).
    pub fn veto(&mut self, proposal: u32, height: u64) -> Result<(), E8Error> {
        if proposal == 0 || proposal as usize > self.gov.proposals().len() {
            return Err(E8Error::ProposalMissing { id: proposal });
        }
        self.gov
            .veto(proposal, height)
            .map_err(|e| map_gov(e, proposal, 0))
    }

    /// The action log: one receipt per accepted vote, in order.
    pub fn receipts(&self) -> &[UiReceipt] {
        &self.receipts
    }

    /// Tally of a proposal: (yes, no, abstain).
    pub fn tally(&self, proposal: u32) -> (Amount, Amount, Amount) {
        let p = self.gov.proposal(proposal);
        (p.yes, p.no, p.abstain)
    }

    /// Human label of a voter id.
    pub fn label(&self, voter: u32) -> &str {
        self.labels
            .get(voter as usize)
            .map(String::as_str)
            .unwrap_or("?")
    }

    /// The deterministic governance page (self-contained HTML).
    ///
    /// The template is mirrored byte for byte by the independent Python
    /// model (`tools/e8_nft_check.py`); both renders must hash equal.
    pub fn render_html(&self) -> String {
        let mut h = String::new();
        for line in [
            "<!doctype html>",
            "<html lang=\"en\">",
            "<head>",
            "<meta charset=\"utf-8\">",
            "<title>AuraLite DAO — governance UI</title>",
            "<style>",
            "body{font-family:sans-serif;background:#0f1220;color:#e8eaf6;margin:2rem;max-width:52rem}",
            "h1{font-size:1.4rem}",
            ".card{background:#1a1f36;border-radius:10px;padding:1rem 1.2rem;margin:1rem 0}",
            ".badge{padding:.15rem .6rem;border-radius:999px;font-size:.8rem;background:#2b3157}",
            ".passed{background:#1b5e20}.executed{background:#00695c}.rejected{background:#7f1d1d}",
            "table{width:100%;border-collapse:collapse;font-size:.9rem}",
            "td,th{padding:.35rem .5rem;text-align:left;border-bottom:1px solid #2b3157}",
            ".meter{background:#2b3157;border-radius:6px;height:.6rem;margin:.25rem 0}",
            ".meter i{display:block;height:100%;border-radius:6px;background:#7c86ff}",
            ".foot{opacity:.7;font-size:.8rem;margin-top:1.5rem}",
            "</style>",
            "</head>",
            "<body>",
            "<h1>AuraLite DAO — governance UI</h1>",
        ] {
            h.push_str(line);
            h.push('\n');
        }
        for p in self.gov.proposals() {
            let state = match p.state {
                ProposalState::Voting => "voting",
                ProposalState::Passed => "passed",
                ProposalState::Rejected => "rejected",
                ProposalState::Executed => "executed",
                ProposalState::Vetoed => "vetoed",
            };
            let (yes, no, abstain) = (p.yes, p.no, p.abstain);
            let total = yes + no + abstain;
            let pct = (yes * 100).checked_div(total).unwrap_or(0) as u32;
            h.push_str(&format!(
                "<div class=\"card\"><strong>Proposal #{}</strong> <span class=\"badge {state}\">{state}</span>",
                p.id
            ));
            h.push('\n');
            h.push_str(&format!(
                "<div>proposer: {} &middot; created at block {}</div>",
                self.label(p.proposer),
                p.created_at
            ));
            h.push('\n');
            h.push_str(&format!(
                "<div class=\"meter\"><i style=\"width:{pct}%\"></i></div>"
            ));
            h.push('\n');
            h.push_str("<table><tr><th>yes</th><th>no</th><th>abstain</th></tr>");
            h.push('\n');
            h.push_str(&format!(
                "<tr><td>{} ALCRY</td><td>{} ALCRY</td><td>{} ALCRY</td></tr>",
                yes / ALCRY,
                no / ALCRY,
                abstain / ALCRY
            ));
            h.push('\n');
            h.push_str("</table></div>");
            h.push('\n');
        }
        h.push_str("<div class=\"card\"><strong>Votes through the UI</strong>");
        h.push('\n');
        h.push_str(
            "<table><tr><th>#</th><th>proposal</th><th>voter</th><th>support</th><th>power</th></tr>",
        );
        h.push('\n');
        for (i, r) in self.receipts.iter().enumerate() {
            let support = match r.support {
                Support::Yes => "yes",
                Support::No => "no",
                Support::Abstain => "abstain",
            };
            h.push_str(&format!(
                "<tr><td>{}</td><td>#{}</td><td>{}</td><td>{}</td><td>{} ALCRY</td></tr>",
                i + 1,
                r.proposal,
                self.label(r.voter),
                support,
                r.power / ALCRY
            ));
            h.push('\n');
        }
        h.push_str("</table></div>");
        h.push('\n');
        h.push_str(&format!(
            "<div class=\"foot\">{} vote receipts &middot; rendered by AuraLite governance UI (E8)</div>",
            self.receipts.len()
        ));
        h.push('\n');
        h.push_str("</body>");
        h.push('\n');
        h.push_str("</html>");
        h.push('\n');
        h
    }
}

/// The pinned E8 DAO run: proposal 1 with **the first vote cast through
/// the UI**.
#[derive(Debug, Clone)]
pub struct E8Run {
    /// Canonical step log (the digest input).
    pub steps: Vec<String>,
    /// The first vote receipt (voter 1, yes).
    pub first_vote: UiReceipt,
    /// Accepted votes through the UI.
    pub ui_votes: u64,
    /// Final tally (yes, no, abstain).
    pub tally: (Amount, Amount, Amount),
    /// Final state name ("executed").
    pub state: &'static str,
}

/// Run the pinned scenario and return its step log.
pub fn run_e8_scenario() -> E8Run {
    let k = |n: u64| Amount::from(n) * ALCRY;
    let mut ui = DaoUi::new(
        vec![k(250_000), k(150_000), k(100_000), k(75_000), k(25_000)],
        ["alice", "bob", "carol", "dave", "eve"]
            .iter()
            .map(|s| String::from(*s))
            .collect(),
    );
    let mut steps: Vec<String> = Vec::new();

    let id = ui.propose(0, 1000).expect("proposer above threshold");
    steps.push(format!("propose|{id}|proposer=0|created=1000"));

    // The first DAO vote, through the UI.
    let first = ui.vote(id, 1, Support::Yes, 2000).expect("first UI vote");
    steps.push(format!("vote|{id}|1|yes|{}", first.power));
    for (voter, support, height, word) in [
        (2u32, Support::Yes, 3000u64, "yes"),
        (3, Support::No, 4000, "no"),
        (4, Support::Abstain, 5000, "abstain"),
    ] {
        let r = ui.vote(id, voter, support, height).expect("UI vote");
        steps.push(format!("vote|{id}|{voter}|{word}|{}", r.power));
    }
    // A second vote by voter 1 is rejected by the UI.
    let err = ui
        .vote(id, 1, Support::Yes, 6000)
        .expect_err("double vote rejected");
    steps.push(format!("vote_err|{id}|1|{err}"));

    ui.finalize(id, 121_960).expect("voting closed");
    let state = match ui.gov().proposal(id).state {
        ProposalState::Passed => "passed",
        ProposalState::Rejected => "rejected",
        _ => unreachable!("finalize decides"),
    };
    steps.push(format!("finalize|{id}|{state}"));

    ui.execute(id, 156_520).expect("timelock elapsed");
    steps.push(format!("execute|{id}|executed"));

    let ui_votes = ui.receipts().len() as u64;
    let tally = ui.tally(id);
    E8Run {
        steps,
        first_vote: first,
        ui_votes,
        tally,
        state: "executed",
    }
}

/// Digest of the scenario step log (sha256, hex).
pub fn scenario_digest(steps: &[String]) -> String {
    let blob = steps.join(";");
    sha256_bytes(blob.as_bytes()).to_hex()
}

impl fmt::Display for Support {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Support::Yes => write!(f, "yes"),
            Support::No => write!(f, "no"),
            Support::Abstain => write!(f, "abstain"),
        }
    }
}
