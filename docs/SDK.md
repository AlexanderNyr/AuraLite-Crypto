# Writing a contract — the `aura-sdk` guide (X4)

A contract is a Rust crate, `#![no_std]`, compiled to `wasm32-unknown-unknown`.
It talks to the chain through **ABI v1** ([`ABI_V1.md`](ABI_V1.md)); `aura-sdk`
is that ABI, typed.

```rust
#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

const COUNT: Slot<u64> = Slot::at(b"count");

#[constructor]
pub fn init() -> u32 {
    COUNT.write(0);
    0
}

#[entrypoint]
pub fn run() -> u32 {
    let next = COUNT.get_or(0) + 1;
    COUNT.write(next);
    emit_or_abort(b"count", &next.to_le_bytes());
    next as u32
}
```

## The three attributes

| attribute | export | arity | called by |
|---|---|---|---|
| `#[entrypoint]` | `run` | 0 | `Lifecycle::call_stored(base, call, None)` |
| `#[entrypoint(arg)]` | `run` | 1 | `Lifecycle::call_stored(base, call, Some(n))` |
| `#[constructor]` | `init` | 0 | `Lifecycle::deploy(.., None)` |
| `#[constructor(arg)]` | `init` | 1 | `Lifecycle::deploy(.., Some(n))` |
| `#[migrate]` | `migrate` | 2 | `Lifecycle::upgrade(..)` with `(from, to)` |

An entry point returns `u32`. That value is the call's `ExecStatus::Returned(n)`
— it is **not** an error channel: the negative status codes belong to the host.
To refuse a call, trap, which reverts everything:

```rust
if amount > balance { abort() }          // or: unwrap(result)
```

A wrong arity, a wrong name or a non-`u32` return is a **compile error** with a
message that says which export wanted what.

## Many methods in one contract: `dispatch!`

A stored contract has one exported `run`, so a contract with several methods puts a
**4-byte little-endian selector** at the front of its calldata and dispatches on it.
ABI v2 (`env2`, [`ABI_V2.md`](ABI_V2.md)) is what lets a contract see that calldata
and answer with data.

```rust
#[entrypoint]
pub fn run() -> u32 {
    dispatch! {
        SEL_TRANSFER => transfer([u8; 20], u128),
        SEL_BALANCE_OF => balance_of([u8; 20]),
        SEL_TOTAL_SUPPLY => total_supply(),
    }
}
```

The macro decodes each argument in order and refuses, rather than guesses:

| situation | answer |
|---|---|
| calldata shorter than 4 bytes | `-20` (`BAD_SELECTOR`) |
| selector not in the table | `-20` |
| too few bytes for the declared arguments | `-21` (`BAD_ARGS`) |
| **any** byte left over | `-21` |

Trailing bytes are refused on purpose: a method's meaning must never depend on what
a later version appends. The argument types are ordinary [`codec`](ABI_V1.md) types,
so the same fixed little-endian layout applies to calldata and to storage.

Two functions a multi-method contract almost always needs come from `env2`:

```rust
let who = unwrap(caller());        // the address that signed the transaction
let me  = unwrap(self_address());  // the address this code is deployed at
unwrap(reply(&value.to_le_bytes())); // the answer, delivered only if the call commits
```

`caller` is not a convenience: a standard that takes `from` from calldata is
permission to spend anyone's tokens. `self_address` exists because a contract that
cannot name itself cannot read its own balance — a multisig asks exactly that.

### Refusals: a code, and nothing written

An entry point returns `u32`. Every `Returned(_)` — negative included — is a
*committed* call under the X2 rule, so a refusal must be a code **and zero writes**.
Check first, write second, and let the gate assert the empty change set. `abort()`
is for bugs, not for business refusals: a trap throws the reason away.

## Storage

`Slot<T>` is a key plus a type. Two ways to name the key:

```rust
const TOTAL: Slot<u128> = Slot::at(b"total");                    // constant
let slot: Slot<u64> = Slot::with_key(Key::new(&[b"bal:", &addr]).unwrap()); // composed
```

* `get()` → `Ok(None)` when absent, `Ok(Some(v))` when present, `Err(..)` when the
  read failed or the stored value was written by a different layout.
* `get_or(default)` / `write(v)` turn any error into a **trap**. A contract that
  cannot read its own state must not continue with an invented value.
* `set(v)` / `clear()` are the explicit forms.

Keys are zero-padded to 32 bytes by the host, so `b"a"` and `b"a\0"` are the
same slot — a documented property, not an accident. `Key::new` refuses an empty
part and a key over 32 bytes.

## Encoding

Fixed layout, little endian, **length-exact**:

| type | bytes |
|---|---|
| `u8` / `bool` | 1 |
| `u16` | 2 |
| `u32` / `i32` | 4 |
| `u64` / `i64` | 8 |
| `u128` | 16 |
| `[u8; N]` | N |

A value does not record its own type, so **changing a slot's type is a contract
change**: read the old bytes with the old layout and write the new one in
`migrate`. A `bool` that is neither `0` nor `1` is a decoding error rather than
"truthy".

There is **no allocator**: values are fixed-layout and stack-buffered by design.

## Events and consensus data

```rust
emit(b"transfer", &amount.to_le_bytes())?;   // topic <= 64 B, data <= 4096 B
let height = block_height()?;                // consensus, identical on every node
let ts = block_timestamp_ms()?;
let digest = hash(b"payload")?;              // BLAKE3, computed by the node
```

Limits are checked *before* the host call, so an oversized topic is an error at
the call site. On a trap every event is discarded with the rest of the call.

## Building

```bash
python3 tools/build_contracts.py     # or: make build-contracts
```

The canonical flags are not optional:

* `--target wasm32-unknown-unknown`, `--release`, `lto = true`,
  `opt-level = "z"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`;
* `-C link-arg=--max-memory=16MiB` — the census refuses linear memory without a
  declared maximum, and `rustc` emits only a minimum;
* `--remap-path-prefix` so the artifact does not depend on the checkout path.

The tool rebuilds twice, refuses to pin a hash when the two builds differ, writes
`examples/<name>/abi.json` and pins size + sha256 in
`crates/aura-exec/tests/fixtures/x4_contracts.txt`.

## What is allowed in bytecode (and what is not)

Deploy runs the X0 census with `ModuleLimits::hosted()`. Compared to the strict
X0 policy it relaxes exactly four things, each because `rustc` cannot produce a
contract without it, and each bounded:

| relaxed | bound |
|---|---|
| ABI imports | only the seven `env` functions of ABI v1 |
| mutable global | at most one (`__stack_pointer`) |
| `memory.copy` / `memory.fill` | those two only; `trunc_sat`, `memory.init`, `data.drop`, `table.*` stay banned |
| `call_indirect` | the element table may contain only functions defined in the module — never an import |

Still banned: floats, SIMD, atomics, threads, the start section, unbounded
memory or tables, `wasi_*` imports, and any opcode outside the whitelist.

## Cost

Deploy costs `1 000 + 10 × bytes` plus whatever `init` does. A 5 KiB contract is
therefore ~52 000 gas to store — five percent of a 1 000 000-gas transaction.
Measured numbers for the reference contracts live in
[`evidence/x4/report.md`](evidence/x4/report.md).

## Limits of this phase

* wasmi is the only backend that executes host calls (wasmtime says so instead of
  pretending; parity is X7).
* No cross-contract calls, so no `caller` of a previous frame (X6).
* `call_indirect` is bounded but not verified — the census proves the table cannot
  reach an import, not that the target is the one you meant (X8).
* No cross-contract calls (X6), no `caller`/owner checks (accounts and signatures
  are T-phase), no allocator, no serde.
