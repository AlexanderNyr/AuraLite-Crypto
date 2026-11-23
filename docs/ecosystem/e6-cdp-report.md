# Отчёт E6 — Stablecoin CDP

Дата: 2026-11-22. Фаза: **E6** (ECOSYSTEM_PLAN). Статус: **ВСЕ ГЕЙТЫ
ЗЕЛЁНЫЕ**.

## Итог

Over-collateralized CDP (MakerDAO-style vaults): депозит залога, минт
USD-стейблкоина, погашение, вывод, ликвидации и peg stress. **Гейт:
200% collateral при −40% цены → ликвидация без bad debt.** Property:
peg-инвариант (каждая единица стейблкоина обеспечена ≥ $1 залога,
`bad_debt == 0`) не рвётся ни на одном шаге стресс-последовательности.
**NC: ослабление guard-сообщения mint ratio (и ещё 5 named messages) →
красный.** D-E3 соблюдён: CDP идёт после оракула (E5), аудита H8 и AMM
(E4).

## Модель `crates/aura-storage/src/cdp.rs`

- `CdpEngine` — реестр vault'ов, оракульная цена (USD за 1 единицу
  залога, 18 decimals), учёт стейблкоина (`stable_supply`) и
  `bad_debt`. `Amount`/`Price` — `u128` 18 decimals (D-T1).
- Риск-параметры: **min ratio 150%** (`E6_MIN_RATIO_BPS = 15000`,
  ликвидационный ratio MakerDAO ETH-A) и **штраф 5%**
  (`E6_LIQ_PENALTY_BPS = 500`, прецедент USDC-A/Raft). Все произведения
  через 256-битный `mul_div` из `amm.rs` (floor на каждом делении).
- Операции: `open_vault`, `deposit`, `mint` (ratio после ≥ 150%),
  `repay`, `withdraw` (ratio после ≥ 150%), `set_price` (peg stress),
  `liquidate`. Отношение `ratio_bps = value·10000/debt`,
  `value = collateral·price/1e18`.
- Ликвидация: изымается залог на сумму `debt·(1+penalty)` по текущей
  цене (`seized = floor(penalty_debt·1e18/price)`, cap по залогу),
  `repaid = min(debt, seized·price/1e18)` сжигается, остаток залога —
  владельцу vault'а; недобор (теоретический) пишется в `bad_debt`.
  Здоровый vault не ликвидируется.
- Peg-инвариант `peg_holds()`: `bad_debt == 0 && backed_value() >=
  stable_supply`. 12 named messages (`E6: …`).

## Гейт

`crates/aura-storage/tests/e6_cdp.rs` — 14 тестов:

- `e6_gate_200pct_minus40_liquidates_no_bad_debt` — **буквальный гейт
  плана**: vault 200/100 (200%), цена 0.6 (−40%) → ratio 12000 bps →
  ликвидация: seized `175e18`, repaid `100e18`, shortfall `0`,
  returned `25e18`; `bad_debt == 0`, `peg_holds`.
- `e6_gate_peg_backing_never_breaks` — property: три vault'а
  (200/220/300 против долга 100), удары 0.9/0.6/0.45, три ликвидации —
  на каждом шаге `bad_debt == 0` и `backed_value ≥ stable_supply`.

## Метаморфизмы

- **Split депозита**: `deposit(40)+deposit(60)` ≡ `deposit(100)` (то
  же состояние и ratio).
- **Mint→repay round-trip** — точный возврат к исходному состоянию и
  supply.
- **Порядок ликвидаций**: ликвидация двух vault'ов в любом порядке →
  одинаковые системные итоги.
- **Детерминизм**: два прогона → идентичный результат.

## Золотые значения (min 15000 bps, penalty 500 bps, floor)

| Сценарий | Значение |
|---|---|
| ratio после −40% | `12000` bps |
| seized / repaid / shortfall / returned (гейт) | `175e18` / `100e18` / `0` / `25e18` |
| seized при цене 0.45 (300/100) | `233333333333333333333` |
| returned при цене 0.45 | `66666666666666666667` |
| max mint под 200e18 (граница 15000 bps) | `133333333333333333333` (+1 → отказ `14999/15000`) |
| max withdraw от 200/100 | `50e18` (+1 → отказ `14999/15000`) |

Rust-пример `crates/aura-storage/examples/e6_cdp_example.rs` ↔ Python
модель `tools/e6_cdp_check.py` воспроизводят `sim/e6-cdp.json`
байт-в-байт (`E6-SIM-OK` / `E6-MODEL-OK`).

## Negative controls

`tools/nc_e6.py` — 6 контролей (tools-only): ослабление каждого целевого
named message → красный, restore → зелёный (`RESULT: 6/6 negative
controls PASS`). **Ключевой NC: `bad-mint-ratio`** (guard минта).

## Источники

MakerDAO: vault/CDP, liquidation ratio 150% (ETH-A), liquidation penalty
13% (ETH-A) / 5% (USDC-A), stability fee как инструмент peg; изъятие
покрывает debt + penalty, остаток — владельцу; «this is a critical
safety mechanism that protects the DAI stablecoin's peg by ensuring all
minted DAI remains fully backed» (community-development.makerdao.com).
Параметры риска и дизайн ликвидационного движка — chainscorelabs.com
«How to Architect a Collateralized Debt Position Engine» (MakerDAO vs
Liquity vs Raft: min ratio 170/110/120%, penalty 13/0/5%); Raft 120%/5%
как прецедент штрафа 5%. Решения D-T1..D-T4 — `docs/economics/t8-whitepaper.md`.
