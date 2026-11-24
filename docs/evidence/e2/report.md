# E2 Report — chain explorer

Date: 2026-11-19

The result: **ALL E2 GATES GREEN**

Gate: 10 user scenarios green; reproducible contract build = same code_hash.

Backend+web UI `crates/aura-storage/src/explorer.rs` (stdlib HTTP, no new dependencies), the gate `crates/aura-storage/tests/e2_explorer.rs` (`campaign/e2-explorer.log`, `campaign/result.json`), the checklist `tools/fixtures/e2_explorer_scenarios.json` (10/10 ready) with the oracle `tools/e2_explorer_check.py` (`baseline.log`) and the independent Python contract builder (`tools/fixtures/e2_contracts.json`).
negative controls `tools/nc_e2.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

