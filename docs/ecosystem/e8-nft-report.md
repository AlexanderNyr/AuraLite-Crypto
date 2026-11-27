# E8 — NFT + names + DAO (отчёт фазы)

Дата: 2026-11-22. Патч фазы: `patches/0091-E8-NFT-DAO.patch`
(E7 → `patches/0090-E7-Lending.patch`). Дайджест агрегата
`39674d6a0813cb9beae5837025e1a8b6c00fb8ebe1bc620a06e844a5829fb313`.

## DoD и что сделано

| Требование | Статус |
|---|---|
| ARC-721/1155 | ✅ модель `crates/aura-storage/src/nft.rs` (mint/transfer/approve/transfer_from/burn, per-id supply, operators, batch, URI) |
| `.aura` name service | ✅ register/renew/transfer/set_target/resolve/set_primary/reverse, срок 365 дней + grace 90 |
| on-chain identity | ✅ `set_identity`/`clear_identity`/`identity` — мета-дайджест привязан к живому имени |
| governance UI | ✅ `crates/aura-storage/src/dao_ui.rs`: propose/vote/finalize/execute/veto через контроллер, журнал receipt'ов, самодостаточная HTML-страница |
| **50 standard vectors** | ✅ `tests/fixtures/e8-standards.txt` (генератор `tools/gen_e8_vectors.py`), все 50 зелёные, дайджест `8cb9da94…62bd3` |
| **first DAO vote through the UI** | ✅ `run_e8_scenario`: голос №1 (voter 1, yes, 150000 ALCRY) проходит через UI, receipt записан, tally `250000/75000/25000` ALCRY, proposal → executed; HTML-страница идентична в Rust и Python (sha `bf1486e2…1167f`) |

## Модель

### ARC-721 (по `standards/arc721`, X5)

Токен существует ровно тогда, когда присутствует слот владельца —
сожжённый id можно минтить заново. **Каждый перевод (владельцем или
апрувленным спендером) удаляет слот апрува** — классический баг
stale-approval; вектор `v013` его закрепляет (после перевода старый
апрув мёртв → `E8: not approved (dave/2)`). Минт/URI — только владелец
коллекции (`root`).

### ARC-1155

Один контракт — много id; у каждого id своя эмиссия (supply 1 = NFT-
семантика, > 1 — взаимозаменяемые). Держатель выдаёт оператору
(`set_operator`) право двигать любые свои id; batch-переводы
all-or-nothing: каждая «нога» проверяется до применения любой.

### `.aura` имена (форма ENS)

Реестр: владелец, цель резолва (target), срок. Регистрация — на
[`TERM_DAYS=365`] дней; после истечения имя резолвится ещё
[`GRACE_DAYS=90`] (продлить может кто угодно — как в ENSv2), затем
`E8: name expired` и повторная регистрация разрешена. Перевод имени
чистит primary-имя старого владельца. Identity-запись (мета-дайджест)
ставит только владелец живого имени.

### Governance UI

`DaoUi` оборачивает T6 `Governance` (порог, кворум 20%, снапшот
голосующей силы, voting period, 48-часовой timelock, veto с sunset).
**Путь только через UI**: каждый принятый голос даёт [`UiReceipt`] в
журнале действий; `render_html()` рендерит страницу DAO (карточки
предложений, tally, таблица голосов) — байт-в-байт воспроизводится
независимой Python-моделью.

## Гейт

1. **50 standard vectors** — фикстура блок-формата (`vector=vNNN`,
   `status=ok|err` с точным литералом `E8: …`, `read.<key>=<value>`).
   Rust-тест и Python-модель исполняют её независимо и оба дают
   дайджест `8cb9da945b0eed8f4432d31b71d20fffb72a6317fc90204d1890bd480c062bd3`.
   Векторы покрывают: минт/дубль-id/несуществующий id/перевод не
   владельцем/zero-address, approve→transfer_from, **stale-approval**,
   burn→re-mint, 1155-балансы/операторы/revocation/batch/length-
   mismatch/zero-amount, регистрацию/занятое имя/невалидное имя/
   резолв/target/перевод/primary/reverse, renew на чужом праве,
   границы expiry+grace (день 822 жив / день 823 мёртв / повторная
   регистрация), identity set/update/clear/чтение.
2. **First DAO vote through the UI** — сценарий: propose(0) → **голос
   №1 через UI** (1/1/yes/150000·ALCRY) → ещё три голоса → повторный
   голос отклонён (`E8: vote already cast (1/1)`) → finalize (passed:
   quorum 120000·ALCRY, yes 250000 > no 75000) → execute после
   timelock → `executed`. Шаги и дайджест
   `17fd44850375c54c9cf9826b8ed55da0faf1855da72514184c83501f07409d71`
   закреплены в Rust-тесте и Python-модели; HTML-страница
   (`docs/evidence/e8/governance-ui.html`) идентична с обеих сторон.

## Тесты и метаморфизмы

`crates/aura-storage/tests/e8_nft.rs` — 15 тестов: все 50 векторов с
точными литералами, дайджесты, гейт-сценарий, HTML-страница, точные
сообщения всех DAO-ошибок (below threshold, voting closed/still open,
already finalized, timelock, already executed, not passed, no voting
power, proposal not found, veto sunset), метаморфизмы (batch ≡
последовательные переводы; независимость от порядка минта; независимость
от порядка голосов; детерминизм двойного прогона), правила имён.

## Negative controls

`tools/nc_e8.py` — 6 мутаций named messages (каждая ослабляет ровно
один литерал, якорь count==1, после прогона — restore + зелёный):
`bad-mint-exists`, `bad-transfer-owner`, `bad-stale-approval`
(сообщение о не-апруве — страж stale-пути), `bad-name-taken`,
`bad-name-owner`, `bad-ui-vote` (гейтовая). **6/6 RED + GREEN.**
Накопительно **602 NC / 30 фаз**.

## Источники

- **ERC-1155**: rareskills.io «ERC-1155 Explained» — один id с supply 1
  = NFT, mint вне спецификации; eco.com «When to Use It» —
  `safeBatchTransferFrom`, `setApprovalForAll`, паттерны id; webopedia
  «ERC Token Standards» и metana «Guide» — сравнение 20/721/1155,
  batch-газ.
- **ENS / name service**: gate.com «ENS Comprehensive Analysis» —
  registry/registrar/resolver, reverse resolution, годовое продление,
  grace period; chainscorelabs.com — registry хранит owner/resolver/TTL,
  имена — NFT (ERC-721); cryptoslate.com — 90-дневный grace и
  последующий аукцион; docs.ens.domains (ETH Registrar v2) —
  продление от любого аккаунта, base-rate без премии.
- **Внутренние**: `standards/arc721/src/lib.rs` (X5: таблица методов,
  «exists = owner slot», переводы чистят апрув), T6
  `crates/aura-storage/src/governance.rs` (жизненный цикл предложения).

## Решения фазы

- **D-E6**: 50 стандартных векторов — фикстура блок-формата в
  `tests/fixtures/e8-standards.txt` (считается `tools/count_vectors.py`);
  генератор `tools/gen_e8_vectors.py` пишет литеральные ожидания,
  Rust и Python исполняют независимо.
- **D-E7**: `.aura` — ENS-форма: срок 365 дней, grace 90 дней (resolve
  жив; продлить может кто угодно), повторная регистрация после
  expiry+grace; перевод имени чистит primary старого владельца.
- **D-E8**: governance UI — единственный путь голосования (DaoUi
  оборачивает T6 DAO и журналит receipt'ы); HTML-страница DAO
  байт-идентична в Rust и Python (кросс-проверка sha).
