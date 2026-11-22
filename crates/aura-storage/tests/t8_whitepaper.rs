//! T8 gate tests: the MiCA checklist (Art. 4/6/8/9), notification >= 20
//! working days, publication before the offer, marketing compliance and
//! the not-legal-advice disclaimer (ECONOMICS_PLAN T8: "checklist against
//! MiCA Art. 4/6/8/9, notification >= 20 working days, publication; not
//! legal advice").

use aura_storage::whitepaper::{
    ClaimKind, Marketing, T8_NOTICE_WORKING_DAYS, T8Error, WhitePaper, WpSection, run_t8_scenario,
    scenario_digest,
};

const GOLDEN_WP_DIGEST: &str = "03a859433b58f14d6ad538faf2f3b504631c8b05707856a6ef1580694a89396f";

fn full_doc(id: u32) -> WhitePaper {
    let mut doc = WhitePaper::new(id, true);
    for section in [
        WpSection::Project,
        WpSection::Technology,
        WpSection::Risks,
        WpSection::Rights,
    ] {
        doc.draw_section(section);
    }
    for kind in [
        ClaimKind::MaxSupply,
        ClaimKind::GenesisSupply,
        ClaimKind::InitialBaseFee,
        ClaimKind::ProposalThreshold,
        ClaimKind::TreasurySignerThreshold,
    ] {
        doc.add_claim(aura_storage::whitepaper::WpClaim {
            kind,
            expected: kind.pin(),
        });
    }
    doc.add_disclaimer();
    doc.notify(100);
    doc.publish(120, 130, None);
    doc.market(Marketing {
        references_wp: true,
        after_publication: true,
    });
    doc
}

#[test]
fn t8_the_mica_checklist_must_cover_articles_4_6_8_and_9() {
    // Art. 4: legal person
    let mut doc = full_doc(1);
    doc.legal_person = false;
    assert_eq!(
        doc.check(),
        Err(T8Error::NoLegalPerson),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
    // Art. 6: content (all four sections)
    let mut doc = full_doc(1);
    doc.sections.retain(|s| *s != WpSection::Risks);
    assert_eq!(
        doc.check(),
        Err(T8Error::SectionMissing),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
    // Art. 8: notification >= 20 working days
    let mut doc = full_doc(1);
    doc.publish(119, 130, None);
    assert_eq!(
        doc.check(),
        Err(T8Error::NoticeTooShort),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
    // Art. 9: publication before the offer starts
    let mut doc = full_doc(1);
    doc.publish(130, 130, None);
    assert_eq!(
        doc.check(),
        Err(T8Error::PublishedAfterOffer),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
    // all four articles pass
    assert_eq!(
        full_doc(1).check(),
        Ok(()),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
}

#[test]
fn t8_every_white_paper_claim_must_be_traceable_to_a_pinned_model() {
    let mut doc = full_doc(1);
    doc.claims[0].expected += 1;
    assert_eq!(
        doc.check(),
        Err(T8Error::ClaimUnverified),
        "T8: every white paper claim must be traceable to a pinned model"
    );
    let (ok, _) = full_doc(1).verify_claims();
    assert!(
        ok,
        "T8: every white paper claim must be traceable to a pinned model"
    );
    assert_eq!(
        ClaimKind::MaxSupply.pin(),
        6_000_000 * 10u128.pow(18),
        "T8: every white paper claim must be traceable to a pinned model"
    );
    assert_eq!(
        ClaimKind::TreasurySignerThreshold.pin(),
        2,
        "T8: every white paper claim must be traceable to a pinned model"
    );
}

#[test]
fn t8_the_notification_must_be_at_least_20_working_days_before_publication() {
    assert_eq!(
        T8_NOTICE_WORKING_DAYS, 20,
        "T8: the notification must be at least 20 working days before publication"
    );
    // boundary: exactly 20 passes
    let mut doc = full_doc(1);
    doc.publish(120, 130, None);
    assert_eq!(
        doc.check(),
        Ok(()),
        "T8: the notification must be at least 20 working days before publication"
    );
    // 19 days is red
    let mut doc = full_doc(1);
    doc.publish(119, 130, None);
    assert_eq!(
        doc.check(),
        Err(T8Error::NoticeTooShort),
        "T8: the notification must be at least 20 working days before publication"
    );
}

#[test]
fn t8_the_white_paper_must_be_published_before_the_offer_starts() {
    let mut doc = full_doc(1);
    doc.publish(131, 130, None);
    assert_eq!(
        doc.check(),
        Err(T8Error::PublishedAfterOffer),
        "T8: the white paper must be published before the offer starts"
    );
    let mut doc = full_doc(1);
    doc.publish(130, 130, None);
    assert_eq!(
        doc.check(),
        Err(T8Error::PublishedAfterOffer),
        "T8: the white paper must be published before the offer starts"
    );
    assert_eq!(
        full_doc(1).check(),
        Ok(()),
        "T8: the white paper must be published before the offer starts"
    );
}

#[test]
fn t8_marketing_communications_must_reference_the_published_white_paper() {
    let mut doc = full_doc(1);
    doc.marketing.push(Marketing {
        references_wp: false,
        after_publication: true,
    });
    assert_eq!(
        doc.check(),
        Err(T8Error::MarketingNotCompliant),
        "T8: marketing communications must reference the published white paper"
    );
    let mut doc = full_doc(1);
    doc.marketing.push(Marketing {
        references_wp: true,
        after_publication: false,
    });
    assert_eq!(
        doc.check(),
        Err(T8Error::MarketingNotCompliant),
        "T8: marketing communications must reference the published white paper"
    );
}

#[test]
fn t8_the_white_paper_must_carry_the_not_legal_advice_disclaimer() {
    let mut doc = full_doc(1);
    doc.disclaimer = false;
    assert_eq!(
        doc.check(),
        Err(T8Error::NoDisclaimer),
        "T8: the white paper must carry the not-legal-advice disclaimer"
    );
    assert!(
        !aura_storage::whitepaper::T8_NOT_LEGAL_ADVICE.is_empty(),
        "T8: the white paper must carry the not-legal-advice disclaimer"
    );
}

#[test]
fn t8_reference_simulation_agrees_with_the_pinned_digest() {
    let run = run_t8_scenario();
    assert_eq!(
        run.steps.len(),
        77,
        "T8: the reference simulation must agree with the pinned digest"
    );
    let digest = scenario_digest(&run.steps);
    assert_eq!(
        digest, GOLDEN_WP_DIGEST,
        "T8: the reference simulation must agree with the pinned digest"
    );
    let s14 = run.steps[14];
    assert_eq!(
        (s14.kind, s14.subject, s14.state),
        (7u8, 1u32, 3u8),
        "T8: the reference simulation must agree with the pinned digest"
    );
    let s41 = run.steps[41];
    assert_eq!(
        (s41.kind, s41.subject, s41.detail),
        (8u8, 4u32, 4u32),
        "T8: the notification must be at least 20 working days before publication"
    );
    let s76 = run.steps[76];
    assert_eq!(
        (s76.kind, s76.a, s76.b),
        (9u8, 8u32, 1u128),
        "T8: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        (run.total, run.valid),
        (8u32, 1u32),
        "T8: the reference simulation must agree with the pinned digest"
    );
    assert_eq!(
        run.outcomes,
        vec![0, 2, 3, 4, 7, 6, 1, 9],
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
}

#[test]
fn t8_valid_document_passes_the_full_checklist() {
    assert_eq!(
        full_doc(9).check(),
        Ok(()),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
}

#[test]
fn t8_reordering_claims_preserves_the_verification() {
    let mut forward = full_doc(1);
    forward.claims.reverse();
    assert_eq!(
        forward.check(),
        Ok(()),
        "T8: every white paper claim must be traceable to a pinned model"
    );
    let mut broken = full_doc(1);
    broken.claims.reverse();
    broken.claims[2].expected += 1;
    assert_eq!(
        broken.check(),
        Err(T8Error::ClaimUnverified),
        "T8: every white paper claim must be traceable to a pinned model"
    );
}

#[test]
fn t8_checklist_result_is_independent_of_section_order() {
    let mut doc = full_doc(1);
    doc.sections.reverse();
    assert_eq!(
        doc.check(),
        Ok(()),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
    let mut doc = full_doc(1);
    doc.sections.clear();
    doc.draw_section(WpSection::Rights);
    doc.draw_section(WpSection::Risks);
    doc.draw_section(WpSection::Technology);
    assert_eq!(
        doc.check(),
        Err(T8Error::SectionMissing),
        "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"
    );
}
