# Отчёт T3 — Fees

Дата: 2026-11-21

Итог: **ВСЕ ГЕЙТЫ T3 ЗЕЛЁНЫЕ**

Гейт: 10 000 congested blocks — base fee растёт/падает по формуле на каждом шаге.

Модель `crates/aura-storage/src/fees.rs`: EIP-1559-like `base_fee` + `tip`; базовая часть **сжигается**, tip — валидатору; эластичный gas-таргет 15 000 000 (лимит 30 000 000, elasticity 2); формула `base ± max(base·|gas−target|/target/8, 1)`, неизменна при gas == target, шаг не меньше 1 базовой единицы. Сценарий `sim/t3-fees.json` (сегменты +25 %/−25 %/+50 %/−50 %/target по 250 блоков × 20), золотой дайджест пути 412f1f9e…bc124 (Rust ↔ Python ↔ пример `t3_fees_example`); итоги: сожжено 2 039 212 976 242 596 352 500 000, tips 150 000 000 000 000 000 000, финальный base 56 473. Гейт `crates/aura-storage/tests/t3_fees.rs` (9 тестов + 2 метаморфизма), оракул `tools/t3_fees_check.py`.
negative controls `tools/nc_t3.py` (`nc.log`, RESULT: 6/6 negative controls PASS).

