# Отчёт T4 — Staking

Дата: 2026-11-21

Итог: **ВСЕ ГЕЙТЫ T4 ЗЕЛЁНЫЕ**

Гейт: 1000 делегаторов — shares суммируются ровно в один пул на каждом шаге; вывод без unbonding — красный.

Модель `crates/aura-storage/src/staking.rs`: min stake 1 000 `ALCRY`; делегация через shares (компаундинг наград меняет цену share, но не количество); комиссия валидатора 1000 bps **только из наград** (D-T2); unbonding 21 день (362 880 блоков при 5 с) — вывод до срока красный (`StillUnbonding`), без unbonding — красный (`NoUnbonding`); slashing period = окно unbonding — bonded и незабранный unbonding режутся пропорционально. Сценарий `sim/t4-staking.json` (2004 шага: 1000 делегаций, 600 наград, слэши 100/500 bps, частичные undelegate, отказ за блок до зрелости, выводы), золотой дайджест e80662c8…87f07 (Rust ↔ Python ↔ пример `t4_staking_example`); финал: bonded 1 325 889 559 769 089 696 565 650, shares 1 408 708 333 333 333 333 333 367 (== Σ shares), комиссия 120 `ALCRY`-млн-ед., выведено 85 430 770 230 910 303 434 449. Гейт `crates/aura-storage/tests/t4_staking.rs` (10 тестов + 2 метаморфизма), оракул `tools/t4_staking_check.py`.
negative controls `tools/nc_t4.py` (`nc.log`, RESULT: 7/7 negative controls PASS) — 7 контролей (стандарт 6 + slash-пропорциональность, прецедент H6–H9).

