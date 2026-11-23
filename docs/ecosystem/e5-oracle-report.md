# E5 — Oracle: децентрализованная сеть ценовых фидов

Дата: 2026-11-22. Фаза **E5** по `docs/plans/ECOSYSTEM_PLAN.md`
(решение **D-E3**: порядок DeFi — после оракула и аудита; аудит H8 закрыт,
поэтому следующая за E3 децентрализованная фаза — E5, затем E4 AMM).

## DoD фазы

- ✅ Feed network: зарегистрированные провайдеры с ed25519-ключами;
- ✅ Каждый отчёт подписан провайдером (canon: `feed16 || price16BE || ts8BE`);
- ✅ Медиана admitted-цен (lower median — детерминирована для чётных длин);
- ✅ Кворум: N/2+1 (минимум 3); один провайдер → красный (named message);
- ✅ Staleness: отчёт строго в окне `(now − 3600s, now]`; будущие — отбой;
- ✅ Outlier band: ±500 bps от предварительной медианы → финальная медиана.

## Модель `crates/aura-storage/src/oracle.rs`

- `Price = u128` (18 decimals, `E5_ONE = 10^18` — решение D-T1);
- `PriceReport::sign/verify` — ed25519 (`aura_crypto::sign_raw/verify_raw`);
- `OracleNetwork::submit(now, reports)`:
  1. **strict admission**: неизвестный провайдер, дубль провайдера в раунде,
     чужой фид, неверная подпись, `ts <= now − 3600` или `ts > now` → `Err`
     с named message (7 штук — см. ниже);
  2. **quorum**: `admitted.len() >= quorum` (quorum = N/2+1, минимум 3);
  3. **aggregation**: `provisional = lower_median(admitted.prices)`,
     выжившие = `|p − provisional| * 10000 <= 500 * provisional`,
     `price = lower_median(survivors.prices)`,
     `timestamp = lower_median(survivors.timestamps)`.
- Константы: `E5_MAX_STALENESS_SECS = 3600` (heartbeat Chainlink),
  `E5_MIN_QUORUM = 3`, `E5_DEVIATION_BPS = 500` (0.5% — отклонение
  перекрытия Chainlink), `E5_ONE = 10^18`.

### Named messages (для `tools/nc_e5.py`)

1. `E5: round requires at least one report`;
2. `E5: report from unregistered provider {provider}`;
3. `E5: one report per provider per round (provider {provider})`;
4. `E5: all reports must target this feed (provider {provider})`;
5. `E5: provider report signature must verify (provider {provider})`;
6. `E5: provider report must be fresh (provider {provider}, ts {timestamp}, now {now})`;
7. `E5: oracle requires a quorum of provider reports ({valid}/{required})`.

## Гейт и метаморфизмы

Гейт `crates/aura-storage/tests/e5_oracle.rs` (14 тестов):

- **literal gate**: 5 провайдеров, 2 вредоносных (памп x100 и дамп
  0.0001) → цена **100.0** (нижняя медиана честных 99.9/100.0/100.1)
  не двигается; оба выброса отброшены;
- **NC**: один провайдер → `E5: oracle requires a quorum of provider
  reports (1/3)`;
- отказы: плохая подпись, stale (ts = now−3600−1), future (ts = now+1),
  дубль провайдера, чужой фид, неизвестный провайдер, пустой раунд;
- outlier: 5 отчётов с одним x100-выбросом → rejected 1, цена 101.0;
- **метаморфизмы**: перестановка отчётов не меняет агрегат; масштабирование
  всех цен ×2 → медиана ×2; повтор того же раунда → тот же агрегат.

Симуляция `sim/e5-oracle.json` + `crates/aura-storage/examples/e5_oracle_example.rs`
(→ `make examples-e5`): `E5-SIM-OK {"providers": 5, "admitted": 5,
"rejected_outliers": 2, "price": "100000000000000000000", "timestamp":
999989, "quorum": 3, "staleness_secs": 3600}`. Независимая Python-модель
`tools/e5_oracle_check.py` (→ `make check-e5`) подтверждает те же числа
(`E5-MODEL-OK`). Агрегатный дайджест:
`aee332b61c43f19d234ec79044208daa4bb1671ec1e8126b5d7ff75a4bdead24`
(канон `E5|feed=ALCRY/USD.......|price=100000000000000000000|admitted=5|
rejected=2|quorum=3|staleness=3600|deviation=500`).

## Negative controls (`tools/nc_e5.py`, 6/6)

Каждая мутация ослабляет ровно один named message (счётчик анкоров == 1),
гоняет `cargo test -p aura-storage --test e5_oracle` и требует красный;
`finally` восстанавливает оригинальные байты → зелёный:

| # | Мутация | Ожидание |
|---|---------|----------|
| NC-1 | quorum message без чисел | красный |
| NC-2 | duplicate-provider message без id | красный |
| NC-3 | unknown-provider message без «unregistered» | красный |
| NC-4 | feed-mismatch message упрощён | красный |
| NC-5 | staleness message без ts/now | красный |
| NC-6 | empty-round message упрощён | красный |

Итог: `NC-GREEN restored: 6/6 mutations turned the gate red`.

## Решения фазы

- **D-E3** (из ECOSYSTEM_PLAN): порядок DeFi — оракул → аудит → AMM/CDP/
  lending; аудит H8 закрыт, поэтому E5 идёт перед E4. Порядок неизменен.
- Медиана (не среднее): устойчива к выбросам; lower median — детерминизм.
- Кворум N/2+1: византийский кворум (2 вредоносных из 5 не двигают медиану
  и не блокируют финализацию — достаточно 3 честных).
- Staleness 3600s + deviation 500 bps: параметры heartbeat/перекрытия
  ценовых фидов Chainlink.

## Источники (web, 2026-11-22)

- Chainlink DON: N = 11–31 операторов, офчейн-агрегация (OCR 2.0),
  quorum N/2+1 подписей, heartbeat 3600s и deviation 0.5% (min responses),
  удаление выбросов/trimmed mean; threshold signatures t-of-n (f < n/3).
  Источники: bex.co/course/defi/6.1; everstake.com/resources/blog/
  chainlink-vs-pyth-oracle-comparison; everstake.com/resources/blog/
  what-is-blockchain-oracle; defi-explained.dev/oracles/;
  bumba.global/assets/whitepaper/Chainlink.pdf.
- Pyth: first-party publishers, pull-модель (~400ms) — для сравнения архитектур.
- UMA: optimistic oracle (оспоримые ответы) — для сравнения архитектур.

## Доказательства

Тиер `docs/evidence/e5/` (baseline.log, e5-oracle.log, campaign/,
gates.txt, inventory.txt, nc.log, nc-suite.log, report.md, SHA256SUMS) —
см. `tools/write_e5_evidence.py`.
