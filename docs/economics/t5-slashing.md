# T5 — Slashing and insurance (ECONOMICS_PLAN)

**Статус:** ✅ закрыто (2026-11-21). **Гейт:** 7 scenarios: penalty ≥ damage,
fund never negative — выполнен буквально.

## Решения

- **Параметры штрафа.** Инцидент (double-sign, простой и т.п.) причиняет
  измеримый `damage`. Штраф обидчика = `damage × T5_PENALTY_BPS / T5_BPS`
  = **150 %** — `penalty ≥ damage` выполняется по построению и проверяется
  на каждом из 48 инцидентов пиненого прогона (включая floor-края
  `damage = 10^18 ± 1`).
- **Страховой фонд.** Пострадавшие покрываются из `InsuranceFund`:
  `covered = min(damage, balance)`; непокрытый остаток — `shortfall`
  (фиксируется, не теряется). Пополнение: **5000 bps** применённого штрафа
  (после mass-slash cap) идёт в фонд. Инвариант: **баланс фонда никогда
  не уходит в минус** (подписанная `i128`, проверка на каждом шаге).
- **Mass-slash protection.** Ни одно окно не может уничтожить больше
  **500 bps** зарегистрированного bonded (параметр `T5_REGISTERED_BONDED`
  = 1 000 000 `ALCRY` → cap = **50 000 `ALCRY`/окно**). Избыток штрафа
  **откладывается** в очередь (`pending`) — не прощается — и собирается
  следующими окнами (`advance`), пока очередь не станет пустой
  (`total_applied == total_penalty`, `pending == 0` в конце прогона).

## Сценарий `sim/t5-slashing.json` (7 сценариев, 87 шагов, 48 инцидентов)

1. единичный double-sign (100 `ALCRY` ущерба);
2. повторный нарушитель (10 × 50);
3. коррелированный mass-slash (20 × 5 000) с дренажом по 3 окнам;
4. давление истощения фонда (3 × 60 000) с дренажом по 6 окнам;
5. floor-края штрафа (10^18+1 и 10^18−1);
6. mass-slash cap поверх нескольких окон (8 × 20 000);
7. цепочка истощения и пополнения (500 000 + 250 000, дренаж, повторное
   покрытие 1 000/2 000, финальный дренаж).

Золотой дайджест
`4cc9ea0319374c3c2c74bc88db6d7747b23f0e21ea637ae2270bc4c5707c9cef`
закреплён в `crates/aura-storage/tests/t5_slashing.rs` и пересчитан
независимым Python-оракулом `tools/t5_slashing_check.py`, который также
запускает пример `t5_slashing_example`. Финал: фонд **536 750 `ALCRY`**,
очередь пуста, `total_applied == total_penalty == 1 790 402 999 999 999
999 999 999`, покрыто **558 451 499 999 999 999 999 998**, непокрыто
**635 150 500 000 000 000 000 002**; фонд опускался ровно до 0
(шаг 60) и восстанавливался — инвариант «never negative» удержится в
каждой точке.

## Negative controls (`tools/nc_t5.py`, 7/7)

| # | Мутация | Named message |
|---|---|---|
| 1 | `T5_PENALTY_BPS` 15 000 → 9 999 | `T5: the penalty must cover the damage` |
| 2 | clamp покрытия убран (`covered = damage`) | `T5: the insurance fund must never go negative` |
| 3 | `T5_MASS_SLASH_CAP_BPS` 500 → 5 000 | `T5: the mass-slash cap must hold in every window` |
| 4 | отложенная часть прощается (`pending = 0`) | `T5: deferred penalties must not be dropped` |
| 5 | доход фонда +1 | `T5: the insurance income must be exactly the fund share of the applied penalty` |
| 6 | формула штрафа /10 001 | `T5: the penalty must be exactly damage times the pinned parameter` |
| 7 | `T5_FUND_SHARE_BPS` 5 000 → 5 001 | `T5: the reference simulation must agree with the pinned digest` |

Эвиденс: `docs/evidence/t5/` (контрольные мутации кумулятивно: **550** за
22 фазы). Патч фазы: `patches/0083-T5-slashing.patch`.
