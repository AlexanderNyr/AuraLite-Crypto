# E3 Report — Faucet + SDK

Date: 2026-11-20

The result: **ALL E3 GATES GREEN**

Gate: 1000 requests: 1 payout/address/day; SDK examples pass in CI.

Faucet `crates/aura-storage/src/faucet.rs` (a deterministic policy: 1 payout/address/day, testnet only) + the client SDK `crates/aura-storage/src/sdk.rs`, `sdk/ts/`, `sdk/python/`; the gate `crates/aura-storage/tests/e3_faucet.rs` (`campaign/e3-faucet.log`, `campaign/result.json`), the oracle `tools/e3_faucet_check.py` (an independent Python model, `tools/fixtures/e3_faucet_script.json`: 1000 requests → 750 payouts, 250 rejections), the SDK examples — `tools/e3_examples_check.py` (`examples.log`: rust/ts/python 3/3).
negative controls `tools/nc_e3.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

