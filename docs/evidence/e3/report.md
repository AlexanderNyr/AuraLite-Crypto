# Отчёт E3 — Faucet + SDK

Дата: 2026-11-20

Итог: **ВСЕ ГЕЙТЫ E3 ЗЕЛЁНЫЕ**

Гейт: 1000 requests: 1 payout/address/day; SDK examples pass in CI.

Faucet `crates/aura-storage/src/faucet.rs` (детерминированная политика: 1 выплата/адрес/день, testnet only) + клиентский SDK `crates/aura-storage/src/sdk.rs`, `sdk/ts/`, `sdk/python/`; гейт `crates/aura-storage/tests/e3_faucet.rs` (`campaign/e3-faucet.log`, `campaign/result.json`), оракул `tools/e3_faucet_check.py` (независимая Python-модель, `tools/fixtures/e3_faucet_script.json`: 1000 запросов → 750 выплат, 250 отказов), примеры SDK — `tools/e3_examples_check.py` (`examples.log`: rust/ts/python 3/3).
negative controls `tools/nc_e3.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

