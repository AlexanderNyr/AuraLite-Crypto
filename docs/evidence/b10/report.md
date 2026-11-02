# CONSENSUS B10 Close-out Report — Safety Evidence & `CONSENSUS_PLAN` Completion

- **Phase:** `B10` (`CONSENSUS_PLAN.md` — closes `B0`–`B10`)
- **Patch:** `patches/0057-B10-safety-evidence.patch`
- **Generated (UTC):** `2026-11-02T06:44:59+00:00`

## Summary
1. **`aura_consensus::safety_model` & `HotStuff3ChainSafety.tla`**:
   - `BftModelConfig`, `BftSafetyModelChecker`, `CounterexampleTrace`, `HundredNodeSimulationConfig`, `run_hundred_node_simulation`, `LongRangeAttackScenario`, and `analyze_long_range_attack`.
2. **Test Gate (`model: safety for f < n/3; NC: remove quorum check -> model finds a counterexample`)**:
   - Verified in `safety_model::tests::model_proves_safety_for_f_less_than_n_over_3`, `safety_model::tests::model_finds_counterexample_when_quorum_check_is_removed_or_f_reaches_one_third`, and `crates/aura-consensus/tests/b10_safety_evidence.rs` (plus 64 deterministic vectors in `b10_safety_model.txt`).
3. **Negative Controls**:
   - `tools/nc_b10.py`: 6/6 library unit-test mutations PASS (`333/333` cumulative).
