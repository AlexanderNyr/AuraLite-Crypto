# T4 — staking: min stake, delegation, commission, unbonding, slashing

**Status:** ✅ T4 closed. **1000 delegators** drive the pool and their
shares sum to **exactly one** pool at every step of the pinned run
(phase gate, literal); a **withdrawal without unbonding is rejected**
(phase NC, literal). Stage 1 of ECONOMICS is complete with T4.

## The model (`sim/t4-staking.json`)

| Parameter | Value |
|---|---|
| min stake | 1 000 `ALCRY` |
| validator commission | 1000 bps (10 %) — **from rewards only** |
| unbonding | 21 days = 362 880 blocks (5 s block) |
| slashing period | = the unbonding window (bonded + not-yet-withdrawn stake) |
| scenario reward | 2 `ALCRY` per step (variable parameter, D-T2) |

Mechanics: delegators hold **shares** of one validator pool; rewards
compound into `bonded` (share price moves, share counts do not); the
commission is skimmed off each reward, never off the principal.
`undelegate` burns shares and queues the proportional amount for
withdrawal exactly 362 880 blocks later; `withdraw` claims matured
entries — with no unbonding stake it fails (`NoUnbonding`), before
maturity it fails (`StillUnbonding`). `slash(rate_bps)` cuts the bonded
pool and every unbonding entry proportionally.

**Golden path digest**
`e80662c82c74e1d1a0a5b0624beb4cf08af88c24f8091751c277cc7ea6887f07`
(2004 scenario steps: 1000 delegations of 1000–1999 `ALCRY`, 600 reward
steps, slashes of 100/500 bps, partial undelegations, one rejected
early-withdrawal attempt one block before maturity, matured
withdrawals). Checkpoints: bonded after slash 100 bps = 1 484 861 400
000 000 000 000 000; after slash 500 bps = 1 325 529 559 769 089 696
565 650; final bonded 1 325 889 559 769 089 696 565 650; final shares =
Σ shares = 1 408 708 333 333 333 333 333 367; withdrawn total 85 430
770 230 910 303 434 449; validator commission 120 000 000 000 000 000
000. The independent Python oracle (`tools/t4_staking_check.py`)
re-implements the model from scratch and cross-runs the Rust example
`t4_staking_example`.

## Rules

* **Shares sum to one:** `Σ delegator shares == total_shares` at every
  step of 1000 delegators, rewards, slashes and undelegations.
* **Withdrawal needs unbonding:** no unbonding stake → `NoUnbonding`
  (red); early claim → `StillUnbonding` (red); exactly after 21 days →
  claimable.
* **Commission from rewards only:** `pending_commission = Σ reward ×
  10 %`; the principal is never cut.
* **Min stake:** a delegation below 1 000 `ALCRY` is rejected.
* **Slashing period:** bonded and unbonding (not yet withdrawn) stake is
  slashed proportionally for the whole window.

## Verification

* 10 gate tests (`crates/aura-storage/tests/t4_staking.rs`): the
  1000-delegator gate, withdrawal-without-unbonding rejection, the exact
  21-day boundary (362 879 red / 362 880 green), commission split, min
  stake, proportional slash (incl. 0 % and 100 %), slashability of
  unbonding stake, the pinned digest, and metamorphisms (splitting a
  delegation preserves the shares; consecutive slashes compose
  proportionally).
* Seven mutation controls (`tools/nc_t4.py`): share accounting drift,
  allowed withdrawal without unbonding, shortened unbonding window,
  commission taken twice, dropped min-stake check, non-proportional
  slash and parameter drift each turn the gate red with their named
  message.

## What T4 deliberately does not do

* No multi-validator set, redelegation or governance (T6/B-series); one
  pool per validator is the model. Rewards are variable parameters, not
  promised income (D-T2); parameter changes are simulated (D-T3) and
  stay governance-locked (D-T1).
* No legal claims (D-T4).
