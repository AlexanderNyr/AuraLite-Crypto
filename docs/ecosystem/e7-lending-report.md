# E7 report — Lending

Date: 2026-11-22. Phase: **E7** (ECOSYSTEM_PLAN). Status: **ALL GATES
GREEN**.

## Summary

An over-collateralized lending market (Aave/Compound style): supply,
borrow against collateral (LTV 75%), a kinked interest-rate model on utilization,
interest accrual, liquidations (health < 125%, 5% bonus, close
factor 50%). **Gate: a 30-day simulation — the invariant
`collateral_value >= debt` never breaks on any day.** **NC: without
liquidations the same path breaks the invariant on day 18 → red** ("no
liquidation → red"), plus 6 named-message controls. D-E3 respected.

## The `crates/aura-storage/src/lending.rs` model

- `LendingMarket` — positions (collateral/debt), an oracle price, daily
  accrual. `Amount`/`Price` are `u128` 18 decimals (D-T1), all
  products go through the 256-bit `mul_div` (floor), `mul_div_ceil` for
  capacity boundaries.
- Parameters (documented precedents): **LTV 75%** (borrow cap,
  Aave style), **the liquidation threshold health 12500 bps** (LTV > 80%; the gap
  between LTV and threshold is a price buffer), **liquidator bonus 5%** (Aave ETH),
  **close factor 50%** (Aave). The rate is the Compound/Aave kinked model:
  `base 2% + slope1 10% · U/kink` below the kink (80%), `+ slope2 30%` above;
  accrual `floor(debt·rate/(365·10000))` per day per account.
- Operations: `open_account`, `supply`, `withdraw` (capacity check),
  `borrow` (`capacity = value·LTV/10000 − debt`), `repay`, `set_price`,
  `advance_day`, `liquidate` (repay ≤ 50% of the debt, seize =
  `floor(repaid·(1+bonus)·ONE/(price·BPS))`). Health =
  `value·10000/debt`. 12 named messages (`E7: …`).

## The gate

`crates/aura-storage/tests/e7_lending.rs` — 14 tests:

- `e7_gate_30day_sim_collateral_never_below_debt` — **the literal gate
  of the plan**: 2 accounts (1000/700 and 500/300), 5 price shocks
  (0.85/0.72/0.61/0.52/0.44 on days 6/12/18/24/30), 30 days of interest,
  16 liquidations — **every day `value >= debt` on every account**;
  min health 12530 bps; day-30 debts `690912489586789599` /
  `4737685642880842971`; total interest `4011969083602784267`.
- `e7_gate_no_liquidation_breaks_invariant` — **the functional NC**:
  the same path without liquidations breaks the invariant **on day 18** →
  «no liquidation → red».

## Metamorphisms

- **Split supply**: `supply(40)+supply(60)` ≡ `supply(100)`.
- **Borrow→repay round-trip**: after full repayment a repeated borrow
  returns the exact state.
- **Interest monotonicity**: 5 days of accrual — the debt strictly grows.
- **Determinism**: two runs → identical results.

## Golden values (Python model ↔ Rust byte-for-byte)

| Scenario | Value |
|---|---|
| rate at U=0 / 6666 / 8000 / 9000 bps | `200` / `1033` / `1200` / `2700` |
| borrow capacity (1000/700) | `50e18` (+1 → reject) |
| max withdraw (1000/700) | `66666666666666666666` (+1 → reject, health after = 13333) |
| liquidations in the simulation | `16` (first: day 6, repay `350609153368496978254`) |
| day 30 (A, B) | `690912489586789599` / `4737685642880842971` |
| total interest / min health | `4011969083602784267` / `12530` bps |
| break without liquidations | day `18` |

The Rust example `crates/aura-storage/examples/e7_lending_example.rs` ↔
Python `tools/e7_lending_check.py` reproduce `sim/e7-lending.json`
(`E7-SIM-OK` / `E7-MODEL-OK`, digest `f8fa28de…bea7dd`).

## Negative controls

`tools/nc_e7.py` — 6 controls (tools-only): weakening each target
named message → red, restore → green (`RESULT: 6/6 negative
controls PASS`). **Key NCs: `bad-borrow-capacity` (the LTV guard) and
the functional "no liquidation → red"** (the gate test uses
liquidations as load-bearing for the invariant: chains of 2–4 liquidations per
shock — the close factor 50% restores health to ≥ 12500).

## Sources

Aave vs Compound (blofin.com): Health Factor = collateral·threshold/debt,
the liquidation threshold above the LTV (a buffer), close factor 50%, bonus 5% (ETH) /
15% (YFI), the kinked rate model (kink ~80%, "rates spike past optimal
utilisation»); academy.redduck.io «Lending and borrowing on-chain»:
a kink model with base/slope, `U* = 80%`, liquidations with a bonus,
«once collateral value falls below debt × bonus, paying the liquidation
bonus mathematically worsens the health factor" (explaining the chains of
liquidations at the boundary); Medium "Comparative Analysis of DeFi Lending"
(Compound/Aave kinked utilization function); GitHub lending-protocol
(the LendingPool/DefaultInterestRateStrategy structure,
`U ≤ U*: base + slope1·U/U*`). Decisions D-T1..D-T4 —
`docs/economics/t8-whitepaper.md`.
