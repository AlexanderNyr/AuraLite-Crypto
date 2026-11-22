# Отчёт T1 — Issuance model

Дата: 2026-11-21

Итог: **ВСЕ ГЕЙТЫ T1 ЗЕЛЁНЫЕ**

Гейт: 10-летняя симуляция — supply совпадает с формулой в пределах 1 базовой единицы.

Модель `crates/aura-storage/src/issuance.rs`: genesis 3 000 000 ALCRY, годовая эмиссия 300 000 ALCRY с халвингом каждые 4 года, потолок 6 000 000 ALCRY, целевая доля стейка 6000 bps. Траектория 10 лет: 3.0M→4.95M ALCRY; золотой дайджест пути c2a46691…c1a35 (Rust ↔ Python эталон ↔ пример `t1_issuance_example`). Сценарий `sim/t1-issuance.json`, гейт `crates/aura-storage/tests/t1_issuance.rs` (`campaign/t1-issuance.log`, `campaign/result.json`).
negative controls `tools/nc_t1.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

