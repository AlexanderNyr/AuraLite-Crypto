# X2 — gas model + ABI v1 (measured)

Generated (UTC): 2026-10-28T19:26:03+00:00. Every number below comes from the fixture
`crates/aura-exec/tests/fixtures/x2_golden.txt`: the modules are assembled
by `tools/gen_x2_vectors.py` (independent Python assembler, independent
copy of the gas table) and the gas columns are measured by
`cargo run -p aura-exec --bin x2-record` through the real executor.

## What landed

* `aura_exec::abi` — ABI v1: module `env`, version 1, seven functions
  (`storage_read`, `storage_write`, `balance`, `transfer`, `emit`, `hash`,
  `block_ctx`), a closed status-code set (`0`, `-1`…`-8`, `-9`, `-10`,
  `-11`), published limits (`MAX_KEY_LEN` 32, `MAX_VALUE_LEN` 4096,
  `MAX_TOPIC_LEN` 64, `MAX_ABI_BUFFER` 64 KiB) and one dispatcher that
  every backend shares.
* `aura_exec::contract` — `HostState` (overlay + call + gas),
  `Executor::call` (budget, host, commit/rollback), `ContractReceipt`
  (`status`, `gas_used`, `host_gas`, `events`, `writes`, `diff`,
  `result_root`).
* Gas: **1 fuel = 1 gas**, one counter. The instruction meter is reset to
  `gas_left` at every host-call boundary, so instruction gas and host
  charges are two views of the same number and cannot double-count.
* The rollback rule: a successful call commits a canonical `StateDiff`;
  anything else (trap, out of gas, refusal) leaves the root byte-identical
  and the event list empty.

## Gas table (ABI v1)

| function | base | per byte | note |
|---|---|---|---|
| `storage_read` | 100 | 10 | bounded by the bytes actually returned |
| `storage_write` | 200 | 25 | the expensive op: it moves the state root |
| `balance` | 50 | 0 | one account lookup |
| `transfer` | 300 | 0 | two account writes |
| `emit` | 200 | 15 | topic + data |
| `hash` | 100 | 5 | BLAKE3 over the payload |
| `block_ctx` | 20 | 0 | reads consensus fields only |

Host gas charged across the whole vector set: **0**. The same
numbers are recomputed in Python from a hand-written copy of this table
(`tools/gen_x2_vectors.py`), and `gas_table_matches_the_independent_copy`
requires the two to agree row by row.

## Vector verdicts

| vector | status | host gas | total gas | writes | events |
|---|---|---|---|---|---|
| `read_absent` | `?` | ? | ? | ? | ? |
| `write_then_read` | `?` | ? | ? | ? | ? |
| `read_existing` | `?` | ? | ? | ? | ? |
| `key_too_long` | `?` | ? | ? | ? | ? |
| `value_too_long` | `?` | ? | ? | ? | ? |
| `ptr_out_of_bounds` | `?` | ? | ? | ? | ? |
| `balance_other` | `?` | ? | ? | ? | ? |
| `balance_self` | `?` | ? | ? | ? | ? |
| `transfer_ok` | `?` | ? | ? | ? | ? |
| `transfer_insufficient` | `?` | ? | ? | ? | ? |
| `emit_event` | `?` | ? | ? | ? | ? |
| `hash_blake3` | `?` | ? | ? | ? | ? |
| `block_height` | `?` | ? | ? | ? | ? |
| `block_bad_field` | `?` | ? | ? | ? | ? |
| `out_of_gas_first_call` | `?` | ? | ? | ? | ? |
| `oog_between_two_calls` | `?` | ? | ? | ? | ? |
| `write_then_spin` | `?` | ? | ? | ? | ? |
| `write_then_trap` | `?` | ? | ? | ? | ? |
| `emit_then_trap` | `?` | ? | ? | ? | ? |
| `import_bad_module` | `?` | ? | ? | ? | ? |
| `import_unknown_env` | `?` | ? | ? | ? | ? |
| `import_memory` | `?` | ? | ? | ? | ? |
| `no_memory_export` | `returned:-1` | 0 | 104 | 0 | 0 |

**15 returned, 3 out of gas, 2 trapped, 3 refused** — 23 vectors in total.

## Negative controls

NC-1 gas table zeroed, NC-2 transfer not charged, NC-3 value ceiling
removed, NC-4 rollback removed on failure, NC-5 import census opened,
NC-6 out-of-gas guard removed, NC-7 instruction gas never settled.
7/7 PASS; total **123** across C0–C10 + S0–S7 + X0–X2.

## Defects the gate found (all fixed in this phase)

1. **The rollback call was not load-bearing.** The failure branch built its
   receipt from a hard-coded empty event vector, so deleting
   `HostState::rollback` left every test green (NC-4 failed). The receipt
   now reads the event list *after* the rollback, so the rollback is the
   thing that empties it and NC-4 goes red when it is removed.
2. **A fuel trap was not out-of-gas.** With 1 fuel = 1 gas, a contract that
   burns its budget in pure computation trapped `trap_out_of_fuel` and was
   reported as a plain trap. The executor now maps both an ABI charge
   failure and an instruction-meter trap to `OutOfGas`, and a contract that
   catches `-8` and returns normally is still out of gas (NC-6).
3. **Key padding has a documented collision.** `slot_of` zero-pads a key to
   32 bytes, so `b"a"` and `b"a\0"` are the same slot. That is now written
   down in `abi::slot_of` and asserted in a unit test, so changing it is a
   decision rather than an accident.

## Honest scope

* **Hosted execution is wasmi-only.** `execute_hosted` is implemented on the
  default backend; the `wasmtime` backend returns
  `SandboxError::EngineUnavailable` for hosted calls instead of pretending.
  The `exec-wasmtime` CI job still runs the X1 outcome-parity suite. A
  cross-engine *gas* claim needs a `wasmtime` hosted adapter and a shared
  metering scale — that is X2b/X7 work, not something this phase can
  measure on a 2 GB machine without a 4 GB swap file.
* **Gas is a cost model, not a market.** No fee market, no refunds, no
  priority pricing: a transaction pays `gas_used` and that is T-phase work.
* **One contract per call, no nesting.** `transfer` moves balance; it does
  not call another contract. Cross-contract calls are X3/X4.
* **No lifecycle.** There is no upload/deploy/upgrade path yet (X3), so the
  executor takes `(wasm bytes, base tree, call)` directly.
* **No SDK.** Contract authors write WASM by hand or with the fixture
  assembler (X4 owns the Rust SDK and the `no_std` guest crate).
* Validators / blocks / transactions remain **0 / 0 / 0**.
