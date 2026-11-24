# E5 Report — Oracle

Date: 2026-11-22

The result: **ALL E5 GATES GREEN**

Gate: **5 providers, 2 malicious → the price does not move**; NC: one
provider → red. The D-E3 order: DeFi (AMM/CDP/lending) — after
the oracle and the audit; E5 closes the oracle.

The `crates/aura-storage/src/oracle.rs` model: a decentralized network of
price feeds. Every provider signs a report (ed25519 over the
canonical message feed16||price16BE||ts8BE); admission is strict
(an unknown provider, a duplicate in the round, a foreign feed, a bad signature,
a stale/future report → named message). Aggregation: quorum N/2+1
(at least 3), the lower median of admitted prices, an outlier band of ±500 bps from
the preliminary median, the final median of the survivors (lower median, and by
timestamp). Constants: Price=u128 (18 decimals), staleness 3600s
(the Chainlink heartbeat), deviation 500bps. Decisions D-E3 (the order: oracle →
audit → DeFi) and the E-phase standard.

The simulation `sim/e5-oracle.json` (5 reports: honest 100.0/100.1/99.9 +
a pump x100 + a dump 0.0001 → price 100.0, 2 outliers dropped), the aggregate
digest aee332b6…dead24 (the Rust example ↔ Python `tools/e5_oracle_check.py`).
The gate `crates/aura-storage/tests/e5_oracle.rs` (14 tests, including
permutation and scaling metamorphic properties). Negative controls
`tools/nc_e5.py` — 6 controls: weakening each named message →
red, restore → green (`nc.log`, RESULT: 6/6 negative controls PASS).
Sources: the Chainlink DON (OCR, quorum N/2+1, heartbeat 3600s, deviation
0.5%), Pyth, UMA — see `docs/ecosystem/e5-oracle-report.md`.
