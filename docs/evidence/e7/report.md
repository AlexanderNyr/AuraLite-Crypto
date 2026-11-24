# E7 report — Lending

Date: 2026-11-22

The result: **ALL E7 GATES GREEN**

Gate: **a 30-day simulation — the `collateral_value >= debt` invariant never
breaks on any day** (2 accounts, 5 price shocks, 16 liquidations,
min health 12530 bps); NC: without liquidations the same path breaks the invariant
on day 18 ("no liquidation → red"); 6 named-message controls.

The `crates/aura-storage/src/lending.rs` model: over-collateralized lending
(Aave/Compound-style): LTV 75%, a liquidation threshold of health 12500 bps
(LTV > 80%, a buffer), a 5% liquidator bonus, close factor 50%, a kinked
rate model (base 2% + slope1 10%·U/kink, kink 80%, slope2 30% above),
daily accrual floor(debt·rate/(365·10000)). 12 named messages.
Decisions D-T1..D-T4.

The simulation `sim/e7-lending.json`, the aggregate digest f8fa28de…bea7dd
(the Rust example ↔ Python `tools/e7_lending_check.py`, `E7-SIM-OK` /
`E7-MODEL-OK` byte-for-byte). The gate `crates/aura-storage/tests/e7_lending.rs`
(14 tests, metamorphic properties: split supply, borrow/repay round-trip,
interest monotonicity, determinism). Negative controls
`tools/nc_e7.py` — 6 controls (`nc.log`, RESULT: 6/6 negative
controls PASS). Sources: blofin.com Aave vs Compound, academy.redduck.io
«Lending and borrowing on-chain», Medium DeFi lending comparative,
GitHub lending-protocol — see `docs/ecosystem/e7-lending-report.md`.
