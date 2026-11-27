# Отчёт E7 — Lending

Дата: 2026-11-22. Фаза: **E7** (ECOSYSTEM_PLAN). Статус: **ВСЕ ГЕЙТЫ
ЗЕЛЁНЫЕ**.

## Итог

Over-collateralized lending-рынок (Aave/Compound-style): supply,
borrow под залог (LTV 75%), kinked-модель ставки от утилизации,
начисление процентов, ликвидации (health < 125%, бонус 5%, close
factor 50%). **Гейт: 30-дневная симуляция — инвариант
`collateral_value >= debt` не рвётся ни одного дня.** **NC: без
ликвидаций тот же путь рвёт инвариант на дне 18 → красный** («no
liquidation → red»), плюс 6 named-message контролей. D-E3 соблюдён.

## Модель `crates/aura-storage/src/lending.rs`

- `LendingMarket` — позиции (collateral/debt), оракульная цена, ежедневное
  начисление. `Amount`/`Price` — `u128` 18 decimals (D-T1), все
  произведения через 256-битный `mul_div` (floor), `mul_div_ceil` для
  capacity-границ.
- Параметры (документированные прецеденты): **LTV 75%** (borrow cap,
  Aave-style), **порог ликвидации health 12500 bps** (LTV > 80%; разрыв
  LTV↔порог — ценовой буфер), **бонус ликвидатора 5%** (Aave ETH),
  **close factor 50%** (Aave). Ставка — kinked-модель Compound/Aave:
  `base 2% + slope1 10% · U/kink` ниже kink (80%), `+ slope2 30%` выше;
  начисление `floor(debt·rate/(365·10000))` в день на аккаунт.
- Операции: `open_account`, `supply`, `withdraw` (capacity-проверка),
  `borrow` (`capacity = value·LTV/10000 − debt`), `repay`, `set_price`,
  `advance_day`, `liquidate` (repay ≤ 50% долга, seize =
  `floor(repaid·(1+bonus)·ONE/(price·BPS))`). Health =
  `value·10000/debt`. 12 named messages (`E7: …`).

## Гейт

`crates/aura-storage/tests/e7_lending.rs` — 14 тестов:

- `e7_gate_30day_sim_collateral_never_below_debt` — **буквальный гейт
  плана**: 2 аккаунта (1000/700 и 500/300), 5 шоков цены
  (0.85/0.72/0.61/0.52/0.44 на дни 6/12/18/24/30), 30 дней процентов,
  16 ликвидаций — **каждый день `value >= debt` на каждом аккаунте**;
  min health 12530 bps; day-30 долги `690912489586789599` /
  `4737685642880842971`; суммарные проценты `4011969083602784267`.
- `e7_gate_no_liquidation_breaks_invariant` — **функциональный NC**:
  тот же путь без ликвидаций рвёт инвариант **на дне 18** →
  «no liquidation → red».

## Метаморфизмы

- **Split supply**: `supply(40)+supply(60)` ≡ `supply(100)`.
- **Borrow→repay round-trip**: после полного погашения повторный borrow
  возвращает точное состояние.
- **Монотонность процентов**: 5 дней accrual — долг строго растёт.
- **Детерминизм**: два прогона → идентичный результат.

## Золотые значения (Python-модель ↔ Rust байт-в-байт)

| Сценарий | Значение |
|---|---|
| ставка при U=0 / 6666 / 8000 / 9000 bps | `200` / `1033` / `1200` / `2700` |
| borrow capacity (1000/700) | `50e18` (+1 → отказ) |
| max withdraw (1000/700) | `66666666666666666666` (+1 → отказ, health после = 13333) |
| ликвидаций в симуляции | `16` (первая: день 6, repay `350609153368496978254`) |
| день 30 (A, B) | `690912489586789599` / `4737685642880842971` |
| суммарные проценты / min health | `4011969083602784267` / `12530` bps |
| слом без ликвидаций | день `18` |

Rust-пример `crates/aura-storage/examples/e7_lending_example.rs` ↔
Python `tools/e7_lending_check.py` воспроизводят `sim/e7-lending.json`
(`E7-SIM-OK` / `E7-MODEL-OK`, дайджест `f8fa28de…bea7dd`).

## Negative controls

`tools/nc_e7.py` — 6 контролей (tools-only): ослабление каждого целевого
named message → красный, restore → зелёный (`RESULT: 6/6 negative
controls PASS`). **Ключевые NC: `bad-borrow-capacity` (guard LTV) и
функциональный «no liquidation → red»** (гейт-тест использует
ликвидации как несущие для инварианта: цепочки из 2–4 ликвидаций на
шок — close factor 50% восстанавливает health до ≥ 12500).

## Источники

Aave vs Compound (blofin.com): Health Factor = collateral·threshold/debt,
порог ликвидации выше LTV (буфер), close factor 50%, бонус 5% (ETH) /
15% (YFI), kinked-модель ставок (kink ~80%, «rates spike past optimal
utilisation»); academy.redduck.io «Lending and borrowing on-chain»:
kink-модель с base/slope, `U* = 80%`, ликвидации с бонусом,
«once collateral value falls below debt × bonus, paying the liquidation
bonus mathematically worsens the health factor» (объясняет цепочки
ликвидаций у границы); Medium «Comparative Analysis of DeFi Lending»
(Compound/Aave kinked utilization function); GitHub lending-protocol
(структура LendingPool/DefaultInterestRateStrategy,
`U ≤ U*: base + slope1·U/U*`). Решения D-T1..D-T4 —
`docs/economics/t8-whitepaper.md`.
