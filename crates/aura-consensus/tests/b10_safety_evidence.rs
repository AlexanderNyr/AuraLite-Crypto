//! B10 integration gate:
//! 1. TLA+/Apalache specification (`docs/evidence/b10/HotStuff3ChainSafety.tla`)
//!    and bounded model checker (`model: safety for f < n/3; NC: remove quorum
//!    check -> model finds a counterexample`).
//! 2. 100-node BFT simulation (`n = 100, f = 33, quorum = 67`) across 50 rounds
//!    with 33 Byzantine equivocators and zero finalized-head divergence.
//! 3. Verification of all 64 deterministic safety-model and long-range vectors
//!    in `tests/fixtures/b10_safety_model.txt`.

use std::collections::BTreeMap;

use aura_consensus::safety_model::{
    BftModelConfig, BftSafetyModelChecker, HundredNodeSimulationConfig, LongRangeAttackScenario,
    ModelCheckResult, TLA_SPEC_SOURCE, analyze_long_range_attack, run_hundred_node_simulation,
};

const SAFETY_VECTORS: &str = include_str!("fixtures/b10_safety_model.txt");
const TLA_SPEC_FILE: &str = include_str!("../../../docs/evidence/b10/HotStuff3ChainSafety.tla");

#[test]
fn b10_model_safety_for_f_less_than_n_over_3_and_counterexample_without_quorum_check() {
    // 1. Pinned TLA+/Apalache specification in docs/evidence/b10/ matches embedded spec.
    assert_eq!(TLA_SPEC_FILE, TLA_SPEC_SOURCE);

    // 2. Model proves safety for f < n/3 across small, medium, and 100-node validator sets.
    for (n, f) in [(4, 1), (7, 2), (10, 3), (13, 4), (31, 10), (100, 33)] {
        let cfg = BftModelConfig::new(n, f, 5);
        let result = BftSafetyModelChecker::check(&cfg);
        assert!(
            result.is_safe(),
            "expected Safe for n={n}, f={f} (f < n/3), got {result:?}"
        );
    }

    // 3. Removing the quorum check produces an explicit counterexample trace.
    let mut no_quorum = BftModelConfig::new(4, 1, 5);
    no_quorum.enforce_quorum_check = false;
    match BftSafetyModelChecker::check(&no_quorum) {
        ModelCheckResult::Counterexample(trace) => {
            assert_ne!(trace.committed_hash_a, trace.committed_hash_b);
            assert_eq!(trace.steps.len(), 6);
        }
        ModelCheckResult::Safe { .. } => {
            panic!("removing quorum check must produce a counterexample")
        }
    }

    // 4. 100-node simulation (n = 100, f = 33, quorum = 67) reaches consensus with 0 divergence.
    let sim = run_hundred_node_simulation(&HundredNodeSimulationConfig::default());
    assert_eq!(sim.node_count, 100);
    assert_eq!(sim.honest_count, 67);
    assert_eq!(sim.byzantine_count, 33);
    assert_eq!(sim.quorum_threshold, 67);
    assert_eq!(sim.conflicting_finalized_nodes, 0);
    assert_eq!(sim.finalized_height, 48);
}

#[test]
fn b10_safety_model_vector_fixture_matches_deterministic_rules() {
    let mut count = 0usize;
    for raw in SAFETY_VECTORS.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = BTreeMap::new();
        for token in line.split_whitespace() {
            let (k, v) = token.split_once('=').expect("key=value");
            fields.insert(k, v);
        }

        let n: u64 = fields["n"].parse().unwrap();
        let f: u64 = fields["f"].parse().unwrap();
        let enforce_quorum: u8 = fields["enforce_quorum"].parse().unwrap();
        let views: u64 = fields["views"].parse().unwrap();
        let expected_safe: u8 = fields["expected_safe"].parse().unwrap();
        let expected_overlap: u64 = fields["min_honest_overlap"].parse().unwrap();

        let head_h: u64 = fields["head_h"].parse().unwrap();
        let ckpt_h: u64 = fields["ckpt_h"].parse().unwrap();
        let window: u64 = fields["window"].parse().unwrap();
        let fork_h: u64 = fields["fork_h"].parse().unwrap();
        let hist_stake_bps: u16 = fields["hist_stake_bps"].parse().unwrap();
        let enforce_ws: u8 = fields["enforce_ws"].parse().unwrap();
        let expected_lr_verdict = fields["expected_lr_verdict"];

        let cfg = BftModelConfig {
            total_validators: n,
            byzantine_validators: f,
            max_views: views,
            enforce_quorum_check: enforce_quorum == 1,
        };
        let res = BftSafetyModelChecker::check(&cfg);
        if expected_safe == 1 {
            match res {
                ModelCheckResult::Safe {
                    min_honest_quorum_overlap,
                    ..
                } => {
                    assert_eq!(
                        min_honest_quorum_overlap, expected_overlap,
                        "case {}",
                        fields["case_id"]
                    );
                }
                ModelCheckResult::Counterexample(ce) => {
                    panic!(
                        "case {} expected Safe, got Counterexample: {ce}",
                        fields["case_id"]
                    );
                }
            }
        } else {
            assert!(
                matches!(res, ModelCheckResult::Counterexample(_)),
                "case {} expected Counterexample",
                fields["case_id"]
            );
        }

        let scenario = LongRangeAttackScenario {
            current_head_height: head_h,
            finalized_checkpoint_height: ckpt_h,
            unbonding_window_blocks: window,
            fork_origin_height: fork_h,
            compromised_historical_stake_bps: hist_stake_bps,
            enforce_weak_subjectivity: enforce_ws == 1,
        };
        let verdict = analyze_long_range_attack(&scenario);
        assert_eq!(
            verdict.as_str(),
            expected_lr_verdict,
            "case {}",
            fields["case_id"]
        );

        count += 1;
    }

    assert_eq!(count, 64, "expected 64 B10 safety-model vectors");
}
