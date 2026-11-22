# T3 — fees: EIP-1559-like `base_fee` + `tip`, burned base, elastic target

**Status:** ✅ T3 closed. Over the pinned **10 000 congested blocks** the
base fee rises and falls **per the formula** at every single step (phase
gate, literal). The base part of every fee is burned; only the tip goes
to the validator.

## The market (`sim/t3-fees.json`)

| Parameter | Value |
|---|---|
| initial `base_fee` | 1 000 000 000 base units (1 gwei-equivalent) |
| gas target | 15 000 000 gas (elastic) |
| elasticity / block limit | ×2 → 30 000 000 gas |
| max base fee change | ±1/8 per block (±12.5 %) |
| tip per gas | 1 000 000 000 base units |
| scenario | 10 000 blocks, 250-block segments cycling **+25 %, −25 %, +50 %, −50 %, target** (bursts, lulls, quiet blocks) |

**Formula** (EIP-1559, integer arithmetic): at `gas == target` the fee is
unchanged; otherwise `base ± max(base·|gas−target|/target/8, 1)` — a
change of at least one base unit whenever the block is off target,
floored at zero on the way down.

**Golden path digest**
`412f1f9e120ec829fb5d2ae65e4590da69771cd7b49bef83f29440d0d93bc124`
(sha256 over per-block lines `index:base_before:gas:base_after:burned:tips`)
is pinned in the Rust tests and re-derived by the independent Python
oracle (`tools/t3_fees_check.py`), which also cross-runs the Rust
example `t3_fees_example`. Checkpoints: base after block 249 = 2 192 756
374 619, after block 499 = 783 284 026, final = 56 473; peak 2 993 325
499 252 235; total burned 2 039 212 976 242 596 352 500 000; total tips
150 000 000 000 000 000 000.

## Rules

* **Per-step formula:** every one of the 10 000 steps is re-checked
  against an independent in-test re-derivation; congested segments
  strictly rise, slack segments strictly fall, exact-target segments are
  flat.
* **Burn:** `burned = gas_used × base_fee` — the base part leaves
  circulation; `paid == burned + tips` always.
* **Validator income:** `tips = gas_used × tip_per_gas` **only**.
* **Elastic limit:** `check_gas` rejects any block above 30 000 000 gas.

## Verification

* 9 gate tests (`crates/aura-storage/tests/t3_fees.rs`): the 10 000-block
  gate, direction semantics, burn, tip, elastic limit, pinned digest,
  and metamorphisms (at-target blocks are flat with the minimum
  one-unit delta off target; splitting a transaction preserves burn,
  tips and total; higher constant demand → strictly higher base fee).
* Six mutation controls (`tools/nc_t3.py`): wrong formula denominator,
  unburned base, uncredited tip, swapped directions, unenforced limit
  and drifted constants each turn the gate red with their named message.

## What T3 deliberately does not do

* No staking/validator-set mechanics (T4) and no governance over the
  parameters (T6, D-T1 timelock). The tip is a payment for inclusion,
  not a promise of returns (D-T2).
* No legal claims (D-T4).
