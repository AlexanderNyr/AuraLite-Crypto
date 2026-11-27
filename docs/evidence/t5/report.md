# Отчёт T5 — Slashing and insurance

Дата: 2026-11-21

Итог: **ВСЕ ГЕЙТЫ T5 ЗЕЛЁНЫЕ**

Гейт: 7 сценариев — penalty >= damage, фонд страхования никогда не уходит в минус.

Модель `crates/aura-storage/src/slashing.rs`: инцидент причиняет `damage`; штраф обидчика = damage × 15000 bps (параметры штрафа, penalty >= damage всегда); пострадавшие покрываются из страхового фонда (`covered = min(damage, balance)`, остаток — `shortfall`); фонд пополняется на 5000 bps от применённого штрафа; mass-slash защита — окно не может уничтожить больше 500 bps зарегистрированного bonded (1 000 000 `ALCRY` = 50 000 `ALCRY`/окно), избыток штрафа ОТКЛАДЫВАЕТСЯ (не прощается) и собирается следующими окнами. Сценарий `sim/t5-slashing.json` (7 сценариев, 87 шагов, 48 инцидентов: единичный double-sign, повторный нарушитель, коррелированный mass-slash, истощение фонда, floor-края штрафа, cap по окнам, цепочка истощения и пополнения), золотой дайджест 4cc9ea03…7c9cef (Rust ↔ Python ↔ пример `t5_slashing_example`); финал: фонд 536 750 `ALCRY`, очередь отложенных пуста (total_applied == total_penalty == 1 790 402 999 999 999 999 999 999), покрыто 558 451 499 999 999 999 999 998, непокрыто 635 150 500 000 000 000 000 002. Гейт `crates/aura-storage/tests/t5_slashing.rs` (10 тестов + 2 метаморфизма), оракул `tools/t5_slashing_check.py`.
negative controls `tools/nc_t5.py` (`nc.log`, RESULT: 7/7 negative controls PASS) — 7 контролей (стандарт 6 + mass-slash cap, прецедент H6–H9/T4).

