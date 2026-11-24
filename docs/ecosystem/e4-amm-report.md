# E4 report — AMM DEX

Date: 2026-11-22. Phase: **E4** (ECOSYSTEM_PLAN). Status: **ALL GATES
GREEN**.

## Summary

A constant-product AMM (x·y = k) with LP tokens, swaps, a slippage guard and
multi-hop routing. **Gate: the property invariant `k` never
decreases** (including routes). **NC: disabling the slippage guard → red.**
The D-E3 order is respected: the DeFi phases go after the oracle (E5) and the audit.

## The `crates/aura-storage/src/amm.rs` model

- `Pool` — an unordered token pair (`Token = [u8; 16]`), reserves and
  LP supply in `Amount = u128` (18 decimals, decision D-T1).
- The curve: `in_fee = floor(amount_in · (10000 − fee_bps) / 10000)`,
  `out = floor(in_fee · reserve_out / (reserve_in + in_fee))` — fee-on-input
  30 bps (0.3%, the Uniswap V2 model), floor rounding on every division.
- 256-bit arithmetic: `wide_mul` (schoolbook 2×2 limbs with carry) and
  `mul_div` (long division over 256 bits, `None` on `u128` overflow).
  The invariant `k = reserve_a · reserve_b` is computed in 256 bits (for
  ordinary reserves `k` does not fit in `u128`).
- LP tokens: the first deposit mints `amount_a + amount_b` shares
  (a documented deviation from Uniswap V2's `sqrt`), then strictly
  proportional deposits `min(a·S/ra, b·S/rb)`; `remove_liquidity` is a
  pro-rata burn with an exact round-trip on the golden values.
- `swap_exact_in(token, amount_in, min_out)` — slippage guard: `out <
  min_out` → the named error `E4: slippage guard: output …`.
- `swap_route(pools, path, amount_in, min_out)` — multi-hop A→B→C,
  checking hop tokens against the pools, `min_out` on the last hop.
- 9 named messages (`E4: …`) — entering invalid states is immediately
  red with the exact string.

## The gate

`crates/aura-storage/tests/e4_amm.rs` — 21 tests, including
`e4_gate_k_never_decreases`: a deterministic sequence of swaps
in both directions with wild sizes + routes — on every pool
`wide_ge(k_after, k_before)` (a wide 256-bit comparison). Plus
`e4_zero_fee_keeps_k` (zero fee → `k` stays exactly unchanged).

## Metamorphisms

- **Split does not beat single** (the concavity of x·y=k): `swap(2A) ≥ swap(1A) +
  swap(1A)` on the same pool — goldens `3910033923564131223 ≥
  3909976989011504086`.
- **Route = the product of hops**: `swap_route([A,B,C]) == the manual
  two-step swap** on untouched pools.
- **Burn round-trip**: mint 50/100 → burn share → exactly (50, 100).
- **Determinism**: two identical runs → identical results.

## Golden values (fee 30bps, floor)

| Scenario | Value |
|---|---|
| 1 A→B from (100/200) | `1974316068794122597` |
| 1 A→B→C via (100/200)+(100/300) | `5791186053878244695` |
| single 2 A | `3910033923564131223` |
| split 1 A + 1 A | `3909976989011504086` |
| initial LP / proportional LP | `300e18` / `150e18` |
| burn round-trip | (50e18, 100e18) exact |

The Rust example `crates/aura-storage/examples/e4_amm_example.rs` ↔ Python
model `tools/e4_amm_check.py` reproduce the scenario `sim/e4-amm.json`
byte-for-byte (the aggregate `E4-SIM-OK`).

## Negative controls

`tools/nc_e4.py` — 6 controls (tools-only, outside the sources): weakening
each named message → red (`RESULT: 6/6 negative controls PASS`).
**Key NC: `bad-slippage` (weakening the slippage guard) → red.**

## Sources

AMM mathematics: Uniswap V2 x·y=k (the whitepaper documentation; the formula
`dy = y·dx/(x+dx)`, a 0.3% fee → 997/1000, MINIMUM_LIQUIDITY,
LP mint/burn), the dev.to articles "AMM Mathematics: Constant Product and
Beyond», autheo.com «Liquidity Pool Mechanics», zealynx.io «How to Build
Uniswap V2 From Scratch". Decisions D-T1..D-T4 (u128/18dec, no income
promises, parameter-change simulations, not legal advice) —
`docs/economics/t8-whitepaper.md`.
