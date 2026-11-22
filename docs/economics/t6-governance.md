# T6 — Governance (ECONOMICS_PLAN)

**Статус:** ✅ закрыто (2026-11-21). **Гейт:** 20 proposals: valid ones pass,
flash-stake attack rejected — выполнен буквально.

## Решения

- **Порог предложения.** Предложение может создать держатель стейка ≥
  `T6_PROPOSAL_THRESHOLD` = **10 000 `ALCRY`** (граница: ровно 10 000 —
  можно; меньше — `BelowThreshold`, красный).
- **Кворум.** Проголосовавшая мощность ≥ **2000 bps** (20 %) от суммарного
  снапшота предложения, иначе оно отклоняется. Граница: голоса == кворуму
  точно → проходит (≥). Ничья `yes == no` — отклонение (нужно строгое
  большинство `yes > no`); abstain считается в кворум, но не в большинство.
- **Период голосования.** 120 960 блоков (7 дней по 5 с). Голос принимается
  в полуинтервале `[created, created + period)` — на блоке закрытия красный
  (`VotingClosed`), на последнем блоке окна — зелёный.
- **48-часовой timelock.** Исполнение возможно не раньше
  `created + period + 34 560` (48 ч по 5 с). На блок раньше — красный
  (`Timelock`), на границе — зелёный.
- **Veto/emergency с sunset.** Экстренное veto переводит Passed в Vetoed;
  право **закатывается** на блоке `T6_VETO_SUNSET_BLOCK = 1 521 000`: за блок
  до — работает, на закате и после — красный (`VetoSunset`).
- **Flash-stake атака отвергается.** Мощность голоса фиксируется
  **снапшотом** на момент создания предложения; стейк, взятый после
  снапшота (в т.ч. flash-loan), даёт нулевую мощность — голос отвергается
  (`NoVotingPower`), снапшот не двигается.

## Сценарий `sim/t6-governance.json` (20 предложений, 113 шагов)

Избиратели: 80 000 / 50 000 / 40 000 / 20 000 / 10 000 / 0 `ALCRY`
(суммарный снапшот 200 000; кворум 40 000 = мощность избирателя 2 точно —
граничный случай). 20 предложений: 15 исполнено (валидные проходят),
4 отклонено (большинство против / пропущен кворум / ничья / нет голосов),
1 ветировано за блок до заката; отдельно: отказ в создании ниже порога,
поздний голос на границе, раннее исполнение на границе timelock, veto
после заката (`VetoSunset`), **flash-stake 1 000 000 `ALCRY` посреди
голосования → `NoVotingPower`**, двойной голос, двойной finalize,
двойное исполнение, поздний стейк не двигает снапшот (голос 20 000, не
40 000).

Золотой дайджест
`828b13cf7ca3abcaea65386f7c62e1e155f4d207cb000aceccd86397b5c729aa`
закреплён в `crates/aura-storage/tests/t6_governance.rs` и пересчитан
независимым Python-оракулом `tools/t6_governance_check.py`, который также
запускает пример `t6_governance_example`.

## Negative controls (`tools/nc_t6.py`, 8/8)

| # | Мутация | Named message |
|---|---|---|
| 1 | условие принятия перевёрнуто | `T6: a valid proposal must pass` |
| 2 | снапшот заменён текущим стейком | `T6: a flash-stake attack must be rejected` |
| 3 | порог предложения не проверяется | `T6: a proposal below the threshold must be rejected` |
| 4 | окно голосования +1 блок | `T6: a vote outside the voting period must be rejected` |
| 5 | кворум = 0 | `T6: the quorum must be met for a proposal to pass` |
| 6 | timelock = 0 | `T6: execution must wait for the timelock` |
| 7 | sunset убран | `T6: the veto power must sunset` |
| 8 | дрейф `T6_VOTING_PERIOD_BLOCKS` | `T6: the reference simulation must agree with the pinned digest` |

Эвиденс: `docs/evidence/t6/` (контрольные мутации кумулятивно: **558** за
23 фазы). Патч фазы: `patches/0084-T6-governance.patch`.
