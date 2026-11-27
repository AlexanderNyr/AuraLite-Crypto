# Отчёт E1 — индексер цепи

Дата: 2026-11-19

Итог: **ВСЕ ГЕЙТЫ E1 ЗЕЛЁНЫЕ**

Гейт: 10 000 блоков + искусственный reorg → 0 дублей/потерь; NC: без rollback → red.

Код `crates/aura-storage/src/indexer.rs` (идемпотентный ingest + reorg rollback над локальным хранилищем — отклонение вместо Postgres зафиксировано в ECOSYSTEM_PLAN); гейт-тест `crates/aura-storage/tests/e1_indexer.rs` (`campaign/indexer.log`, `campaign/result.json`).
negative controls `tools/nc_e1.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

