# E6 report — Stablecoin CDP

Date: 2026-11-22

The result: **ALL E6 GATES GREEN**

Gate: **200% collateral at a −40% price move → liquidation with no bad debt**
(seized 175e18, repaid 100e18, shortfall 0, returned 25e18); property:
the peg invariant never breaks (3 vaults, 3 liquidations, two shock fronts);
NC: weakening the mint-ratio guard message → red. D-E3 is observed
(after the oracle E5, the audit H8 and the AMM E4).

The `crates/aura-storage/src/cdp.rs` model: an over-collateralized CDP
(MakerDAO-style): min ratio 150% (ETH-A), a 5% penalty (USDC-A/Raft),
256-bit `mul_div`, floor rounding; mint/withdraw keep the ratio,
liquidation takes debt·(1+penalty), the remainder goes to the owner; the peg invariant
`bad_debt == 0 && backed_value >= stable_supply`. 12 named messages.
Decisions D-T1..D-T4.

The simulation `sim/e6-cdp.json`, the aggregate digest
9b64b653…a499c95 (the Rust example ↔ Python `tools/e6_cdp_check.py`,
`E6-SIM-OK` / `E6-MODEL-OK` byte-for-byte). The gate
`crates/aura-storage/tests/e6_cdp.rs` (14 tests, metamorphic properties: split
deposit, mint/repay round-trip, liquidation order, determinism).
Negative controls `tools/nc_e6.py` — 6 controls (`nc.log`, RESULT:
6/6 negative controls PASS). Sources: MakerDAO vaults/liquidation
(community-development.makerdao.com), chainscorelabs.com CDP engine
architecture, a MakerDAO whitepaper explainer — see.
`docs/ecosystem/e6-cdp-report.md`.
