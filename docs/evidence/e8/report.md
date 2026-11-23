# Отчёт E8 — NFT + names + DAO

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E8 ЗЕЛЁНЫЕ**

Гейт 1: **50 standard vectors** (ARC-721/1155 + `.aura` имена +
identity) — фикстура `tests/fixtures/e8-standards.txt` с точными
литералами ошибок `E8: …`; Rust-тест и независимая Python-модель дают
одинаковый дайджест `8cb9da94…62bd3`. Гейт 2: **первый DAO-голос через
UI** — receipt записан, tally `250000/75000/25000` ALCRY доведён до
`executed`; HTML-страница governance UI байт-идентична в Rust и Python
(sha `bf1486e2…1167f`). 6 named-message мутационных контролей.

Модель `crates/aura-storage/src/nft.rs`: ARC-721 (per-slot ownership,
переводы чистят апрув — stale-approval вектор v013), ARC-1155 (per-id
supply, операторы, all-or-nothing batch), `.aura` name service
(365 дней + grace 90, продление от кого угодно, повторная регистрация
после expiry+grace, primary/reverse), on-chain identity (мета-дайджест
привязан к живому имени). Governance UI `src/dao_ui.rs` оборачивает T6
DAO: propose/vote/finalize/execute/veto, журнал receipt'ов, детермини-
рованная HTML-страница. 25 named messages. Решения D-E6/D-E7/D-E8.

Симуляция `sim/e8-dao.json`, агрегат `39674d6a…fb313`
(`E8-VECTORS-OK 50` / `E8-UI-OK` / `E8-SIM-OK` байт-в-байт между
`examples/e8_nft_example.rs` и `tools/e8_nft_check.py`). Гейт-тесты
`tests/e8_nft.rs` (15 тестов: векторы, дайджесты, сценарий, HTML, точные
DAO-ошибки, метаморфизмы batch≡sequential / порядок минта / порядок
голосов / детерминизм). Negative controls `tools/nc_e8.py` — 6
контролей (`nc.log`, RESULT: 6/6 negative controls PASS).
Источники: rareskills.io ERC-1155, eco.com, webopedia, metana;
gate.com ENS, chainscorelabs.com, cryptoslate.com, docs.ens.domains —
см. `docs/ecosystem/e8-nft-report.md`.
