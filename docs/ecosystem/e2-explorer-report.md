# Отчёт E2 — обозреватель цепи (Explorer)

Дата: 2026-11-19. Фаза **E2** по ECOSYSTEM_PLAN (строка «Explorer»):
Rust backend + web UI: blocks/tx/accounts/validators/contracts, verified
source. Гейт фазы: **10 user scenarios green; reproducible contract build =
same code_hash**.

## Что пользователь должен уметь делать (10 сценариев)

<!-- E2: scenarios -->

| ID | Сценарий | Что пользователь должен уметь делать | Гейт |
|---|---|---|---|
| E2S-01 | Открыть главную страницу | видеть все разделы: блоки, транзакции, счета, валидаторы, контракты, поиск | `E2: home page must show blocks, tx, accounts, validators, contracts and search` |
| E2S-02 | Смотреть последние блоки | видеть последние канонические блоки ровно как в индексере цепи | `E2: explorer must read blocks from the indexer` |
| E2S-03 | Открыть блок по высоте или хэшу | попадать на одну и ту же страницу блока по высоте и по хэшу | `E2: block lookup by height and hash must agree` |
| E2S-04 | Найти транзакцию и её статус | видеть confirmed/finalized/orphaned/unknown по канонической цепи | `E2: tx status must follow the canonical chain` |
| E2S-05 | Баланс и история счёта | смотреть баланс и прошлые транзакции без посредников | `E2: account history must match the chain` |
| E2S-06 | Довести транзакцию до финальности | видеть одно однозначное слово статуса | `E2: transaction finality must be unambiguous` |
| E2S-07 | Валидаторы и здоровье консенсуса | видеть набор валидаторов, участие и здоровье сети | `E2: validator view must show consensus health` |
| E2S-08 | Поиск по всему | вводить высоту, хэш, адрес или адрес контракта и попадать на объект | `E2: search must find blocks, txs, accounts and contracts` |
| E2S-09 | Проверенный исходник контракта | читать verified source, совпадающий с задеплоенным code_hash | `E2: contract page must show the verified source` |
| E2S-10 | Страница = API | видеть в странице те же данные, что возвращает JSON API | `E2: the web UI must render the same data as the API` |

**Readiness: 10/10 ready.**

Трассировка: E2S-05 = US-03, E2S-06 = US-04, E2S-07 = US-05, E2S-09 = шаг
verified source из US-07 (реестр `tools/fixtures/e0_user_scenarios.json`,
отчёт E0). Сценарии обслуживаются обозревателем, который читает блоки
**только** из E1-индексера (решение D-E1).

## Reproducible contract build

Сборка контрактов детерминирована (формат ALVM-1, сборщик в
`crates/aura-storage/src/explorer.rs`): каноникализация источника
(пробелы/регистр/комментарии) → одинаковые байты кода → одинаковый
`code_hash`. Независимый эталон (Python-сборщик в
`tools/e2_explorer_check.py`) пересчитывает `tools/fixtures/e2_contracts.json`:
`adder` и его псевдоним `  PUSH   003  …` дают один хэш
`64e3e32d…d0b0d`, `counter` — `b17231dd…d412a`. Свойство в тесте:
`E2: contract build must be reproducible (same code_hash)`; проверка
verified source: `E2: verified source must match the deployed code_hash`.

## Как это устроено

- Backend: `crates/aura-storage/src/explorer.rs` — read-only представления
  (блоки, транзакции, счета, валидаторы, контракты) поверх
  `ChainIndexer`; статусы транзакций выводятся из каноничности блоков
  (finalized при ≥8 подтверждениях).
- Web UI: одна страница `render_page()` + JSON-маршруты
  (`/blocks`, `/block/<h>`, `/tx/<hash>`, `/account/<addr>`,
  `/validators`, `/contracts`, `/contract/<addr>`, `/search?q=`,
  `/health`), отдаются stdlib-HTTP-сервером `explorer::serve`
  (новых зависимостей нет — Cargo.lock запинен H1).
- Гейт: `crates/aura-storage/tests/e2_explorer.rs` — 10 сценариев через
  реальные HTTP-запросы + 2 свойства; мутационные контроли
  `tools/nc_e2.py` (6/6).
