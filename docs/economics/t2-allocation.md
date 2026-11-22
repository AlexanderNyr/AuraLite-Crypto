# T2 — genesis allocation: validators, treasury, community, team

**Status:** ✅ T2 closed. The allocation table reconciles **exactly** with
the T1 genesis supply (3 000 000 `ALCRY`) — any extra coin anywhere turns
the gate red (phase NC, literal). Vesting cliffs and lockups are enforced,
not decorative.

## The table (`sim/t2-allocation.json`)

| Pot | Amount | Vesting | At month 12 |
|---|---|---|---|
| validators | 1 200 000 `ALCRY` (40 %) | — | fully liquid |
| treasury | 600 000 `ALCRY` (20 %) | linear 24 months, 6-month cliff | 150 000 liquid / 450 000 locked |
| community | 750 000 `ALCRY` (25 %) | — | fully liquid |
| team | 450 000 `ALCRY` (15 %) | linear 36 months, 12-month cliff | 150 000 liquid / 300 000 locked |
| **sum** | **3 000 000 `ALCRY`** | | = genesis supply |

Golden table digest `5fe7417a134011e7028f255cf7cf180335cc23c773ee9657a68de260fcc9ea41`
is pinned in the Rust tests and re-derived by the independent Python
script (`tools/t2_allocation_check.py`) — the same script that performs
the phase gate: **reconciling genesis balances with the table**.

## Rules

* **Reconciliation:** unique pot names, `sum(pots) == genesis supply`
  exactly — no extra coins, no rounding dust. The "extra coin" mutation
  turns the gate red with `T2: the allocation must sum to the genesis
  supply`.
* **Vesting:** `vested(m) = 0` before the cliff; `amount × m / total`
  truncated after it; exactly `amount` at and beyond `total_months` (no
  dust, no over-issue).
* **Lockups:** the unvested part of a pot is **locked** and cannot be
  spent; `check_spend` refuses any spend above the liquid part.

## Verification

* 8 gate tests (`crates/aura-storage/tests/t2_allocation.rs`): sum,
  vesting ≤ pot, cliff, completion, lockups, reference digest, and two
  metamorphisms (splitting a pot preserves the total and the lock
  profile; monthly deltas telescope to the vested amount).
* Six mutation controls (`tools/nc_t2.py`): extra coins, vesting
  overflow, cliff removal, non-completing vesting, ignored lockup and
  table drift each turn the gate red with their named message.

## What T2 deliberately does not do

* No fee markets (T3) or staking mechanics (T4). Validator rewards are
  paid from the validators pot under T1/T4 parameters — no promises of
  returns (D-T2).
* No legal claims (D-T4).
