# T0 Report — Economics baseline

Date: 2026-11-20

The result: **ALL T0 GATES GREEN**

Gate: "emission from nowhere" in the simulation → red; no token exists; the desired properties are described (scarcity, security, accessibility).

The `crates/aura-storage/src/econ.rs` module (a deterministic simulation: xorshift64*, seed 20261180, 1000 steps, the cap-at-balance rule), the `sim/t0-baseline.json` scenario, the gate `crates/aura-storage/tests/t0_economics.rs` (`campaign/t0-economics.log`, `campaign/result.json`), the independent oracle `tools/t0_economics_check.py` (a Python model + cross-running the `t0_sim` example; the golden digest aec12b64…c7a51a).
negative controls `tools/nc_t0.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

