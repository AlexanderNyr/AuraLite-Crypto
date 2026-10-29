# ABI v3 — calling another contract

**Version:** 3 (`abi_v3::ABI3_VERSION = 3`) · **Module:** `env3` · **Phase:** X6 ·
**Status:** implemented on the `wasmi` backend (`aura_exec::abi_v3`)

ABI v1 ([`ABI_V1.md`](ABI_V1.md)) gave a contract seven functions and ABI v2
([`ABI_V2.md`](ABI_V2.md)) gave it calldata, return data and identity. Neither
lets one contract **call another**, which is what a router, a pool or any
composed system needs. X6 adds exactly that — and the rules that make it safe.

## 1. Why `env3` and not `env2` v2.1

`env2` is frozen the way `env` was: six functions, their prices, their status codes
and 43 tests that read them. A new surface therefore gets a new **module name**, as
in X5 and as in `wasi_snapshot_preview1`. A contract that never calls another
contract imports neither `env3` nor anything else new and is bit-for-bit
unaffected; the census whitelist is a set of `(module, name)` pairs, so the three
surfaces are enforced independently.

## 2. Imports

| # | name | signature | effect |
|---|---|---|---|
| 1 | `call` | `(addr_ptr, addr_len, data_ptr, data_len, value_lo: i64, value_hi: i64) -> i32` | runs another contract, returns its status |
| 2 | `returndata_len` | `() -> i32` | bytes the last successful call answered |
| 3 | `returndata_read` | `(dst_ptr, dst_len) -> i32` | copies that answer |
| 4 | `depth` | `() -> i32` | this frame's index; a top-level call is `1` |

`addr_len` must be 20 and the address is read from the caller's own memory;
`data_len` is bounded by `MAX_CALLDATA_LEN` (1 KiB) and `await`ed to
`MAX_RETURN_LEN` (1 KiB) on the way back. The `value` is a `u128` split into two
`i64` halves because a wasm function cannot return or take a 128-bit integer
without a pointer, and passing a pointer would let calldata alias with the value.

## 3. Semantics — the two rules

**A frame may not call an address that is already on the stack.** This is `-14`
(`reentrancy`). It is stricter than a per-account lock and much simpler: the
classic withdraw-then-reenter attack becomes *unrepresentable* rather than
*detected*, and the check is a scan of at most eight addresses.

**A stack may not exceed eight frames.** This is `-15` (`call_depth`), published as
`abi_v3::MAX_CALL_DEPTH` and re-checked by the executor. The top-level call is
depth 1, so a chain of eight contracts runs and the ninth refuses.

Both rules live in the **host**, not the SDK: `admit` rejects before a child store
exists, so a contract cannot talk its way past them, and NC-1/NC-2 remove each rule
and require the X6 gate to go red.

## 4. Frames, value and the tree

A call runs in a **child frame**: its own store, its own fuel budget drawn from the
parent's remainder, and its own base version. On success its `StateDiff` is merged
into the parent's (`merge_child`), its events are appended (`merge_events`), and its
return data becomes the parent's `returndata`. On failure the callee's value transfer
is undone (`undo_value`), the return data is cleared and the parent sees a status —
its own frame still commits, because a callee's failure is a *value*, not a trap.

That last sentence is the difference between a transaction and a frame. X2's rule
"any `Returned(_)` commits" is right for a transaction; for a frame, the answer must
reach the caller unchanged. An X6 gate failed until the child's status was propagated
(`-21` from a callee's dispatcher arrives as `-21`, not `0`).

## 5. Status codes

| code | name | meaning |
|---|---|---|
| `-14` | `reentrancy` | the target is already on the stack |
| `-15` | `call_depth` | the stack already holds `MAX_CALL_DEPTH` frames |
| `-16` | `no_code` | the target is an account with no contract |
| `-17` | `callee_failed` | the child trapped or ran out of gas |
| `-8` | `out_of_gas` | the call's own charge exceeded the frame's budget |

Codes `-6`, `-7`, `-20`–`-24` from X5 keep their meaning and are passed through
untouched when they come from a callee.

## 6. What is deliberately missing

* **No callbacks.** Because a frame cannot be re-entered, a payment cannot trigger
  receiver logic in the same transaction. This is a Stage 1 restriction, stated in
  `docs/CALLS.md` and in the honest-scope section of `docs/evidence/x6/report.md`.
* **No gas reservation before the call.** The child's budget is the parent's
  remainder and the charge is settled after the child returns, so a parent cannot
  overspend — but the fee accounting of a multi-frame transaction is the T-phase's.
* **No value path between two contracts in the success case.** `value` is exercised
  on the revert path and on transfer to an EOA; the honest-scope section says which
  assertions exist and which do not.

## 7. Guest side

`aura_sdk::abi3` wraps the four functions:

```rust
let status = call_contract(to, &data, 0);          // u128 value
let len = returndata_len();
let answered = returndata(&mut buf);
let mine = depth();
```

Same memory protocol as v1 and v2: `i32` offsets into the contract's own linear
memory, bounds-checked before anything is written.
