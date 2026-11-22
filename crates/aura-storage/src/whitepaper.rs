//! T8 white paper and law: a machine-readable MiCA checklist for the
//! AuraLite crypto-asset white paper (ECONOMICS_PLAN T8).
//!
//! The gate is literal: **checklist against MiCA Art. 4/6/8/9,
//! notification >= 20 working days, publication; not legal advice**.
//! This module is an engineering instrument that checks disclosure
//! readiness — it is **not legal advice** (decision D-T4); the plain
//! wording of the white paper is an instrument, not a legal opinion.
//!
//! MiCA (Regulation (EU) 2023/1114) articles enforced here:
//! - **Art. 4** — offers to the public: the offeror is a legal person
//!   and the paper is drawn up (Art. 6), notified (Art. 8) and
//!   published (Art. 9), with marketing communications drafted to
//!   those rules;
//! - **Art. 6** — content and form: the paper carries the four
//!   sections of the deliverable (project, technology, risks, holder
//!   rights) and every claim is traceable to a pinned model;
//! - **Art. 8** — notification to the competent authority at least
//!   [`T8_NOTICE_WORKING_DAYS`] working days before publication;
//! - **Art. 9** — publication on the offeror's website before the
//!   starting date of the offer; the published version is identical to
//!   the notified one.
//!
//! The pinned scenario [`run_t8_scenario`] drives 8 white paper
//! documents through the lifecycle (1 fully valid; the rest fail one
//! gate each); [`scenario_digest`] is cross-run against the independent
//! Python oracle (`tools/t8_whitepaper_check.py`).

use crate::fees::T3_INITIAL_BASE_FEE;
use crate::governance::T6_PROPOSAL_THRESHOLD;
use crate::issuance::{GENESIS_SUPPLY, MAX_SUPPLY};
use crate::treasury::T7_SIGNER_THRESHOLD;

/// MiCA Art. 8(5): notify at least 20 working days before publication.
pub const T8_NOTICE_WORKING_DAYS: u32 = 20;

/// The four MiCA articles of the gate checklist.
pub const T8_ARTICLES: [u8; 4] = [4, 6, 8, 9];

/// The mandatory not-legal-advice disclaimer (decision D-T4).
pub const T8_NOT_LEGAL_ADVICE: &str =
    "This document is an engineering instrument and not legal advice.";

/// White paper gate errors (numeric codes appear in the step log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum T8Error {
    /// A required section (project/technology/risks/rights) is missing.
    SectionMissing = 1,
    /// A claim does not match its pinned model constant.
    ClaimUnverified = 2,
    /// The not-legal-advice disclaimer is absent.
    NoDisclaimer = 3,
    /// Publication less than [`T8_NOTICE_WORKING_DAYS`] after notification.
    NoticeTooShort = 4,
    /// Notification or publication did not happen.
    NotPublished = 5,
    /// Marketing does not reference the published white paper.
    MarketingNotCompliant = 6,
    /// Published at or after the starting date of the offer.
    PublishedAfterOffer = 7,
    /// Published version differs from the notified version (Art. 9(2)).
    VersionMismatch = 8,
    /// The offeror is not a legal person (Art. 4(1)(a)).
    NoLegalPerson = 9,
}

/// The four mandatory white paper sections (the T8 deliverable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WpSection {
    /// What the project is.
    Project = 1,
    /// How the technology works.
    Technology = 2,
    /// What can go wrong.
    Risks = 3,
    /// What holders are entitled to.
    Rights = 4,
}

/// Claim kinds traceable to pinned model constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    /// `issuance::MAX_SUPPLY`.
    MaxSupply = 1,
    /// `issuance::GENESIS_SUPPLY`.
    GenesisSupply = 2,
    /// `fees::T3_INITIAL_BASE_FEE`.
    InitialBaseFee = 3,
    /// `governance::T6_PROPOSAL_THRESHOLD`.
    ProposalThreshold = 4,
    /// `treasury::T7_SIGNER_THRESHOLD`.
    TreasurySignerThreshold = 5,
}

impl ClaimKind {
    /// The pinned model constant this claim must equal.
    pub fn pin(self) -> u128 {
        match self {
            ClaimKind::MaxSupply => MAX_SUPPLY,
            ClaimKind::GenesisSupply => GENESIS_SUPPLY,
            ClaimKind::InitialBaseFee => T3_INITIAL_BASE_FEE,
            ClaimKind::ProposalThreshold => T6_PROPOSAL_THRESHOLD,
            ClaimKind::TreasurySignerThreshold => T7_SIGNER_THRESHOLD as u128,
        }
    }
}

/// One white paper claim with its expected pinned value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WpClaim {
    /// Which pinned model constant the claim cites.
    pub kind: ClaimKind,
    /// The value the paper states; must equal [`ClaimKind::pin`].
    pub expected: u128,
}

/// One marketing communication (MiCA Art. 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marketing {
    /// The communication references the white paper.
    pub references_wp: bool,
    /// The communication was disseminated after WP publication.
    pub after_publication: bool,
}

/// A crypto-asset white paper under the T8 machine.
#[derive(Debug, Clone)]
pub struct WhitePaper {
    /// Document id, counting from 1.
    pub id: u32,
    /// The offeror is a legal person (Art. 4(1)(a)).
    pub legal_person: bool,
    /// Sections drawn up so far.
    pub sections: Vec<WpSection>,
    /// Claims stated in the paper.
    pub claims: Vec<WpClaim>,
    /// The not-legal-advice disclaimer is included.
    pub disclaimer: bool,
    /// Working day of the Art. 8 notification.
    pub notified_day: Option<u32>,
    /// Working day of the Art. 9 publication.
    pub published_day: Option<u32>,
    /// Version notified to the competent authority.
    pub notified_version: u64,
    /// Version published on the website.
    pub published_version: u64,
    /// Starting date of the offer (working day).
    pub offer_start: u32,
    /// Marketing communications attached to the offer.
    pub marketing: Vec<Marketing>,
}

impl WhitePaper {
    /// New draft by an offeror.
    pub fn new(id: u32, legal_person: bool) -> Self {
        Self {
            id,
            legal_person,
            sections: Vec::new(),
            claims: Vec::new(),
            disclaimer: false,
            notified_day: None,
            published_day: None,
            notified_version: 1,
            published_version: 1,
            offer_start: 0,
            marketing: Vec::new(),
        }
    }

    /// Draw up one section.
    pub fn draw_section(&mut self, section: WpSection) {
        self.sections.push(section);
    }

    /// State one claim.
    pub fn add_claim(&mut self, claim: WpClaim) {
        self.claims.push(claim);
    }

    /// Verify every claim against its pinned model constant.
    pub fn verify_claims(&self) -> (bool, u32) {
        let ok = self
            .claims
            .iter()
            .filter(|claim| claim.expected == claim.kind.pin())
            .count() as u32;
        (ok == self.claims.len() as u32, ok)
    }

    /// Include the not-legal-advice disclaimer.
    pub fn add_disclaimer(&mut self) {
        self.disclaimer = true;
    }

    /// Notify the competent authority (Art. 8) on a working day.
    pub fn notify(&mut self, day: u32) {
        self.notified_day = Some(day);
        self.notified_version = 1;
    }

    /// Publish on the website (Art. 9) before `offer_start` begins.
    pub fn publish(&mut self, day: u32, offer_start: u32, version: Option<u64>) {
        self.published_day = Some(day);
        self.published_version = version.unwrap_or(self.notified_version);
        self.offer_start = offer_start;
    }

    /// Attach one marketing communication (Art. 7).
    pub fn market(&mut self, marketing: Marketing) {
        self.marketing.push(marketing);
    }

    /// The MiCA checklist (Art. 4/6/8/9 + Art. 7 marketing). Returns
    /// `Ok(())` when the paper is disclosure-ready, or the first failed
    /// article check as [`T8Error`].
    pub fn check(&self) -> Result<(), T8Error> {
        if !self.legal_person {
            return Err(T8Error::NoLegalPerson);
        }
        for section in [
            WpSection::Project,
            WpSection::Technology,
            WpSection::Risks,
            WpSection::Rights,
        ] {
            if !self.sections.contains(&section) {
                return Err(T8Error::SectionMissing);
            }
        }
        let (claims_ok, _) = self.verify_claims();
        if !claims_ok {
            return Err(T8Error::ClaimUnverified);
        }
        if !self.disclaimer {
            return Err(T8Error::NoDisclaimer);
        }
        let (Some(notified), Some(published)) = (self.notified_day, self.published_day) else {
            return Err(T8Error::NotPublished);
        };
        if published - notified < T8_NOTICE_WORKING_DAYS {
            return Err(T8Error::NoticeTooShort);
        }
        if self.published_version != self.notified_version {
            return Err(T8Error::VersionMismatch);
        }
        if published >= self.offer_start {
            return Err(T8Error::PublishedAfterOffer);
        }
        for marketing in &self.marketing {
            if !marketing.references_wp || !marketing.after_publication {
                return Err(T8Error::MarketingNotCompliant);
            }
        }
        Ok(())
    }
}

/// One action of the pinned scenario run (digest step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct T8Step {
    /// Step number, starting at 0.
    pub index: u32,
    /// 0 section, 1 claim, 2 verify, 3 disclaimer, 4 notify, 5 publish,
    /// 6 market, 7 check pass, 8 failed check, 9 final step.
    pub kind: u8,
    /// White paper id (0 for final).
    pub subject: u32,
    /// Working day of the action.
    pub day: u32,
    /// kind-specific: section code / claim kind / verified count /
    /// disclaimer flag / notify flag / publish flag / marketing refs.
    pub a: u32,
    /// kind-specific: claim expected / verified total / offer start /
    /// marketing after-publication flag / valid count (final).
    pub b: u128,
    /// kind-specific: published version (final 0).
    pub c: u64,
    /// Document state after the action (0 draft, 1 notified, 2
    /// published, 3 ready; 9 = n/a).
    pub state: u8,
    /// Error code for failed checks (0 otherwise).
    pub detail: u32,
}

/// Result of the pinned scenario run.
#[derive(Debug, Clone)]
pub struct T8Run {
    /// Digest steps of the run.
    pub steps: Vec<T8Step>,
    /// Final checklist outcome of every document (0 = ready).
    pub outcomes: Vec<u32>,
    /// Number of documents.
    pub total: u32,
    /// Number of disclosure-ready documents.
    pub valid: u32,
}

/// The pinned T8 scenario: 8 white papers through the MiCA lifecycle.
pub fn run_t8_scenario() -> T8Run {
    let mut docs: Vec<WhitePaper> = Vec::new();
    let mut steps: Vec<T8Step> = Vec::new();
    let mut states: Vec<u8> = Vec::new();

    macro_rules! snap {
        ($kind:expr, $subject:expr, $day:expr, $a:expr, $b:expr, $c:expr, $state:expr, $detail:expr) => {
            steps.push(T8Step {
                index: steps.len() as u32,
                kind: $kind,
                subject: $subject,
                day: $day,
                a: $a,
                b: $b,
                c: $c,
                state: $state,
                detail: $detail,
            })
        };
    }
    macro_rules! doc_state {
        ($doc:expr) => {{
            let doc = &$doc;
            if doc.published_day.is_some() {
                2u8
            } else if doc.notified_day.is_some() {
                1u8
            } else {
                0u8
            }
        }};
    }

    fn finish(steps: &mut Vec<T8Step>, states: &mut Vec<u8>, doc: &WhitePaper, day: u32) {
        let index = steps.len() as u32;
        match doc.check() {
            Ok(()) => {
                states.push(3);
                steps.push(T8Step {
                    index,
                    kind: 7,
                    subject: doc.id,
                    day,
                    a: 1,
                    b: 0,
                    c: 0,
                    state: 3,
                    detail: 0,
                });
            }
            Err(err) => {
                let state = if doc.published_day.is_some() {
                    2
                } else if doc.notified_day.is_some() {
                    1
                } else {
                    0
                };
                states.push(state);
                steps.push(T8Step {
                    index,
                    kind: 8,
                    subject: doc.id,
                    day,
                    a: 0,
                    b: 0,
                    c: 0,
                    state,
                    detail: err as u32,
                });
            }
        }
    }

    // WP1: fully valid (boundary: notify 100, publish 120 -> exactly 20).
    let mut doc = WhitePaper::new(1, true);
    for code in [1u32, 2, 3, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            3 => WpSection::Risks,
            _ => WpSection::Rights,
        });
        snap!(0, 1, 10 + code, code, 0, 0, doc_state!(doc), 0);
    }
    for kind in [1u32, 2, 3, 4, 5] {
        let claim = WpClaim {
            kind: match kind {
                1 => ClaimKind::MaxSupply,
                2 => ClaimKind::GenesisSupply,
                3 => ClaimKind::InitialBaseFee,
                4 => ClaimKind::ProposalThreshold,
                _ => ClaimKind::TreasurySignerThreshold,
            },
            expected: 0,
        };
        doc.add_claim(WpClaim {
            kind: claim.kind,
            expected: claim.kind.pin(),
        });
        snap!(
            1,
            1,
            20 + kind,
            kind,
            claim.kind.pin(),
            0,
            doc_state!(doc),
            0
        );
    }
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        1,
        30,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    doc.add_disclaimer();
    snap!(3, 1, 31, 1, 0, 0, doc_state!(doc), 0);
    doc.notify(100);
    snap!(4, 1, 100, 1, 0, 0, doc_state!(doc), 0);
    doc.publish(120, 130, None);
    snap!(5, 1, 120, 1, 130, 1, doc_state!(doc), 0);
    doc.market(Marketing {
        references_wp: true,
        after_publication: true,
    });
    snap!(6, 1, 121, 1, 1, 0, doc_state!(doc), 0);
    finish(&mut steps, &mut states, &doc, 122);
    docs.push(doc);

    // WP2: unverified claim.
    let mut doc = WhitePaper::new(2, true);
    for code in [1u32, 2, 3, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            3 => WpSection::Risks,
            _ => WpSection::Rights,
        });
        snap!(0, 2, 200 + code, code, 0, 0, doc_state!(doc), 0);
    }
    doc.add_claim(WpClaim {
        kind: ClaimKind::MaxSupply,
        expected: ClaimKind::MaxSupply.pin(),
    });
    snap!(
        1,
        2,
        210,
        1,
        ClaimKind::MaxSupply.pin(),
        0,
        doc_state!(doc),
        0
    );
    doc.add_claim(WpClaim {
        kind: ClaimKind::InitialBaseFee,
        expected: ClaimKind::InitialBaseFee.pin() + 7,
    });
    snap!(
        1,
        2,
        211,
        3,
        ClaimKind::InitialBaseFee.pin() + 7,
        0,
        doc_state!(doc),
        0
    );
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        2,
        212,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    finish(&mut steps, &mut states, &doc, 213);
    docs.push(doc);

    // WP3: no disclaimer.
    let mut doc = WhitePaper::new(3, true);
    for code in [1u32, 2, 3, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            3 => WpSection::Risks,
            _ => WpSection::Rights,
        });
        snap!(0, 3, 300 + code, code, 0, 0, doc_state!(doc), 0);
    }
    doc.add_claim(WpClaim {
        kind: ClaimKind::TreasurySignerThreshold,
        expected: ClaimKind::TreasurySignerThreshold.pin(),
    });
    snap!(
        1,
        3,
        310,
        5,
        ClaimKind::TreasurySignerThreshold.pin(),
        0,
        doc_state!(doc),
        0
    );
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        3,
        311,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    doc.notify(320);
    snap!(4, 3, 320, 1, 0, 0, doc_state!(doc), 0);
    doc.publish(341, 350, None);
    snap!(5, 3, 341, 1, 350, 1, doc_state!(doc), 0);
    finish(&mut steps, &mut states, &doc, 342);
    docs.push(doc);

    // WP4: notification too short (19 working days).
    let mut doc = WhitePaper::new(4, true);
    for code in [1u32, 2, 3, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            3 => WpSection::Risks,
            _ => WpSection::Rights,
        });
        snap!(0, 4, 400 + code, code, 0, 0, doc_state!(doc), 0);
    }
    doc.add_claim(WpClaim {
        kind: ClaimKind::MaxSupply,
        expected: ClaimKind::MaxSupply.pin(),
    });
    snap!(
        1,
        4,
        410,
        1,
        ClaimKind::MaxSupply.pin(),
        0,
        doc_state!(doc),
        0
    );
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        4,
        411,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    doc.add_disclaimer();
    snap!(3, 4, 412, 1, 0, 0, doc_state!(doc), 0);
    doc.notify(420);
    snap!(4, 4, 420, 1, 0, 0, doc_state!(doc), 0);
    doc.publish(439, 450, None);
    snap!(5, 4, 439, 1, 450, 1, doc_state!(doc), 0);
    finish(&mut steps, &mut states, &doc, 440);
    docs.push(doc);

    // WP5: published after offer start.
    let mut doc = WhitePaper::new(5, true);
    for code in [1u32, 2, 3, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            3 => WpSection::Risks,
            _ => WpSection::Rights,
        });
        snap!(0, 5, 500 + code, code, 0, 0, doc_state!(doc), 0);
    }
    doc.add_claim(WpClaim {
        kind: ClaimKind::MaxSupply,
        expected: ClaimKind::MaxSupply.pin(),
    });
    snap!(
        1,
        5,
        510,
        1,
        ClaimKind::MaxSupply.pin(),
        0,
        doc_state!(doc),
        0
    );
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        5,
        511,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    doc.add_disclaimer();
    snap!(3, 5, 512, 1, 0, 0, doc_state!(doc), 0);
    doc.notify(520);
    snap!(4, 5, 520, 1, 0, 0, doc_state!(doc), 0);
    doc.publish(541, 540, None);
    snap!(5, 5, 541, 1, 540, 1, doc_state!(doc), 0);
    finish(&mut steps, &mut states, &doc, 542);
    docs.push(doc);

    // WP6: marketing without reference and before publication.
    let mut doc = WhitePaper::new(6, true);
    for code in [1u32, 2, 3, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            3 => WpSection::Risks,
            _ => WpSection::Rights,
        });
        snap!(0, 6, 600 + code, code, 0, 0, doc_state!(doc), 0);
    }
    doc.add_claim(WpClaim {
        kind: ClaimKind::MaxSupply,
        expected: ClaimKind::MaxSupply.pin(),
    });
    snap!(
        1,
        6,
        610,
        1,
        ClaimKind::MaxSupply.pin(),
        0,
        doc_state!(doc),
        0
    );
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        6,
        611,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    doc.add_disclaimer();
    snap!(3, 6, 612, 1, 0, 0, doc_state!(doc), 0);
    doc.notify(620);
    snap!(4, 6, 620, 1, 0, 0, doc_state!(doc), 0);
    doc.publish(641, 650, None);
    snap!(5, 6, 641, 1, 650, 1, doc_state!(doc), 0);
    doc.market(Marketing {
        references_wp: false,
        after_publication: false,
    });
    snap!(6, 6, 625, 0, 0, 0, doc_state!(doc), 0);
    finish(&mut steps, &mut states, &doc, 642);
    docs.push(doc);

    // WP7: missing risks section.
    let mut doc = WhitePaper::new(7, true);
    for code in [1u32, 2, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            _ => WpSection::Rights,
        });
        snap!(0, 7, 700 + code, code, 0, 0, doc_state!(doc), 0);
    }
    doc.add_claim(WpClaim {
        kind: ClaimKind::MaxSupply,
        expected: ClaimKind::MaxSupply.pin(),
    });
    snap!(
        1,
        7,
        710,
        1,
        ClaimKind::MaxSupply.pin(),
        0,
        doc_state!(doc),
        0
    );
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        7,
        711,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    finish(&mut steps, &mut states, &doc, 712);
    docs.push(doc);

    // WP8: not a legal person.
    let mut doc = WhitePaper::new(8, false);
    for code in [1u32, 2, 3, 4] {
        doc.draw_section(match code {
            1 => WpSection::Project,
            2 => WpSection::Technology,
            3 => WpSection::Risks,
            _ => WpSection::Rights,
        });
        snap!(0, 8, 800 + code, code, 0, 0, doc_state!(doc), 0);
    }
    doc.add_claim(WpClaim {
        kind: ClaimKind::MaxSupply,
        expected: ClaimKind::MaxSupply.pin(),
    });
    snap!(
        1,
        8,
        810,
        1,
        ClaimKind::MaxSupply.pin(),
        0,
        doc_state!(doc),
        0
    );
    let (ok, verified) = doc.verify_claims();
    snap!(
        2,
        8,
        811,
        verified,
        doc.claims.len() as u128,
        0,
        doc_state!(doc),
        if ok { 0 } else { 2 }
    );
    finish(&mut steps, &mut states, &doc, 812);
    docs.push(doc);

    let outcomes: Vec<u32> = docs
        .iter()
        .map(|d| d.check().err().map(|e| e as u32).unwrap_or(0))
        .collect();
    let valid = outcomes.iter().filter(|&&o| o == 0).count() as u128;
    snap!(9, 0, 0, docs.len() as u32, valid, 0, 9, 0);
    let _ = &mut states;
    T8Run {
        outcomes,
        total: docs.len() as u32,
        valid: valid as u32,
        steps,
    }
}

/// Digest of the scenario step log (sha256, hex).
pub fn scenario_digest(steps: &[T8Step]) -> String {
    let mut blob = String::new();
    for step in steps {
        blob.push_str(&format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}\n",
            step.index,
            step.kind,
            step.subject,
            step.day,
            step.a,
            step.b,
            step.c,
            step.state,
            step.detail
        ));
    }
    aura_state::sha256_bytes(blob.as_bytes()).to_hex()
}
