# E6 report — Stablecoin CDP

Date: 2026-11-22. Phase: **E6** (ECOSYSTEM_PLAN). Status: **ALL GATES
GREEN**.

## Summary

An over-collateralized CDP (MakerDAO-style vaults): collateral deposit, minting
a USD stablecoin, repayment, withdrawal, liquidations and peg stress. **Gate:
200% collateral at −40% price → liquidation with no bad debt.** Property:
the peg invariant (every unit of stablecoin is backed by ≥ $1 of collateral,
`bad_debt == 0`) never breaks on any step of the stress sequence.
**NC: weakening the mint-ratio guard message (and 5 more named messages) →
red.** D-E3 respected: CDP goes after the oracle (E5), the H8 audit and AMM
(E4).

## The `crates/aura-storage/src/cdp.rs` model

- `CdpEngine` — a vault registry, an oracle price (USD per 1 unit of
  collateral, 18 decimals), stablecoin accounting (`stable_supply`) and
  `bad_debt`. `Amount`/`Price` — `u128` 18 decimals (D-T1).
- Risk parameters: **min ratio 150%** (`E6_MIN_RATIO_BPS = 15000`,
  the MakerDAO ETH-A liquidation ratio) and a **5% penalty**
  (`E6_LIQ_PENALTY_BPS = 500`, the USDC-A/Raft precedent). All products
  go through the 256-bit `mul_div` from `amm.rs` (floor on every division).
- Operations: `open_vault`, `deposit`, `mint` (ratio after ≥ 150%),
  `repay`, `withdraw` (ratio after ≥ 150%), `set_price` (peg stress),
  `liquidate`. The ratio `ratio_bps = value·10000/debt`,
  `value = collateral·price/1e18`.
- Liquidation: collateral covering `debt·(1+penalty)` at the current price is seized
  (`seized = floor(penalty_debt·1e18/price)`, capped by the collateral),
  `repaid = min(debt, seized·price/1e18)` is burned, the rest of the collateral goes
  to the vault owner; a (theoretical) shortfall is written to `bad_debt`.
  A healthy vault is not liquidated.
- The peg invariant `peg_holds()`: `bad_debt == 0 && backed_value() >=
  stable_supply`. 12 named messages (`E6: …`).

## The gate

`crates/aura-storage/tests/e6_cdp.rs` — 14 tests:

- `e6_gate_200pct_minus40_liquidates_no_bad_debt` — **the literal gate
  of the plan**: vault 200/100 (200%), price 0.6 (−40%) → ratio 12000 bps →
  liquidation: seized `175e18`, repaid `100e18`, shortfall `0`,
  returned `25e18`; `bad_debt == 0`, `peg_holds`.
- `e6_gate_peg_backing_never_breaks` — property: three vaults
  (200/220/300 against a debt of 100), shocks 0.9/0.6/0.45, three liquidations —
  at every step `bad_debt == 0` and `backed_value ≥ stable_supply`.

## Metamorphisms

- **Deposit split**: `deposit(40)+deposit(60)` ≡ `deposit(100)` (the
  same state and ratio).
- **Mint→repay round-trip** — an exact return to the initial state and
  supply.
- **Liquidation order**: liquidating two vaults in any order →
  identical system totals.
- **Determinism**: two runs → identical results.

## Golden values (min 15000 bps, penalty 500 bps, floor)

| Scenario | Value |
|---|---|
| ratio after −40% | `12000` bps |
| seized / repaid / shortfall / returned (gate) | `175e18` / `100e18` / `0` / `25e18` |
| seized at price 0.45 (300/100) | `233333333333333333333` |
| returned at price 0.45 | `66666666666666666667` |
| max mint under 200e18 (the 15000 bps boundary) | `133333333333333333333` (+1 → reject `14999/15000`) |
| max withdraw from 200/100 | `50e18` (+1 → reject `14999/15000`) |

The Rust example `crates/aura-storage/examples/e6_cdp_example.rs` ↔ Python
model `tools/e6_cdp_check.py` reproduce `sim/e6-cdp.json`
byte-for-byte (`E6-SIM-OK` / `E6-MODEL-OK`).

## Negative controls

`tools/nc_e6.py` — 6 controls (tools-only): weakening each target
named message → red, restore → green (`RESULT: 6/6 negative
controls PASS`). **Key NC: `bad-mint-ratio`** (the mint guard).

## Sources

MakerDAO: vault/CDP, liquidation ratio 150% (ETH-A), liquidation penalty
13% (ETH-A) / 5% (USDC-A), the stability fee as a peg instrument; seizure
covers debt + penalty, the remainder goes to the owner; "this is a critical
safety mechanism that protects the DAI stablecoin's peg by ensuring all
minted DAI remains fully backed» (community-development.makerdao.com).
Risk parameters and the liquidation-engine design — chainscorelabs.com
«How to Architect a Collateralized Debt Position Engine» (MakerDAO vs
Liquity vs Raft: min ratio 170/110/120%, penalty 13/0/5%); Raft 120%/5%
as the 5% penalty precedent. Decisions D-T1..D-T4 — `docs/economics/t8-whitepaper.md`.
