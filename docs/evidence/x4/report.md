# X4 — Rust contract SDK (measured)

Generated (UTC): 2026-10-29T07:39:09+00:00. Every number below comes from
`crates/aura-exec/tests/fixtures/x4_contracts.txt`: the size, hash, imports
and exports come from `tools/build_contracts.py` reading the built artifact,
and the gas lines come from `cargo run -p aura-exec --bin x4-record` through
the real `Lifecycle`.

## What landed

* `aura-sdk` — a `no_std` contract SDK: the seven ABI v1 functions as typed
  Rust, typed storage (`Slot<T>`, composed `Key`), a fixed-layout codec, and
  events with the published limits checked before the call.
* `aura-sdk-macros` — `#[entrypoint]`, `#[constructor]`, `#[migrate]`: each
  renames the annotated function and emits the `extern "C"` export the X3
  lifecycle calls (`run`, `init`, `migrate`). No other code generation.
* Three reference contracts in `examples/` (`store`, `kv`, `events`), each its
  own workspace, built for `wasm32-unknown-unknown` with release + LTO +
  `opt-level=z` + `strip`.
* The shipped `.wasm` artifacts are committed and **pinned** by hash: the
  engine gate deploys the artifact, not a source file.

## Contracts (measured from the artifacts)

| contract | bytes | sha256 | imports | exports | memory max (pages) |
|---|---|---|---|---|---|
| `store` | 5187 | `a084a0d84179fd3b…` | 3 | `memory,init:0,run:0` | 256 |
| `kv` | 5290 | `3db941f9ec24193e…` | 2 | `memory,init:1,migrate:2,run:1` | 256 |
| `events` | 5033 | `fd724c60730dc156…` | 4 | `memory,run:0` | 256 |

Three contracts, 15510 bytes in total — small enough that a deploy is
comfortable inside the Stage 1 budget of 1,000,000 gas.

## Measured gas

| operation | gas |
|---|---|
| `deploy_store_gas` | 58495 |
| `deploy_store_code_gas` | 52870 |
| `call_store_gas` | 13115 |
| `deploy_kv_gas` | 62353 |
| `call_kv_gas` | 8506 |
| `upgrade_kv_gas` | 62261 |
| `deploy_events_gas` | 51330 |
| `call_events_gas` | 12940 |

Deploy gas is `1000 + 10 × bytes` for the code plus whatever `init` costs;
the recorder pins both, and the engine gate asserts the formula and the
measured total.

## The three X0 rules a Rust contract cannot live without

The first time the engine gate deployed a real contract it was refused three
times in a row. Each refusal was a genuine conflict between X0's policy and
*any* compiler output, and each is now handled by relaxing the **hosted**
policy while leaving `restrictive()` — and therefore every X0 vector —
exactly as shipped.

| rule | why rustc needs it | how it is bounded |
|---|---|---|
| mutable global | `__stack_pointer` is a mutable i32 global | `max_mutable_globals` = 1; a global is private to one instantiation and reset every call |
| `0xFC` bulk memory | every slice copy lowers to `memory.copy` | only `memory.copy`/`memory.fill` (and the data-count section); `trunc_sat`, `memory.init`, `data.drop`, `table.*` stay banned |
| `call_indirect` | `core::fmt`'s integer formatting compiles to a table call | the element table may contain only functions **defined in this module** — never an import, so a function pointer can never reach host code |

The engine had to be told the same thing: `wasmi`'s `Config` had mutable
globals, bulk memory and reference types off, so a module the census had
accepted was refused by the compiler backend. The policy is the census's
job; the engine now mirrors it.

## Negative controls

NC-1 `#[entrypoint]` exports the wrong name, NC-2 the SDK ignores the
absent-key status, NC-3 the codec stops length-checking, NC-4 the
mutable-global allowance is removed, NC-5 the element-table rule is switched
off, NC-6 the pinned artifact hash is not checked.
6/6 PASS; total **135** across C0–C10 + S0–S7 + X0–X4.

## Defects the gate found (all fixed in this phase)

1. **The census rejected every Rust contract** — three separate rules (see
   the table above). Without this phase's gate, the SDK would have shipped
   code that could not be deployed.
2. **`storage` without `--max-memory` is undeployable.** `rustc` declares a
   minimum and no maximum, and X0 refuses unbounded memory (rightly: memory
   growth is a DoS lever). The canonical build now passes
   `-C link-arg=--max-memory=16MiB`, so a contract carries its own ceiling.
3. **The engine was stricter than the census.** After the census was
   relaxed, `wasmi` still refused the module (`bulk memory support is not
   enabled`, then `zero byte expected` for `call_indirect`). Both were
   `Config` flags, not version limits — recorded so nobody "fixes" it by
   bumping the engine.
4. **`entry_arity` and the export name were both untested against a real
   toolchain.** The wrappers now carry the exact names and arities X3 asks
   for, and NC-1 pins that.

## Honest scope

* **Hosted execution is still wasmi-only.** wasmtime answers
  `EngineUnavailable` for hosted calls; a wasmtime parity run is X7.
* **A contract cannot allocate.** There is no allocator: values are
  fixed-layout and stack-buffered. `String`/`Vec` in a contract is a future
  decision, not an oversight.
* **No `serde`, no derives beyond the three attributes.** The encoding is
  published (`aura_sdk::codec`) and a *change to it is a contract-visible
  change*, because a stored value does not record its own type.
* **`call_indirect` is allowed but not verified.** The census proves the
  table cannot reach an import; it does not prove the target is the one the
  author meant. That is X8's static verifier.
* **Reproducibility is path-scoped, and the scope is measured.** Two clean
  builds of the same tree at the same path are byte-identical for all three
  contracts. At a *different* checkout path the hashes can differ (cargo
  folds the package path into `-C metadata`, which reorders monomorphised
  functions) and **which contracts differ is not stable**: one cross-path
  experiment left only `events` different, a second (a clean clone of the
  released tree) left `store` and `events` different while `kv` matched.
  The claim the gate enforces is therefore the narrow one: same toolchain,
  same flags, same checkout path ⇒ byte-identical. Full path-independence
  needs a canonical build path or a normalised artifact (X5/X7).
* **No SDK for cross-contract calls, no auth in the SDK.** `caller` checks
  are T-phase (accounts/signatures), cross-contract calls are X6.
* Validators / blocks / transactions remain **0 / 0 / 0**.
