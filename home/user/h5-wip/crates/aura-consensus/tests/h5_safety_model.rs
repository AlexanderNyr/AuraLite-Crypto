//! H5 consensus safety gates (docs/h5-consensus-plan.md §2).
//!
//! The executable model must prove safety for f < n/3 and — the phase gate —
//! must FIND the counterexample the moment the 2f+1 quorum is removed. The
//! TLA+ spec must not drift from the pinned source.
//! `tools/nc_h5.py` injects the bugs these tests must catch.

use aura_consensus::safety_model::{
    BftModelConfig, BftSafetyModelChecker, ModelCheckResult, TLA_SPEC_SOURCE,
};

const SPEC_FILE: &str = include_str!("../../../docs/evidence/b10/HotStuff3ChainSafety.tla");
const MC_SPEC_FILE: &str = include_str!("../../../docs/h5/HotStuff3ChainSafetyMC.tla");

#[test]
fn h5_model_safety_and_counterexample() {
    // Safety for f < n/3 across the validator-set table (B10 table).
    for (n, f) in [(4u64, 1u64), (7, 2), (10, 3), (13, 4), (31, 10), (100, 33)] {
        let config = BftModelConfig::new(n, f, 3);
        match BftSafetyModelChecker::check(&config) {
            ModelCheckResult::Safe {
                min_honest_quorum_overlap,
                ..
            } => {
                assert!(
                    min_honest_quorum_overlap >= 1,
                    "H5: safety must hold for f < n/3 ({n},{f})"
                );
            }
            ModelCheckResult::Counterexample(trace) => {
                panic!("H5: safety must hold for f < n/3 ({n},{f}): {trace}")
            }
        }
    }

    // THE GATE: remove the 2f+1 quorum check — the model must find a
    // counterexample and name the double-signing offenders.
    for (n, f) in [(4u64, 1u64), (7, 2), (10, 3)] {
        let mut config = BftModelConfig::new(n, f, 3);
        config.enforce_quorum_check = false;
        match BftSafetyModelChecker::check(&config) {
            ModelCheckResult::Counterexample(trace) => {
                assert!(
                    !trace.equivocating_validators.is_empty(),
                    "H5: accountable safety extraction must name offenders"
                );
                let offenders = BftSafetyModelChecker::extract_conflicting_qc_offenders(
                    &trace.quorum_branch_a,
                    &trace.quorum_branch_b,
                );
                assert!(
                    !offenders.is_empty(),
                    "H5: accountable safety extraction must name offenders"
                );
            }
            ModelCheckResult::Safe { .. } => {
                panic!("H5: model must find a counterexample when 2f+1 is removed ({n},{f})")
            }
        }
    }
}

#[test]
fn h5_tla_spec_integrity() {
    // The executable spec file and the pinned source must stay identical.
    assert_eq!(SPEC_FILE, TLA_SPEC_SOURCE, "H5: TLA spec drift");
    // The 2f+1 threshold and the accountable-safety invariant stay in the spec.
    assert!(
        TLA_SPEC_SOURCE.contains("QuorumThreshold == 2 * F + 1"),
        "H5: TLA spec drift"
    );
    assert!(
        TLA_SPEC_SOURCE.contains("AccountableSafetyInvariant"),
        "H5: TLA spec drift"
    );
    // The model-checkable spec keeps both threshold shapes wired to the
    // WeakenQuorum switch.
    assert!(
        MC_SPEC_FILE.contains("IF WeakenQuorum THEN F + 1 ELSE 2 * F + 1"),
        "H5: TLA spec drift"
    );
    assert!(
        MC_SPEC_FILE.contains("AccountableSafety"),
        "H5: TLA spec drift"
    );
}
