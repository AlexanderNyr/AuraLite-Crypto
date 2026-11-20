# Отчёт E3 — Faucet + SDK (2026-11-19)

Фаза **E3** по ECOSYSTEM_PLAN (строка «Faucet + SDK»): rate-limited
faucet (testnet only), Rust/TS/Python SDK, docs site. Гейт фазы
выполнен буквально: **1000 requests: 1 payout/address/day; SDK examples
pass in CI**.

## Гейт (детально)

- **1000 запросов → 1 выплата на адрес в день**: скрипт гейта —
  250 адресов × 3 дня (750 первых запросов) + 250 повторных запросов
  в тот же день = ровно 1000 запросов. Итог: **750 выплат, 250 отказов
  по rate limit, ни одного ключа (адрес, день) не оплачен дважды**.
  Независимая Python-модель (`tools/e3_faucet_check.py`) переигрывает
  тот же скрипт и сходится по цифрам (`tools/fixtures/e3_faucet_script.json`).
- **SDK examples pass in CI**: три примера (Rust/TS/Python) выполняются
  `tools/e3_examples_check.py` против живого reference-крана и
  печатают одинаковый `E3-EXAMPLE-OK`-саммари: выплаты 2, отказов по
  rate limit 1, отказов по сети 1. **3/3 PASS.**

## Именованные сообщения гейта

- `E3: faucet must pay 1 payout per address per day` (литерал гейта)
- `E3: faucet must refuse non-testnet requests`
- `E3: faucet must stop when out of funds`
- `E3: the daily window must roll over`
- `E3: the faucet HTTP API must apply the same policy as the library`
- `E3: the faucet must validate addresses`

## Negative controls (6/6)

| № | Внедрённый баг | Именованное сообщение |
|---|---|---|
| NC-1 | убрана проверка повтора в тот же день (литерал гейта) | `E3: faucet must pay 1 payout per address per day` |
| NC-2 | убран testnet-only guard | `E3: faucet must refuse non-testnet requests` |
| NC-3 | убрана остановка при нулевом фонде | `E3: faucet must stop when out of funds` |
| NC-4 | дневное окно не перекатывается | `E3: the daily window must roll over` |
| NC-5 | HTTP-маршрут обходит политику | `E3: the faucet HTTP API must apply the same policy as the library` |
| NC-6 | убрана валидация адресов | `E3: the faucet must validate addresses` |

## Артефакты

| Артефакт | Путь |
|---|---|
| Faucet (детерминированная политика + stdlib HTTP) | `crates/aura-storage/src/faucet.rs` |
| Rust SDK | `crates/aura-storage/src/sdk.rs` |
| TypeScript SDK | `sdk/ts/aura_lite.ts` (+ `sdk/ts/example.ts`) |
| Python SDK | `sdk/python/aura_lite.py` (+ `sdk/python/example.py`) |
| Гейт-тест | `crates/aura-storage/tests/e3_faucet.rs` (`make test-e3`) |
| Оракул политики | `tools/e3_faucet_check.py` + `tools/fixtures/e3_faucet_script.json` |
| Примеры SDK в CI | `tools/e3_examples_check.py` (`make examples-e3`) |
| Docs site | `docs/sdk/index.md` |
| Эвиденс | `docs/evidence/e3/` (gates.txt, nc.log, campaign/result.json → `TOTAL: 512`) |

## Отклонения от плана (зафиксированы правкой план-дока)

1. Выплата = запись в леджер крана со списанием фонда; ончейн-перевод
   не выполняется, пока W1-FORMAT-v1 testnet freeze в силе.
2. Демо-протокол HTTP несёт явный `now` (детерминизм CI).
3. TS-пример выполняется в CI детерминированным type-strip + `node`
   (TypeScript-тулчейн не пинен в CI).
4. Docs site — статические страницы `docs/sdk/` без пайплайна публикации.

Итоговый счётчик: **512 negative controls cumulative** (506 + 6).
