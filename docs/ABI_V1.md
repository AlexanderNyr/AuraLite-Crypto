# ABI v1 — the contract host interface

**Version:** 1 (`ABI_VERSION = 1`) · **Module:** `env` · **Phase:** X2 ·
**Status:** implemented on the `wasmi` backend (`aura_exec::abi`)

This is the contract between a deployed WASM module and the chain. It is a
**protocol parameter**: changing a signature, a status code, a limit or a price
requires a new `ABI_VERSION` and a migration test — never a patch release.

`docs/spec/` is frozen C10 evidence, so the ABI is published here.

## 1. Imports

Every function is imported from the module `env`, takes `i32` arguments and
returns an `i32` status. Nothing else may be imported: an unknown
`env.*` name, a different module, or a memory/global/table import is a
**refusal at deploy** (`SandboxError::ImportsNotAllowed`), not a link error.

| # | name | signature | effect |
|---|---|---|---|
| 1 | `storage_read` | `(key_ptr, key_len, out_ptr, out_len) -> i32` | reads the contract's own slot into `out`; returns bytes written, or `-9` if absent |
| 2 | `storage_write` | `(key_ptr, key_len, val_ptr, val_len) -> i32` | writes `value` to the contract's own slot; empty value deletes |
| 3 | `balance` | `(addr_ptr, addr_len, out_ptr) -> i32` | writes a 16-byte little-endian `u128` balance |
| 4 | `transfer` | `(addr_ptr, addr_len, amount_ptr) -> i32` | moves `u128` from the **contract** to `addr` |
| 5 | `emit` | `(topic_ptr, topic_len, data_ptr, data_len) -> i32` | appends one event |
| 6 | `hash` | `(data_ptr, data_len, out_ptr) -> i32` | BLAKE3 of the payload, 32 bytes |
| 7 | `block_ctx` | `(field, out_ptr) -> i32` | writes one `u64` consensus field |

`block_ctx` fields: `0` = block height, `1` = block timestamp (ms, consensus
time, never a clock), `2` = `ABI_VERSION`. Any other field returns `-10`.

## 2. Status codes

The status is the function's return value. `0` is success; a negative value is a
**defined refusal, not a trap** — the contract sees it and may handle it. A trap
is reserved for engine-level failures (out of gas, unreachable, memory fault).

| code | name | meaning |
|---|---|---|
| `0` | success | |
| `-1` | `out_of_memory` | a pointer/length pair is outside linear memory, or a buffer exceeds the cap |
| `-2` | `key_too_long` | key longer than 32 bytes |
| `-3` | `value_too_long` | value longer than 4096 bytes |
| `-4` | `topic_too_long` | topic longer than 64 bytes |
| `-5` | `not_owned` | the slot is not in the contract's own namespace |
| `-6` | `insufficient_balance` | the contract cannot cover the transfer |
| `-7` | `overflow` | an amount or counter would overflow |
| `-8` | `out_of_gas` | the budget is exhausted; the call will be rolled back |
| `-9` | **absent** (not an error) | `storage_read` found no value |
| `-10` | **unknown field** (not an error) | `block_ctx` field is not defined |
| `-11` | `too_many_events` | the event cap was reached |

`-8` is terminal: the budget does not refill, and a contract that catches it and
returns normally is still recorded as `OutOfGas` (the state is rolled back).

## 3. Limits

| limit | value | enforced in |
|---|---|---|
| `MAX_KEY_LEN` | 32 bytes | `abi::slot_of` |
| `MAX_VALUE_LEN` | 4096 bytes | `abi` storage write |
| `MAX_TOPIC_LEN` | 64 bytes | `abi` emit |
| `MAX_ABI_BUFFER` | 65 536 bytes | every guest-memory access |
| `MAX_EVENTS` | 4096 per call | `aura_state` |
| address length | 20 bytes | `aura_types::Address` |
| amount encoding | 16-byte little-endian `u128` | `abi` |

### Key identity

A storage key **is** its zero-padded 32-byte slot: keys are bounded by 32 bytes
and trailing NUL bytes are not significant, so `b"a"` and `b"a\0"` are the same
slot. This keeps slots readable in proofs and ordered by key bytes. The
consequence is bounded to one contract's own namespace; a contract that needs
both keys must encode a length prefix itself. This is asserted in a unit test so
that changing it is a decision, not an accident.

## 4. Gas

1 gas = 1 fuel. There is **one** counter per call: the engine's instruction meter
and the host charges draw on the same number, and the meter is reset to
`gas_left` at every host-call boundary.

| function | base | per byte | note |
|---|---|---|---|
| `storage_read` | 100 | 10 | charged for the bytes actually returned |
| `storage_write` | 200 | 25 | the expensive op: it moves the state root |
| `balance` | 50 | 0 | one account lookup |
| `transfer` | 300 | 0 | two account writes; charged even when it fails |
| `emit` | 200 | 15 | topic + data |
| `hash` | 100 | 5 | BLAKE3 over the payload |
| `block_ctx` | 20 | 0 | consensus fields only |

A call that is rejected before it does work (key too long, value too long,
pointer outside memory, missing memory export) is **not** charged: a contract
cannot be billed for work the host refused to do.

An exhausted budget is billed in full (`gas_used == gas_limit`); partial billing
would let a failing call buy unbounded work.

## 5. Commit and rollback

| outcome | state | events | receipt |
|---|---|---|---|
| `Returned(v)` | a canonical `StateDiff` is committed | returned | `status = returned:v` |
| `OutOfGas` | rolled back; root unchanged | empty | `status = out_of_gas`, `gas_used = gas_limit` |
| `Trap(kind)` | rolled back; root unchanged | empty | `status = trap:<code>` |
| `Rejected` | never ran | empty | `status = rejected` |

`writes` in the receipt counts the writes the call **performed**, including ones
that were rolled back — "tried twice, committed nothing" is a different fact
from "never wrote". `diff: None` is what says nothing was committed.

## 6. Minimal contract

```wat
(module
  (import "env" "storage_write" (func $write (param i32 i32 i32 i32) (result i32)))
  (import "env" "storage_read"  (func $read  (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1 16)
  (data (i32.const 0) "counter")
  (data (i32.const 16) "hello")
  (func (export "run") (result i32)
    (drop (call $write (i32.const 0) (i32.const 7) (i32.const 16) (i32.const 5)))
    (call $read (i32.const 0) (i32.const 7) (i32.const 64) (i32.const 64))))
```

This is vector `write_then_read` in
`crates/aura-exec/tests/fixtures/x2_golden.txt` (23 vectors, assembled by
`tools/gen_x2_vectors.py`, gas measured by `cargo run -p aura-exec --bin
x2-record`).

## 7. What v1 does not have

* No cross-contract calls and no reentrancy guard (X6).
* No deploy/upgrade lifecycle, no `init`, no code hash in the account yet (X3).
* No fee market, refunds or priority pricing: gas is a cost model (T-phase).
* No iteration over storage, no typed storage, no Rust SDK (X4).
* Hosted execution exists on the `wasmi` backend only.
