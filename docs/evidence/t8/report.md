# Отчёт T8 — White paper + law

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ T8 ЗЕЛЁНЫЕ**

Гейт: чеклист против MiCA Art. 4/6/8/9, уведомление >= 20 рабочих дней,
публикация; **не юридическая консультация**.

Модель `crates/aura-storage/src/whitepaper.rs`: машина чеклиста
критпто-активного white paper по MiCA (Regulation (EU) 2023/1114).
**Art. 4** — предложение публике: offeror = юридическое лицо; white
paper составлен (**Art. 6**: секции project/technology/risks/holder
rights + заявления, трассируемые к пинам моделей T0–T7), уведомлён
(**Art. 8**: не менее 20 рабочих дней до публикации — граница: ровно 20
проходит, 19 — красный) и опубликован (**Art. 9**: до старта предложения,
версия = уведомлённой). Маркетинг (**Art. 7**) — только со ссылкой на
опубликованный white paper и после публикации. Дисклеймер «not legal
advice» обязателен (D-T4); обещания дохода запрещены (D-T2). Документы
проекта: `docs/whitepaper.md`, `docs/marketing.md`,
`docs/legal-review.md` (чеклист + юрисдикция EU/Latvia). Сценарий
`sim/t8-whitepaper.json` (8 документов, 77 шагов: 1 полностью валиден,
каждый отказник проваливает ровно один пункт), золотой дайджест
03a85943…a89396f (Rust ↔ Python ↔ пример `t8_whitepaper_example`). Гейт
`crates/aura-storage/tests/t8_whitepaper.rs` (10 тестов + 2 метаморфизма),
оракул `tools/t8_whitepaper_check.py`.
negative controls `tools/nc_t8.py` (`nc.log`, RESULT: 7/7 negative
controls PASS) — 7 контролей (стандарт 6 + literal NC 20-дневного окна).
