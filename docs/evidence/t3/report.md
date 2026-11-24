# T3 Report — Fees

Date: 2026-11-21

The result: **ALL T3 GATES GREEN**

Gate: 10 000 congested blocks — the base fee rises/falls by the formula at every step.

The `crates/aura-storage/src/fees.rs` model: an EIP-1559-like `base_fee` + `tip`; the base part is **burned**, the tip goes to the validator; an elastic gas target of 15 000 000 (a limit 30 000 000, elasticity 2); the formula `base ± max(base·|gas−target|/target/8, 1)`, unchanged when gas == target, with a step of at least 1 base unit. The scenario `sim/t3-fees.json` (segments +25%/−25%/+50%/−50%/target of 250 blocks × 20), the golden path digest 412f1f9e…bc124 (Rust ↔ Python ↔ the example `t3_fees_example`); the totals: burned 2 039 212 976 242 596 352 500 000, tips 150 000 000 000 000 000 000, final base 56 473. The gate `crates/aura-storage/tests/t3_fees.rs` (9 tests + 2 metamorphic properties), the oracle `tools/t3_fees_check.py`.
negative controls `tools/nc_t3.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

