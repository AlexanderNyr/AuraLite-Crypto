# T1 Report — Issuance model

Date: 2026-11-21

The result: **ALL T1 GATES GREEN**

Gate: a 10-year simulation — the supply matches the formula within 1 base unit.

The `crates/aura-storage/src/issuance.rs` model: genesis 3 000 000 ALCRY, annual emission of 300 000 ALCRY with a halving every 4 years, a cap of 6 000 000 ALCRY, target stake share 6000 bps. The 10-year trajectory: 3.0M→4.95M ALCRY; the golden path digest c2a46691…c1a35 (Rust ↔ Python reference ↔ the `t1_issuance_example` example). The scenario `sim/t1-issuance.json`, the gate `crates/aura-storage/tests/t1_issuance.rs` (`campaign/t1-issuance.log`, `campaign/result.json`).
negative controls `tools/nc_t1.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

