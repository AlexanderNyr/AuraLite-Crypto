# ABI v2 — calldata, return data, caller, self

**Version:** 2 (`abi_v2::ABI2_VERSION = 2`) · **Module:** `env2` · **Phase:** X5 ·
**Status:** implemented on the `wasmi` backend (`aura_exec::abi_v2`)

ABI v1 ([`ABI_V1.md`](ABI_V1.md)) gave a contract seven functions, one `i32` of
input and no output beyond a status. That is enough for a counter and not enough
for a token: a standard needs a method selector, arguments wider than 32 bits, an
answer the caller can read, and the address that signed the transaction.

## 1. Why a new module name and not a new version of `env`

`env` is frozen. Its seven functions, their arities, their prices and their status
codes are checksummed in `docs/evidence/x2/`, and 23 golden vectors were generated
against them. Renaming or re-pricing anything there would invalidate a closed
phase's receipts for no benefit.

So the extension lives in a **different import module**, which is how WebAssembly
surfaces have always been versioned (`wasi_snapshot_preview1` is the canonical
example). The consequences are the ones you want:

* a v1 contract imports `env` only and is bit-for-bit unaffected — it does not
  even see that v2 exists;
* the census whitelist is a *set of `(module, name)` pairs*, so the two surfaces
  are enforced independently;
* a module declares which surface it speaks by what it imports, which is visible
  in the bytecode without a metadata convention.

A contract may import from both modules; the standards do.

## 2. Imports

| # | name | signature | effect |
|---|---|---|---|
| 1 | `calldata_len` | `() -> i32` | length of the call's calldata |
| 2 | `calldata_read` | `(dst_ptr: i32, dst_len: i32) -> i32` | copies calldata, returns bytes copied |
| 3 | `return_write` | `(src_ptr: i32, len: i32) -> i32` | sets the call's return data |
| 4 | `caller` | `(out_ptr: i32, out_len: i32) -> i32` | writes the 20-byte signer address |
| 5 | `self_address` | `(out_ptr: i32, out_len: i32) -> i32` | writes the 20-byte contract address |
| 6 | `abi_version` | `() -> i32` | `2` |

Same memory protocol as v1: `i32` byte offsets into the contract's own linear
memory, bounds-checked before anything is written. A bad range is `-1`
(`OutOfMemory`) and touches nothing.

## 3. Semantics

**Calldata** is fixed before execution and immutable during it, so every node
reads the same bytes. It is not state: it is not in the diff, and it does not
survive the call. `calldata_read` copies `min(dst_len, calldata_len)` bytes — a
short buffer truncates, it does not fail — so a contract that must see everything
compares the answer with `calldata_len` first.

**Return data** is the call's answer. It is written only by `return_write`, the
last write wins, and it reaches the caller **only if the call commits**: a trap or
an out-of-gas drops it, so "returned 0" and "reverted after writing" cannot be
confused. `ContractReceipt::return_data` and `LifecycleReceipt::ret` carry it.

**`caller`** is the address that signed the transaction. It is the only trustworthy
source of "who is asking": a standard that takes `from` from calldata is not a
transfer, it is permission to spend anyone's tokens.

**`self_address`** is the address the code is deployed at. A contract cannot read
its own account balance without naming itself, and a multisig must ask "can I
cover this?" before it marks a proposal executed.

## 4. Limits

| limit | value | enforced where |
|---|---|---|
| `MAX_CALLDATA_LEN` | 1 024 bytes | `Session::set_calldata` and `Lifecycle::call_data`, **before** the engine runs |
| `MAX_RETURN_LEN` | 1 024 bytes | `return_write` |
| address | 20 bytes | `caller`, `self_address` (`-13` when the buffer is smaller) |

One kilobyte of calldata is far more than any standard needs — ARC-20's widest
method is `transfer_from(from, to, amount)` = 4 + 20 + 20 + 16 = 60 bytes — and
small enough that the per-call copy cannot become a way to buy unbounded work.

## 5. Gas

| function | base | per byte |
|---|---|---|
| `calldata_len` | 20 | 0 |
| `calldata_read` | 20 | 3 |
| `return_write` | 20 | 3 |
| `caller` | 20 | 0 |
| `self_address` | 20 | 0 |
| `abi_version` | 20 | 0 |

The charge is taken **before** the effect, from the same single counter as
instruction metering and ABI v1, so a call cannot do work and then discover it
could not pay for it.

## 6. Status codes

| code | meaning |
|---|---|
| `>= 0` | success; for `calldata_read` it is the number of bytes copied |
| `-1` | a pointer/length pair pointed outside linear memory |
| `-3` | the value is longer than the published limit (`return_write`) |
| `-8` | out of gas — recorded on the host and turned into a trap by the engine |
| `-13` | the output buffer is smaller than the value the host must write |

Codes `-2`, `-4`…`-7`, `-9`…`-12` are v1's and keep their v1 meanings; v2 does not
redefine them.

The **SDK** adds two of its own above the ABI, because they are produced by
contract code, not by the host: `-20` (`BAD_SELECTOR`) and `-21` (`BAD_ARGS`). See
[`STANDARDS.md`](STANDARDS.md) §2.

## 7. What v2 does not do

* no cross-contract call (`call`, `delegate`, static call) — that is X6, and it is
  the phase that makes reentrancy a real question;
* no event filtering, no logs by topic — `env.emit` already covers emission;
* no arbitrary-length storage read — `env.storage_read` into a caller-owned buffer;
* no `caller` of *the previous* frame: with no cross-contract calls there is
  exactly one meaning for `caller`, and X6 must decide what the nested meaning is.
