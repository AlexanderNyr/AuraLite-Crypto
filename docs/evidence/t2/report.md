# T2 Report — Allocation

Date: 2026-11-21

The result: **ALL T2 GATES GREEN**

Gate: the script reconciles genesis balances with the table; extra coins → red.

The `crates/aura-storage/src/allocation.rs` table: validators 1 200 000, treasury 600 000 (vesting 6/24 months), community 750 000, team 450 000 (vesting 12/36 months) = 3 000 000 ALCRY = the T1 genesis supply. The golden digest of the table 5fe7417a…c9ea41 (Rust ↔ Python ↔ the example `t2_allocation_example`). Vesting: linear with a cliff; the locked part is not spendable. The scenario `sim/t2-allocation.json`, the gate `crates/aura-storage/tests/t2_allocation.rs` (`campaign/t2-allocation.log`, `campaign/result.json`).
negative controls `tools/nc_t2.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

