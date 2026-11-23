# E9 — Bridge + AuraLite OS (отчёт фазы)

Дата: 2026-11-22. Патч фазы: `patches/0092-E9-Bridge.patch`.
Дайджест агрегата
`4f8d9af363ab88fb7a6ce0ef6af52facb886f5e123ca0a128d1693e06d575e4f`.

## DoD и критерии (D-E4)

| Критерий | Статус |
|---|---|
| **MITM forged header → reject** | ✅ `verify_commit` проверяет КАЖДУЮ подпись (без early-stop): неизвестный подписчик (атака A, id 99) → `E9: forged header signature (99)`; бит-флип подписи валидатора (атака B) → `E9: forged header signature (1)`; полностью чужой коммит (атака C) и дубль-голос → тоже reject. 3 из 4 атак в гейте + сценарная полоса (2 попытки, `forged_rejected=2`) |
| **OS client checks finalized checkpoint** | ✅ `OsClient` (`src/bridge.rs`) — точка встраивания для AuraLite OS: `verify_checkpoint` (коммит сверяется с валидаторским набором), `accept` (только высоты ≤ finalized checkpoint → `E9: checkpoint not finalized (6)`), `check_root` |
| **emulator lane green** | ✅ `tools/e9_emulator_lane.py` → `E9-LANE-OK 4`: `aurad --version` ✓, `aurad report --fixture` ✓, запуск без `--chain-id` → exit 2 ✓, bounded-запуск ноды + SIGTERM → чистое завершение ✓ (лог `docs/evidence/e9/lane.log`); зарегистрирован в CI (`make emulator-lane`, входит в `make test`) |
| **limits / pause (D-E4)** | ✅ per-транзакционный лимит 1000 и дневной 1200 (velocity-окно, сброс по дням), guardian-pause (circuit breaker), replay-защита (id обрабатывается один раз), инвариант резерва `locked == minted` (withdraw не может превысить резерв) |
| **отдельный аудит (D-E4)** | ✅ зафиксировано: мост не включается на mainnet без H8 (отдельный аудит + bug bounty); SECURITY_PLAN: «A bridge bug is the most expensive: E9 is not enabled on mainnet without H8» |

## Модель `crates/aura-storage/src/bridge.rs`

### Light client (форма IBC/Tendermint Core Verification)

Заголовок: `height | parent | state root | time`; хэш — sha256 каноничной
формы `e9h|…`. Коммит — ed25519-подписи (RFC 8032, `aura_crypto`) хэша
заголовка. Правила принятия (LCV-FUNC-VALID.2):

1. `header.height > trusted.height` (иначе `header not newer`);
2. `header.parent == trusted.hash` (иначе `header does not chain`);
3. каждая подпись коммита — от **известного** валидатора и **верифицируется**
   (неизвестный подписчик или неверная подпись → `forged header signature`;
   дубль-голос → тоже forgery); **early-stop нет** — плохая «лишняя» подпись
   это отказ, а не шум (строже комет-ранней остановки CometBFT);
4. мощность подписей **строго больше 2/3** (`total·2/3 + 1` = 27 из 40),
   иначе `commit lacks quorum (…)`.

Каждый принятый заголовок — **finalized checkpoint** (мгновенная
финальность 2/3). Набор валидаторов фиксирован (4 × power 10);
ротация — отдельная задача поверх той же схемы (NextValidatorsHash).

### Защиты моста (уроки инцидентов)

- **Replay** (Nomad: повтор одного валидного сообщения $190M) —
  `message already processed (id)`, обработка ровно один раз.
- **Лимиты** (Nomad: $190M за 150 минут без circuit breaker) —
  per-tx 1000 + дневной velocity 1200 (обе проверки с точными границами:
  ровно лимит — ok, +1 wei — отказ).
- **Pause** (zealynx/CCIP: kill switch 1-of-N guardians) —
  `bridge is paused`, guardian-пауза/снятие.
- **Инвариант резерва** (Wormhole: mint без депозита $326M) —
  `locked == minted` после каждого шага, withdraw сверх резерва →
  `withdraw exceeds reserve`.
- **Корневая привязка** (родственник zero-root бага Nomad) — сообщение
  доказано против конкретного `state root` финализированной высоты
  (`message root mismatch`).

### AuraLite OS client

`OsClient` — self-contained (только sha256 + ed25519), минимальная
поверхность для встраивания в AuraLite OS: `verify_checkpoint` +
`accept`/`check_root` на **finalized** чекпоинте. Полная интеграция
(аппаратный keystore, TLS-канал) живёт в репозитории AuraLite-OS —
**deviation**: внешние AuraLite-OS / AuraLite-Emulator недоступны из
этого репозитория; вместо эмулятора — детерминированный in-repo лейн
`tools/e9_emulator_lane.py`, выполняющий контракт «CI run of `aurad`».

## Гейт и тесты

`crates/aura-storage/tests/e9_bridge.rs` — 8 тестов: три критерия DoD
(MITM ×4 атаки; OS-чекпоинт + forged checkpoint; полный сценарий с
построчно закреплённым шаговым логом), точные литералы 6 NC-целей,
остальные named messages, метаморфизмы (порядок подписей в коммите не
важен; границы лимитов; `locked == minted`; детерминизм двойного
прогона; кворум 27/40). Сценарий: эмулятор H1–H5 (коммиты 0,1,2) →
полоса атак (stale header, недобор кворума, 2 MITM-подделки, форк) →
переводы (500+400 депозит, 100 вывод) → replay/limit/root отказы →
pause/unpause → OS-чекпоинт (5 ok / 6 отказ) → день 2 → отказ сверх
резерва → инвариант. Шаги и дайджест
`039d498ba6fcd17e7640094492283a94be2c3447f8ebec89f9306eb438339580`
закреплены в Rust и в независимой Python-модели
`tools/e9_bridge_check.py` (ed25519 через `cryptography`, RFC 8032) —
вывод `E9-STEPS-OK 25` / `E9-SIM-OK …` байт-в-байт.

## Negative controls

`tools/nc_e9.py` — 6 мутаций named messages: `bad-forged-header`,
`bad-quorum`, `bad-chain`, **`bad-finalized` (гейтовая — «OS client
checks finalized checkpoint»)**, `bad-replay`, `bad-pause`.
**6/6 RED + GREEN.** Накопительно **608 NC / 31 фаза**.

## Источники

- **Tendermint/IBC light client**: github.com/tendermint/tendermint
  spec `verification_002_draft.md` — Core Verification: коммит содержит
  подписи >2/3 валидаторов и ни одной от посторонних; подделанный
  заголовок faulty-ноды не должен сохраниться никогда.
  chainscorelabs.com «IBC Light Client Verification» — trusted
  height/validators якорь, 2/3+ против хэша набора, misbehaviour →
  freeze. docs.tendermint.com accountability — сценарии атак (phantom
  validators, equivocation, forged header). GitHub ipsadev/ipsa-contracts
  — VerifyCommitLight: таллирование до >2/3, отказ «не новее trusted»,
  отказ коммита другой высоты.
- **Инциденты мостов**: nomoslabs.io (Nomad $190M — zero-root init,
  отсутствие rate limit/circuit breaker; Wormhole $326M — баг верификации
  подписей, инвариант minted↔deposited), dev.to/0xrivet (5 слоёв защиты;
  Bybit $1.46B без rate limit), zealynx.io checklist (pause 1-of-N,
  volume-лимиты, CCIP rate limiting), blockchaindose.com (replay nonce,
  dynamic rate limiting + auto-pause).
- **Внутренние**: `crates/aura-crypto/src/ed25519.rs` (RFC 8032,
  `sign_raw`/`verify_raw` + canonical-S guard), `crates/aura-node`
  (`aurad`: report/fixture, SIGTERM-shutdown, exit-коды 0/2/64),
  SECURITY_PLAN (D-E4, H6 bridge drill, «E9 не включается без H8»).

## Решения фазы

- **D-E9**: коммит проверяется целиком (все подписи, дубли = forgery,
  без early-stop) — строже CometBFT-ранней остановки; неизвестный
  подписчик = `forged header signature`, а не игнор.
- **D-E9b**: обе velocity-лимиты (per-tx + дневной) считаются на
  каждое сообщение включая выводы; окно сбрасывается сменой дня.
- **D-E9c**: внешние AuraLite-OS/AuraLite-Emulator заменены
  in-repo компонентами (OsClient + лейн-раннер `aurad`) — deviation
  записан; интеграция отслеживается в их репозиториях.
