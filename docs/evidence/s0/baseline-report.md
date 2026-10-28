# S0 baseline — measured “no root / no persistence” report

Generated (UTC): 2026-10-26T19:04:52+00:00. Numbers are read from the independent
Python golden fixture `crates/aura-state/tests/fixtures/s0_golden.txt`
and re-verified by `crates/aura-state/tests/s0_baseline.rs`.

This is a MEASUREMENT of the problem S1 has to solve, not a design.
`BaselineState` is an append-only write log with last-write-wins
lookup. It will be replaced by a deterministic, versioned, JMT-like
Merkle tree in S1. Do not build consensus code on it.

## Measured defects

| Property | Measured result | Who fixes it |
|---|---|---|
| State root | `state_root()` → `Err(BaselineError::NoRoot)`; `no_root=1` | S1 (Merkle tree) |
| Persistence | `persist()` / `load()` → `Err(BaselineError::NoPersistence)`; `no_persistence=1` | S2 (RocksDB + WAL) |
| Determinism | write order changes balances; `deterministic=0` | S1 |

## Insertion-order defect (S0 DoD gate)

Two “nodes” apply the SAME set of writes in opposite order:

- Scenario 1: 3 writes → final view differs = `1`, account-1 balance A = `300` aalcry, B = `100` aalcry.
- Scenario 2: 4 writes → final view differs = `1`, account-1 balance A = `30` aalcry, B = `10` aalcry.
- Scenario 3: 3 writes → final view differs = `0`, account-1 balance A = `11` aalcry, B = `11` aalcry.
- Scenario 4: 5 writes → final view differs = `1`, account-1 balance A = `340282366920938463463374607431768211454` aalcry, B = `340282366920938463463374607431768211455` aalcry.

Scenario 3 uses disjoint keys and is order-independent — this guards
against a false positive. The defect is specifically last-write-wins
on conflicting keys with no logical version/ordering.

## Baseline report values

- writes: `3`
- accounts: `1`
- has_root: `0`
- has_persistence: `0`
- deterministic: `0`

## What S0 does NOT provide

- no `state_root`, no proofs, no versioning, no sync;
- no disk writes, no WAL, no crash recovery;
- no deterministic ordering, no canonical state commitment;
- no execution overlay (S3), snapshots (S4), pruning (S6) or backup (S7).

Validators / blocks / transactions remain **0 / 0 / 0**.
