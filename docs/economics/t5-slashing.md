# T5 — Slashing and insurance (ECONOMICS_PLAN)

**Status:** ✅ closed (2026-11-21). **Gate:** 7 scenarios: penalty ≥ damage,
fund never negative — met literally.

## Decisions

- **Penalty parameters.** An incident (double sign, downtime, etc.) causes
  measurable `damage`. The offender's penalty = `damage × T5_PENALTY_BPS / T5_BPS`
  = **150%** — `penalty ≥ damage` holds by construction and is checked
  on each of the 48 incidents of the pinned run (including the floor edges
  `damage = 10^18 ± 1`).
- **Insurance fund.** Victims are covered from the `InsuranceFund`:
  `covered = min(damage, balance)`; the uncovered remainder is `shortfall`
  (recorded, not lost). Replenishment: **5000 bps** of the applied penalty
  (after the mass-slash cap) goes to the fund. Invariant: **the fund balance never
  goes negative** (signed `i128`, checked at every step).
- **Mass-slash protection.** No window can destroy more than
  **500 bps** of registered bonded (the parameter `T5_REGISTERED_BONDED`
  = 1 000 000 `ALCRY` → cap = **50 000 `ALCRY`/window**). Penalty excess is
  **deferred** into a queue (`pending`) — never forgiven — and collected by
  the following windows (`advance`) until the queue is empty
  (`total_applied == total_penalty`, `pending == 0` at the end of the run).

## Scenario `sim/t5-slashing.json` (7 scenarios, 87 steps, 48 incidents)

1. a single double-sign (100 `ALCRY` of damage);
2. a repeat offender (10 × 50);
3. a correlated mass-slash (20 × 5 000) drained over 3 windows;
4. fund-exhaustion pressure (3 × 60 000) drained over 6 windows;
5. penalty floor edges (10^18+1 and 10^18−1);
6. the mass-slash cap across several windows (8 × 20 000);
7. an exhaustion-and-replenish chain (500 000 + 250 000, drain, re-
   covering 1 000/2 000, a final drain).

The golden digest
`4cc9ea0319374c3c2c74bc88db6d7747b23f0e21ea637ae2270bc4c5707c9cef`
is pinned in `crates/aura-storage/tests/t5_slashing.rs` and recomputed
by the independent Python oracle `tools/t5_slashing_check.py`, which also
runs the `t5_slashing_example` example. The finale: fund **536 750 `ALCRY`**,
the queue is empty, `total_applied == total_penalty == 1 790 402 999 999 999
999 999 999`, covered **558 451 499 999 999 999 999 998**, uncovered
**635 150 500 000 000 000 000 002**; the fund dipped exactly to 0
(step 60) and recovered — the "never negative" invariant holds at
every point.

## Negative controls (`tools/nc_t5.py`, 7/7)

| # | Mutation | Named message |
|---|---|---|
| 1 | `T5_PENALTY_BPS` 15 000 → 9 999 | `T5: the penalty must cover the damage` |
| 2 | the coverage clamp removed (`covered = damage`) | `T5: the insurance fund must never go negative` |
| 3 | `T5_MASS_SLASH_CAP_BPS` 500 → 5 000 | `T5: the mass-slash cap must hold in every window` |
| 4 | the deferred part forgiven (`pending = 0`) | `T5: deferred penalties must not be dropped` |
| 5 | fund income +1 | `T5: the insurance income must be exactly the fund share of the applied penalty` |
| 6 | the penalty formula /10 001 | `T5: the penalty must be exactly damage times the pinned parameter` |
| 7 | `T5_FUND_SHARE_BPS` 5 000 → 5 001 | `T5: the reference simulation must agree with the pinned digest` |

Evidence: `docs/evidence/t5/` (mutation controls cumulative: **550** over
22 phases). Phase patch: `patches/0083-T5-slashing.patch`.
