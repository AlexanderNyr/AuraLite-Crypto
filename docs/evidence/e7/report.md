# Отчёт E7 — Lending

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E7 ЗЕЛЁНЫЕ**

Гейт: **30-дневная симуляция — инвариант `collateral_value >= debt` не
рвётся ни одного дня** (2 аккаунта, 5 шоков цены, 16 ликвидаций,
min health 12530 bps); NC: без ликвидаций тот же путь рвёт инвариант
на дне 18 («no liquidation → red»); 6 named-message контролей.

Модель `crates/aura-storage/src/lending.rs`: over-collateralized lending
(Aave/Compound-style): LTV 75%, порог ликвидации health 12500 bps
(LTV > 80%, буфер), бонус ликвидатора 5%, close factor 50%, kinked
модель ставок (base 2% + slope1 10%·U/kink, kink 80%, slope2 30% выше),
ежедневное начисление floor(debt·rate/(365·10000)). 12 named messages.
Решения D-T1..D-T4.

Симуляция `sim/e7-lending.json`, агрегатный дайджест f8fa28de…bea7dd
(Rust-пример ↔ Python `tools/e7_lending_check.py`, `E7-SIM-OK` /
`E7-MODEL-OK` байт-в-байт). Гейт `crates/aura-storage/tests/e7_lending.rs`
(14 тестов, метаморфизмы split supply, borrow/repay round-trip,
монотонность процентов, детерминизм). Negative controls
`tools/nc_e7.py` — 6 контролей (`nc.log`, RESULT: 6/6 negative
controls PASS). Источники: blofin.com Aave vs Compound, academy.redduck.io
«Lending and borrowing on-chain», Medium DeFi lending comparative,
GitHub lending-protocol — см. `docs/ecosystem/e7-lending-report.md`.
