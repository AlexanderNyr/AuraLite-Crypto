# Отчёт T0 — Economics baseline

Дата: 2026-11-20

Итог: **ВСЕ ГЕЙТЫ T0 ЗЕЛЁНЫЕ**

Гейт: «emission from nowhere» в симуляции → красный; токена нет; желаемые свойства описаны (scarcity, security, accessibility).

Модуль `crates/aura-storage/src/econ.rs` (детерминированная симуляция: xorshift64*, сид 20261180, 1000 шагов, правило cap-at-balance), сценарий `sim/t0-baseline.json`, гейт `crates/aura-storage/tests/t0_economics.rs` (`campaign/t0-economics.log`, `campaign/result.json`), независимый оракул `tools/t0_economics_check.py` (Python-модель + кросс-запуск примера `t0_sim`; золотой дайджест aec12b64…c7a51a).
negative controls `tools/nc_t0.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

