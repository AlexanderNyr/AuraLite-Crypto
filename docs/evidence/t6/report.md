# T6 Report — Governance

Date: 2026-11-21

The result: **ALL T6 GATES GREEN**

Gate: 20 proposals — valid ones pass, a flash-stake attack is rejected.

The `crates/aura-storage/src/governance.rs` model: the proposal threshold 10 000 `ALCRY`; quorum of 2000 bps of the snapshot (the boundary: votes == quorum → passes); the voting period 120 960 blocks (7 days of 5 s), voting is allowed in [created, created+period) — at the boundary it is red; a **48 h timelock** (34 560 blocks) between closing the vote and execution (the boundary: one block earlier — red); voting follows the **snapshot** of power at the moment of creation — a flash-stake after the snapshot gives no power, its vote is rejected (`NoVotingPower`); emergency veto with a **sunset** at block 1 521 000 (the boundary: one block before — it works, at the sunset and after — `VetoSunset`). The scenario `sim/t6-governance.json` (20 proposals, 113 steps: 15 executed, 4 rejected (majority/quorum/tie), 1 vetoed; valid ones pass), the golden digest 828b13cf…c729aa (Rust ↔ Python ↔ the example `t6_governance_example`). The gate `crates/aura-storage/tests/t6_governance.rs` (10 tests + 2 metamorphic properties), the oracle `tools/t6_governance_check.py`.
negative controls `tools/nc_t6.py` (`nc.log`, RESULT: 8/8 negative controls PASS) — 8 controls (the standard 6 + flash-stake + veto sunset, precedent H6–H9/T4/T5).

