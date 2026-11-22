# T1 — issuance model of `ALCRY`

**Status:** ✅ T1 closed. Decisions D-T1/D-T3 in force: the parameters below
are fixed in genesis and may only be changed by governance with a timelock —
not by a simple commit. Everything is verified by the ten-year simulation in
`crates/aura-storage/src/issuance.rs` (gate: supply matches the formula
within 1 base unit).

## Parameters (`sim/t1-issuance.json`)

| Parameter | Value | Meaning |
|---|---|---|
| unit | `ALCRY`, u128, 18 decimals | D-T1 |
| genesis supply | 3 000 000 `ALCRY` | minted at year 0; distributed by the T0 table (no hidden premine) |
| annual emission (year 1) | 300 000 `ALCRY` | first halving period |
| halving period | 4 years | emission(y) = annual_emission >> ((y−1)/4) |
| max supply | 6 000_000 `ALCRY` | hard cap, fixed ("if fixed" — it is) |
| target staked ratio | 6000 bps (60 %) | declared target for T4 staking; validated, not consumed yet |

## Inflation curve

Disinflation by halving: 300 000 `ALCRY`/year in years 1–4, 150 000 in
years 5–8, 75 000 in years 9–12, … until the cap. The ten-year path:

| year | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| supply, `ALCRY` | 3.0M | 3.3M | 3.6M | 3.9M | 4.2M | 4.35M | 4.5M | 4.65M | 4.8M | 4.875M | 4.95M |

`emission(y)` truncates at the cap: once the supply reaches 6 000 000
`ALCRY`, issuance stops permanently. The cap-binding scenario
(`sim/t1-issuance.json`) proves it: a 3.5M cap halts issuance at exactly
3 500 000 `ALCRY`.

## Verification

* Stepwise simulation vs closed-form formula: **≤ 1 base unit** at every
  year (the phase gate; integer truncation gives 0 drift).
* Golden supply-path digest `c2a4669192ddde5d9cc4e7cb32a76ecd06356022bce7c7230826f068854c1a35`
  pinned in the Rust tests and re-derived by the independent Python oracle
  (`tools/t1_issuance_check.py`), which also cross-runs the Rust example.
* Six mutation controls (`tools/nc_t1.py`): formula mismatch, halving
  removal, cap removal, genesis drift, staked-ratio validation removal and
  horizon truncation each turn the gate red with their named message.

## What T1 deliberately does not do

* No allocation (T2), fees (T3) or staking mechanics (T4). The target
  staked ratio is pinned here only so T4 cannot invent it quietly.
* No legal claims (D-T4).
