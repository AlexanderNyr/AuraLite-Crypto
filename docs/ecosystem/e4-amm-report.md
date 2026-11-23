# Отчёт E4 — AMM DEX

Дата: 2026-11-22. Фаза: **E4** (ECOSYSTEM_PLAN). Статус: **ВСЕ ГЕЙТЫ
ЗЕЛЁНЫЕ**.

## Итог

Constant-product AMM (x·y = k) с LP-токенами, свопами, slippage guard и
мультимаршрутной торговлей. **Гейт: property-инвариант `k` никогда не
убывает** (включая маршруты). **NC: отключение slippage guard → красный.**
Порядок D-E3 соблюдён: DeFi-фазы идут после оракула (E5) и аудита.

## Модель `crates/aura-storage/src/amm.rs`

- `Pool` — неупорядоченная пара токенов (`Token = [u8; 16]`), резервы и
  LP-supply в `Amount = u128` (18 decimals, решение D-T1).
- Кривая: `in_fee = floor(amount_in · (10000 − fee_bps) / 10000)`,
  `out = floor(in_fee · reserve_out / (reserve_in + in_fee))` — fee-on-input
  30 bps (0.3%, модель Uniswap V2), округление floor на каждом делении.
- 256-битная арифметика: `wide_mul` (schoolbook 2×2 limbs с carry) и
  `mul_div` (деление столбиком по 256 битам, `None` при выходе за `u128`).
  Инвариант `k = reserve_a · reserve_b` считается в 256 битах (для
  обычных резервов `k` не влезает в `u128`).
- LP-токены: первый депозит минтит `amount_a + amount_b` share
  (объяснённое отклонение от `sqrt` Uniswap V2), далее — строго
  пропорциональный депозит `min(a·S/ra, b·S/rb)`; `remove_liquidity` —
  pro-rata burn с точным round-trip на золотых значениях.
- `swap_exact_in(token, amount_in, min_out)` — slippage guard: `out <
  min_out` → именованная ошибка `E4: slippage guard: output …`.
- `swap_route(pools, path, amount_in, min_out)` — мультимаршрут A→B→C,
  проверка соответствия токенов хопов пулам, `min_out` на последнем хопе.
- 9 named messages (`E4: …`) — вход в невалидные состояния немедленно
  красный с точной строкой.

## Гейт

`crates/aura-storage/tests/e4_amm.rs` — 21 тест, включая
`e4_gate_k_never_decreases`: детерминированная последовательность свопов
в обе стороны с дикими размерами + маршруты — на каждом пуле
`wide_ge(k_after, k_before)` (широкое сравнение 256-бит). Плюс
`e4_zero_fee_keeps_k` (нулевая комиссия → `k` неизменно точно).

## Метаморфизмы

- **Split не бьёт single** (конкавность x·y=k): `swap(2A) ≥ swap(1A) +
  swap(1A)` на одном и том же пуле — золотые `3910033923564131223 ≥
  3909976989011504086`.
- **Route = произведение хопов**: `swap_route([A,B,C]) == ручной
  двухшаговый своп** на нетронутых пулах.
- **Burn round-trip**: mint 50/100 → burn share → ровно (50, 100).
- **Детерминизм**: два одинаковых прогона → идентичный результат.

## Золотые значения (fee 30bps, floor)

| Сценарий | Значение |
|---|---|
| 1 A→B из (100/200) | `1974316068794122597` |
| 1 A→B→C через (100/200)+(100/300) | `5791186053878244695` |
| single 2 A | `3910033923564131223` |
| split 1 A + 1 A | `3909976989011504086` |
| initial LP / proportional LP | `300e18` / `150e18` |
| burn round-trip | (50e18, 100e18) точно |

Rust-пример `crates/aura-storage/examples/e4_amm_example.rs` ↔ Python
модель `tools/e4_amm_check.py` воспроизводят сценарий `sim/e4-amm.json`
байт-в-байт (агрегат `E4-SIM-OK`).

## Negative controls

`tools/nc_e4.py` — 6 контролей (tools-only, вне исходников): ослабление
каждого named message → красный (`RESULT: 6/6 negative controls PASS`).
**Ключевой NC: `bad-slippage` (ослабление slippage guard) → красный.**

## Источники

Математика AMM: Uniswap V2 x·y=k (документация whitepaper; формула
`dy = y·dx/(x+dx)`, комиссия 0.3% → 997/1000, MINIMUM_LIQUIDITY,
LP mint/burn), статьи dev.to «AMM Mathematics: Constant Product and
Beyond», autheo.com «Liquidity Pool Mechanics», zealynx.io «How to Build
Uniswap V2 From Scratch». Решения D-T1..D-T4 (u128/18dec, без обещаний
дохода, симуляция при смене параметров, не юр. консультация) —
`docs/economics/t8-whitepaper.md`.
