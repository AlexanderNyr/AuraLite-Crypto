# T4 Report — Staking

Date: 2026-11-21

The result: **ALL T4 GATES GREEN**

Gate: 1000 delegators — the shares sum to exactly one pool at every step; a withdrawal without unbonding — red.

The `crates/aura-storage/src/staking.rs` model: min stake 1 000 `ALCRY`; delegation via shares (compounding rewards changes the share price, but not the count); the validator fee 1000 bps **from rewards only** (D-T2); unbonding of 21 days (362 880 blocks at 5 s) — an early withdrawal is red (`StillUnbonding`), without unbonding — red (`NoUnbonding`); slashing period = the unbonding window — bonded and unclaimed unbonding are slashed proportionally. The scenario `sim/t4-staking.json` (2004 steps: 1000 delegations, 600 rewards, slashes of 100/500 bps, partial undelegates, a refusal one block before maturity, withdrawals), the golden digest e80662c8…87f07 (Rust ↔ Python ↔ the example `t4_staking_example`); the finale: bonded 1 325 889 559 769 089 696 565 650, shares 1 408 708 333 333 333 333 333 367 (== Σ shares), a commission of 120 `ALCRY`-million units, withdrawn 85 430 770 230 910 303 434 449. The gate `crates/aura-storage/tests/t4_staking.rs` (10 tests + 2 metamorphic properties), the oracle `tools/t4_staking_check.py`.
negative controls `tools/nc_t4.py` (`nc.log`, RESULT: 7/7 negative controls PASS) — 7 controls (the standard 6 + slash proportionality, precedent H6–H9).

