# Отчёт E6 — Stablecoin CDP

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E6 ЗЕЛЁНЫЕ**

Гейт: **200% collateral при −40% цены → ликвидация без bad debt**
(seized 175e18, repaid 100e18, shortfall 0, returned 25e18); property:
peg-инвариант не рвётся (3 vault, 3 ликвидации, два ударных фронта);
NC: ослабление guard-сообщения mint ratio → красный. D-E3 соблюдён
(после оракула E5, аудита H8 и AMM E4).

Модель `crates/aura-storage/src/cdp.rs`: over-collateralized CDP
(MakerDAO-style): min ratio 150% (ETH-A), штраф 5% (USDC-A/Raft),
256-битный `mul_div`, floor-округление; mint/withdraw держат ratio,
ликвидация изымает debt·(1+penalty), остаток — владельцу; peg-инвариант
`bad_debt == 0 && backed_value >= stable_supply`. 12 named messages.
Решения D-T1..D-T4.

Симуляция `sim/e6-cdp.json`, агрегатный дайджест
9b64b653…a499c95 (Rust-пример ↔ Python `tools/e6_cdp_check.py`,
`E6-SIM-OK` / `E6-MODEL-OK` байт-в-байт). Гейт
`crates/aura-storage/tests/e6_cdp.rs` (14 тестов, метаморфизмы split
депозита, mint/repay round-trip, порядок ликвидаций, детерминизм).
Negative controls `tools/nc_e6.py` — 6 контролей (`nc.log`, RESULT:
6/6 negative controls PASS). Источники: MakerDAO vaults/liquidation
(community-development.makerdao.com), chainscorelabs.com CDP engine
architecture, MakerDAO whitepaper explainer — см.
`docs/ecosystem/e6-cdp-report.md`.
