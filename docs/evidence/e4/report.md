# E4 report — AMM DEX

Date: 2026-11-22

The result: **ALL E4 GATES GREEN**

Gate: **the `k` invariant never decreases** (256-bit comparison, swaps and
routes); NC: disabling the slippage guard → red. The DeFi phases come after
the oracle (E5) and the audit (D-E3).

The `crates/aura-storage/src/amm.rs` model: a constant-product AMM (x·y = k)
with LP tokens. Fee-on-input 30 bps (0.3%, the Uniswap V2 model), floor rounding
at every division, 256-bit arithmetic (`wide_mul`/`mul_div`), the `k` invariant
in 256 bits. LP: the first deposit is `a + b` (a documented deviation from
sqrt), then strictly proportional `min(a·S/ra, b·S/rb)`, burn — pro-rata
with an exact round-trip. `swap_exact_in` with a slippage guard, `swap_route` —
the A→B→C multi-route. 9 named messages. Decisions D-T1..D-T4.

The simulation `sim/e4-amm.json` (1 A→B from (100/200) = 1974316068794122597;
route 1 A→B→C = 5791186053878244695; single 2A = 3910033923564131223 ≥
split = 3909976989011504086; LP 300e18/150e18, burn (50e18, 100e18) exactly),
the aggregate digest c1286a35…283865 (the Rust example ↔ Python
`tools/e4_amm_check.py`). The gate `crates/aura-storage/tests/e4_amm.rs`
(21 tests, split/route/determinism metamorphic properties). Negative controls
`tools/nc_e4.py` — 6 controls: weakening each named message →
red, restore → green (`nc.log`, RESULT: 6/6 negative controls PASS).
Sources: the Uniswap V2 whitepaper, dev.to "AMM Mathematics", autheo.com,
zealynx.io — see `docs/ecosystem/e4-amm-report.md`.
