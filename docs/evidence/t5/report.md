# T5 Report — Slashing and insurance

Date: 2026-11-21

The result: **ALL T5 GATES GREEN**

Gate: 7 scenarios — penalty >= damage, the insurance fund never goes negative.

The `crates/aura-storage/src/slashing.rs` model: an incident causes `damage`; the offender's penalty = damage × 15000 bps (the penalty parameters, penalty >= damage always); victims are covered from the insurance fund (`covered = min(damage, balance)`, the remainder is `shortfall`); the fund is replenished by 5000 bps of the applied penalty; the mass-slash protection — no window can destroy more than 500 bps of registered bonded (1 000 000 `ALCRY` = 50 000 `ALCRY`/window), excess penalty is DEFERRED (never forgiven) and collected by the following windows. The scenario `sim/t5-slashing.json` (7 scenarios, 87 steps, 48 incidents: a single double-sign, a repeat offender, a correlated mass-slash, fund exhaustion, penalty floor edges, per-window caps, an exhaustion-and-replenishment chain), the golden digest 4cc9ea03…7c9cef (Rust ↔ Python ↔ the `t5_slashing_example` example); the finale: fund 536 750 `ALCRY`, the deferred queue is empty (total_applied == total_penalty == 1 790 402 999 999 999 999 999 999), covered 558 451 499 999 999 999 999 998, uncovered 635 150 500 000 000 000 000 002. The gate `crates/aura-storage/tests/t5_slashing.rs` (10 tests + 2 metamorphic properties), the oracle `tools/t5_slashing_check.py`.
negative controls `tools/nc_t5.py` (`nc.log`, RESULT: 7/7 negative controls PASS) — 7 controls (the standard 6 + the mass-slash cap, precedent H6–H9/T4).

