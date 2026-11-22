# Отчёт T2 — Allocation

Дата: 2026-11-21

Итог: **ВСЕ ГЕЙТЫ T2 ЗЕЛЁНЫЕ**

Гейт: скрипт сверяет genesis-балансы с таблицей; лишние монеты → красный.

Таблица `crates/aura-storage/src/allocation.rs`: validators 1 200 000, treasury 600 000 (вестинг 6/24 мес), community 750 000, team 450 000 (вестинг 12/36 мес) = 3 000 000 ALCRY = genesis supply T1. Золотой дайджест таблицы 5fe7417a…c9ea41 (Rust ↔ Python ↔ пример `t2_allocation_example`). Вестинг: линейный с клиффом; locked-часть не тратится. Сценарий `sim/t2-allocation.json`, гейт `crates/aura-storage/tests/t2_allocation.rs` (`campaign/t2-allocation.log`, `campaign/result.json`).
negative controls `tools/nc_t2.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

