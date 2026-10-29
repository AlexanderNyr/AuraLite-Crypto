# EXEC_PLAN — deterministic WASM machine and contracts

**Status:** ✅ STAGE 1 COMPLETE (X0 ✅ 2026-10-28, X1 ✅ 2026-10-28, X2 ✅ 2026-10-28, X3 ✅ 2026-10-28, X4 ✅ 2026-10-29, X5 ✅ 2026-10-29, X6 ✅ 2026-10-29, X7 ✅ 2026-10-29, X8 ✅ 2026-10-29). ⏸ Stage 2: X9 next, on an owner decision.  
**Stage:** Stage 1: X0–X8. ⏸ Stage 2: X9 (EVM compatibility).  
**Deliverable:** `patches/0020-X0-...` … `patches/0029-X9-...`
**Dependencies:** CORE C2–C5, STATE S1–S3.

## Problem

Smart contracts are the main source of nondeterminism: NaN in `f64`, WASI
clocks, iteration order, "random" host functions, different wasmtime behaviour
on different OSes. One differing NaN and the network diverges mid-block.
The plan first **measures the nondeterminism surface**, then closes it.

## Decisions

- **D-X1.** ~~`wasmtime` with fuel metering~~ **corrected in X1 (measured):** the default engine is `wasmi` 2.0.0 (deterministic interpreter, `portable-dispatch`); `wasmtime` 50.0.0 is the second backend behind the `wasmtime-engine` feature. Cranelift does not compile in 2 GB of RAM, and a gate that cannot run on every machine is not a gate. No WASI, no threads, no floats (banned in X0).
- **D-X2.** Host ABI is versioned; every call costs gas; limits on memory/stack/call depth/log size.
- **D-X3.** A contract compiles to `wasm32-unknown-unknown` from the Rust SDK; the block stores `code_hash`, not "source".
- **D-X4.** A bytecode verifier is a mandatory gate before deploy.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **X0** | Determinism census | table of "what wasmtime can expose nondeterministically" + restrictive config, report | 20 programs (NaN, clocks, imports) — either banned or deterministic |
| ✅ **X1** | Sandbox | engine, memory/stack/fuel limits, zero host imports | NC: contract with `wasi_snapshot_preview1` → `Err` at deploy |
| ✅ **X2** | Gas + ABI v1 | `storage_read/write`, `balance`, `transfer`, `emit`, `hash`, `block_ctx`; gas table; OutOfGas → revert (S3) | 23 vectors, host gas recomputed from an independent table copy; NC: gas table zeroed → red |
| ✅ **X3** | Lifecycle | deploy/instantiate/upgrade/code_hash/version, `init`, storage migration | upgrade preserves balance, changes logic; NC: upgrade without version check → red |
| ✅ **X4** | `aura-sdk` | Rust macros `#[entrypoint]`, typed storage, events, ABI-JSON generation | 3 example contracts build to WASM, size and hash reproducible |
| ✅ **X5** | Reference standards + ABI v2 | ARC-20, ARC-721, ARC-M1 multisig, `env2` (calldata/return/caller/self) | 5 gates: 43 tests over the shipped artifacts (incl. ABI v2 bounds); every refusal writes nothing; NC: 6 tampers each red |
| ✅ **X6** | Call isolation | call depth, reentrancy guard, value transfer, atomic revert of the tree | `env3` (call/returndata/depth), 15 tests: the "evil" contract cannot drain a vault that pays before it debits; NC: guard off → test red |
| ✅ **X7** | Determinism gate | 8 nodes × 1000 contract txs × 3 build configs → byte-identical `state_root` | decisive M0 gate; any divergence = ⛔ NO-GO for mainnet |
| ✅ **X8** | Static verifier | ban floats/forbidden imports/size/stack, error report | 100 mutants: all invalid rejected with the expected code, 6 shipped artifacts accepted |
| ⏸ **X9** | Optional EVM-lane (stretch) | `revm` behind a feature flag, shared state/gas, address mapping | compatibility with 10 known ERC-20 contracts; **not the critical path**, can be moved after M4 |

### X0 receipt (2026-10-28)

- Deliverable: `patches/0020-X0-determinism-census.patch`. No new crate and —
  deliberately — **no engine**: `aura-exec` stops being an empty placeholder
  and gains a hand-written WASM binary classifier.
- Landed `aura_exec::census`: `CENSUS` (16 published rows: source, feature,
  policy, enforcement layer, why), `ModuleLimits::restrictive()`, `Ban`
  (18 variants), `Verdict`, `CensusReport` and `census(bytes, &limits)`.
- **The gate is static and fail-closed.** It checks magic/version, section
  framing and order, then every import, table, memory, global and *every
  instruction of every function body* against a whitelist. An opcode nobody
  reviewed is rejected as `UnknownOpcode` — the risk of the scanner is false
  rejection, never false acceptance.
- **Restrictive configuration:** module ≤ 512 KiB, linear memory must declare
  a maximum ≤ 4096 pages (256 MiB), tables must declare a maximum ≤ 1024
  entries, zero imports, no start function, no mutable globals, no floats, no
  SIMD/atomics/bulk memory, no indirect calls.
- **DoD gate: 21 programs, every one either banned or deterministic** — 5
  allowed (empty module, i32 arithmetic, i64 memory round-trip, locals +
  direct calls, blocks/loops/branches) and 16 banned with an exact reason
  (f32/f64 arithmetic, `f64.const` NaN, `f64` conversion, WASI
  `clock_time_get`, `env::random_get`, shared memory, `memory.atomic.wait`,
  v128 SIMD, bulk `memory.copy`, `call_indirect`, start section, mutable
  global, memory without a maximum, memory above the cap, an unlisted
  opcode). Every program is assembled by an **independent Python assembler**
  (`tools/gen_x0_vectors.py`) from the specification — the crate never builds
  its own test inputs.
- Golden vectors: 72 fixture lines (21 programs × hex/verdict/size plus the
  published limits), offline and byte-stable.
- **Correction found by the gate:** `f32.const`/`f64.const` (0x43/0x44) were
  initially not classified as float instructions, so their immediates were
  read as opcodes and the programs were reported as `unknown_opcode`. The ban
  is now explicit and, more importantly, the float policy lives in exactly one
  function (`float_ban`), which is also what NC-2 tampers with.
- Gate: `make test` -> **184 passed / 0 failed** (177 S7 + 3 unit + 4 gate),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate 95
  external crates, vector inventory 4 415 across 18 fixtures,
  `make audit-closeout` green.
- NC: `make nc-x0` -> **6/6 PASS**: imports, float choke point, start section,
  memory maximum, shared memory, fail-closed whitelist. Total C0–C10 + S0–S7 +
  X0: **110 NC**.
- Evidence: `docs/evidence/x0/gates.txt`, `nc.log`, `report.md` (the census
  table and the 21-program verdict table), `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/x0-dependency-addendum.txt`.
- **Honest scope:** X0 executes nothing. There is no fuel, no memory
  isolation and no engine-level determinism claim; those are X1's gate. The
  census table marks every engine-dependent row with
  `enforcement = X1 (engine)`.
- Deferred: `wasmtime` pin + sandbox + fuel (X1), gas and ABI v1 (X2),
  lifecycle (X3 ✅), SDK (X4), standards (X5), call isolation (X6), the decisive
  multi-node determinism gate (X7), the static verifier with 100 mutants (X8),
  EVM lane (X9, Stage 2).

### X1 receipt (2026-10-28)

- Deliverable: `patches/0021-X1-sandbox.patch`. `aura-exec` gains its first
  external dependency and stops being a static-only crate.
- Landed `aura_exec::sandbox` — the pipeline **size → census → compile →
  zero-import → instantiate → call `run` → trap classification**, where every
  step can only reject — plus `limits::SandboxLimits::restrictive()`
  (fuel 1,000,000; memory 4096 pages; module 512 KiB; stack 512 KiB; trace 4096
  events), `Engine` trait, `Execution { outcome, fuel_used, trace, exports }`,
  `Outcome`, `Event`, `SandboxError` (7 variants) and `TrapKind` (6 variants).
- **Two backends, one configuration.** `wasmi =2.0.0` (default: interpreter,
  features `std + validate + deterministic + portable-dispatch`) and
  `wasmtime =50.0.0` (feature `wasmtime-engine`). Both are built from the same
  `SandboxLimits` and return the same `Execution`, which is what makes X7
  expressible.
- **DoD gate: 19 programs — 6 executed, 5 trapped, 8 refused.** Executed
  (`const42` → 42, `add_mul` → 31, `sum_to_n` → 500 500, `mem_roundtrip` → 7,
  `mem_grow` → 2, `branchy` → 5); trapped (`spin` → `trap_out_of_fuel` at
  exactly 1,000,000 fuel, `unreachable`, `mem_oob`, `div_zero`,
  `deep_recursion` → `trap_stack_exhausted` after 5 063 fuel); refused (WASI
  clock and `env::random_get` → `censored:host_import`, `f32.add` →
  `censored:float_instruction`, start section, mutable global, truncated
  binary, `no_run_export` → `instantiate`, `run_two_params` →
  `limit:run_arity`).
- **Fuel is pinned per program** (30 for `const42`, 13 246 for
  `sum_to_n(1000)`, …) and written by `cargo run -p aura-exec --bin x1-record`,
  so a metering change turns the gate red and becomes a reviewed protocol
  decision instead of a silent `cargo update`.
- **Cross-engine result (measured):** with the feature on and a 4 GB swap file,
  `wasmtime` passes the whole gate and both engines agree on the outcome class
  of all 19 programs (`wasmtime_backend_agrees_on_outcomes`). Fuel is
  `wasmi`-specific; a shared gas scale is X2, byte-identical roots are X7.
- **Three defects found by the gates and fixed in this patch:**
  1. `wasmi` 2.0.0 built **without** a dispatch feature overflows the native
     stack on `loop { br 0 }` instead of consuming fuel — the workspace now pins
     `portable-dispatch` + `deterministic`.
  2. the first trap classifier missed `wasmtime`'s real strings
     (`"all fuel consumed by WebAssembly"`, `"memory fault at wasm address …"`)
     and reported traps as instantiation errors.
  3. disabling `reference-types` while the component model stays enabled makes
     `wasmtime` reject **every** module; the whole component model is now off.
- Gate: `make test` -> **193 passed / 0 failed** (184 X0 + 9 X1 sandbox),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate
  **239 external crates** (95 + 144: wasmi 10, wasmtime 134), vector inventory
  **4 532** across 19 fixtures, `make audit-closeout` green.
- NC: `make nc-x1` -> **6/6 PASS** (census bypass, fuel budget, memory ceiling,
  trap classification, module-size check, trace cap). Total C0–C10 + S0–S7 +
  X0–X1: **116 NC**.
- Evidence: `docs/evidence/x1/gates.txt`, `nc.log`, `report.md` (per-program
  verdict table with measured fuel), `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/x1-dependency-addendum.txt`.
- **Honest scope:** no gas price (X2), no storage ABI (X2), no lifecycle (X3),
  no SDK (X4), no reentrancy protection (X6), no 8-node determinism (X7).
  `fuel_used` under-reports when the budget runs out at call entry (measured:
  0 instead of 100); it never over-reports, and X2 must derive gas from a value
  the store owns.
- Deferred: gas + ABI v1 (X2), lifecycle (X3), SDK (X4), standards (X5), call
  isolation (X6), the decisive multi-node determinism gate (X7), the static
  verifier with 100 mutants (X8), EVM lane (X9, Stage 2).

### X2 receipt (2026-10-28)

- Deliverable: `patches/0022-X2-gas-abi.patch`. A contract can finally **read and
  write state, hold and move value, emit events, and pay for it**.
- Landed `aura_exec::abi` — ABI v1: module `env`, version 1, exactly seven
  functions (`storage_read`, `storage_write`, `balance`, `transfer`, `emit`,
  `hash`, `block_ctx`), a closed status-code set (`0` success, `-1` oob,
  `-2` key too long, `-3` value too long, `-4` topic too long, `-5` not owned,
  `-6` insufficient balance, `-7` overflow, `-8` out of gas, `-11` too many
  events, plus the non-error statuses `-9` absent key and `-10` unknown block
  field), published limits (`MAX_KEY_LEN` 32, `MAX_VALUE_LEN` 4096,
  `MAX_TOPIC_LEN` 64, `MAX_ABI_BUFFER` 65 536) and **one dispatcher** shared by
  every backend.
- Landed `aura_exec::contract` — `HostState` (S3 overlay + call + gas),
  `Executor::call(wasm, base_tree, call, arg)`, `ContractReceipt { status,
  gas_used, host_gas, events, writes, diff, result_root }`, `ExecStatus`
  (`Returned | OutOfGas | Trap | Rejected`).
- **Gas: 1 fuel = 1 gas, one counter.** The instruction meter is reset to
  `gas_left` at every host-call boundary, so instruction gas and host charges are
  two views of the same number. This closes the X1 defect (`fuel_used`
  under-reported when the budget ran out at call entry): the bill is now read
  from the S3 view, which the executor owns.
- **The commit rule:** a successful call commits a canonical `StateDiff`; a trap,
  an out-of-gas and a refusal all leave the state root byte-identical and the
  event list empty. A contract that catches `-8` and returns normally is still
  out of gas (the budget is a rule, not a return value).
- **DoD gate: 23 vectors — 13 returned, 5 out of gas, 3 trapped, 3 refused**
  (`read_absent` → `-9`, `write_then_read` → 5 with the slot committed,
  `balance_other` → 5 000, `transfer_ok` → 900/100, `transfer_insufficient` →
  `-6` with both balances unchanged, `emit_event`, `hash_blake3`, `block_height`
  → 7, `block_bad_field` → `-10`, `out_of_gas_first_call`,
  `oog_between_two_calls` (write charged, read refuses, root unchanged),
  `write_then_spin` (whole budget burned, one write attempted, nothing
  committed), `write_then_trap`, `emit_then_trap`, `import_bad_module`,
  `import_unknown_env`, `import_memory`, `no_memory_export` → `-1`).
- **The gas table is checked against a second, hand-written copy** (Python,
  `tools/gen_x2_vectors.py`): every vector's `expect_host_gas` is recomputed
  from it, and `gas_table_matches_the_independent_copy` requires row-by-row
  agreement with `aura_exec::GAS_TABLE`. Total host gas charged across the
  vector set is pinned in `docs/evidence/x2/report.md`.
- **Three defects found by the gates and fixed in this patch:**
  1. the rollback was **not load-bearing** — the failure branch built its
     receipt from a hard-coded empty event vector, so deleting
     `HostState::rollback` left every test green (NC-4 failed). The receipt now
     reads events *after* the rollback.
  2. a fuel trap was reported as a plain trap even though 1 fuel = 1 gas; both
     an ABI charge failure and an instruction-meter trap now map to `OutOfGas`.
  3. `slot_of` zero-pads keys, so `b"a"` and `b"a\0"` address the same slot.
     Documented and asserted rather than left as an accident.
- Gate: `make test` -> **208 passed / 0 failed** (193 X1 + 9 X2 gate + 6 ABI unit
  tests), fmt/clippy clean, drift guard green, dependency gate **239 external
  crates** (X2 adds zero external crates; `aura-exec` gains three *workspace*
  dependencies), vector inventory **4 555** across 20 fixtures,
  `make audit-closeout` green.
- NC: `make nc-x2` -> **7/7 PASS** (gas table zeroed, transfer not charged, value
  ceiling removed, rollback removed, import census opened, out-of-gas guard
  removed, instruction gas never settled). Total C0–C10 + S0–S7 + X0–X2:
  **123 NC**.
- Evidence: `docs/evidence/x2/gates.txt`, `nc.log`, `report.md` (per-vector
  verdict table with measured gas), `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/x2-dependency-addendum.txt`.
- **Honest scope:** **hosted execution is wasmi-only** — `execute_hosted` is
  implemented on the default backend and the `wasmtime` backend returns
  `SandboxError::EngineUnavailable` for it instead of pretending to run it (the
  `exec-wasmtime` CI job still covers X1 outcome parity; a cross-engine *gas*
  claim needs a wasmtime hosted adapter and a shared metering scale — X7 owns
  byte-identical roots). No fee market or refunds (T-phase), no cross-contract
  calls (X3/X4), no deploy/upgrade lifecycle (X3), no Rust SDK (X4), no
  reentrancy protection (X6).
- Deferred: lifecycle (X3), SDK (X4), standards (X5), call isolation (X6), the
  decisive multi-node determinism gate (X7), the static verifier with 100
  mutants (X8), EVM lane (X9, Stage 2).

### X3 receipt (2026-10-28)

- Deliverable: `patches/0023-X3-lifecycle.patch`. A contract is finally **a
  record in state**: it can be deployed, called by address, upgraded and
  migrated — with permission to run checked at deploy time, not at call time.
- Landed `aura_exec::lifecycle` — `CodeRecord` (`abi_version`, `code_version`,
  `code`), `Lifecycle::{new, deploy, upgrade, call_stored}`, `LifecycleReceipt`
  (`status`, `lifecycle`, `gas_used`, `code_gas`, `diff`, `events`,
  `result_root`, `writes`), `LifecycleStatus` (`Deployed { code_hash,
  code_version } | Upgraded { from, to, code_hash } | Reverted`) and
  `LifecycleError` — eight typed refusals (`NotDeployed`, `AlreadyDeployed`,
  `VersionMismatch`, `InsufficientBalance`, `MissingRunEntry`,
  `Refused(SandboxError)`, `Engine(SandboxError)`) with stable `code()`
  strings.
- **Code record layout**: `magic(8) "ALCRYCD1" || format u32 || abi u32 ||
  version u32 || len u32 || code`. Decoding is strict: bad magic, bad format,
  a length that disagrees with the payload and trailing bytes are four
  different errors, all asserted. `Account::code_hash = BLAKE3(code)` — the
  identity of the logic, not of the record — so "is this a contract?" needs no
  side table.
- **Deploy is atomic with instantiation**: one session stores the code, funds
  the account and runs the optional `init`. A trapping or out-of-gas `init`
  leaves no account, no code and no moved value (asserted for both).
- **Upgrade is optimistic and explicit**: `upgrade(expected_version)` refuses
  unless the caller names the version it upgrades *from* — two concurrent
  upgrades cannot silently race. It rewrites the code record and
  `Account::code_hash` and **never touches balance or storage**; a storage
  migration is contract logic (`migrate(from, to)`), not a chain operation.
  Rolling it back is the X2 rule unchanged.
- **Code is priced, not capped**: `DEPLOY_BASE_GAS + CODE_BYTE_GAS * len(code)`
  (1 000 + 10/byte). With the Stage 1 budget of 1 000 000 gas one transaction
  can store at most ~100 KiB, so "unlimited code" is economically impossible
  instead of a constant a later phase has to renegotiate.
- **Census at deploy**: `ModuleLimits::hosted()` allows exactly the seven X2 ABI
  imports and bans everything else, so forbidden bytecode never reaches state.
- **DoD gate: 9 programs, 12 tests.** Measured lifecycle gas
  (`tests/fixtures/x3_golden.txt`, written by `x3-record`):
  `deploy(v1)=3 560`, `call(v1)=30`, `upgrade(v1→v2)=4 527`, `call(v2)=30`,
  `deploy(run_only)=2 860`. An upgrade costs more than a first deploy because
  it runs `migrate`; a call costs the same before and after it, because code
  version is not rent.
- **Four defects found by the gates and fixed in this patch:**
  1. the **X0 import ban and the X2 ABI could not both be true** — the first
     hosted deploy was refused with `censored:host_import(storage_write)`, so
     the ABI could never load. `ModuleLimits.allow_abi_imports` keeps the X0
     verdicts (`restrictive()` still bans everything; the X0 vectors still
     expect `host_import`) while `hosted()` allows the ABI functions.
  2. **`entry_arity` could not go through `prepare`** — `prepare` requires a
     `run` export, so "does this module have `init`?" answered with an
     instantiation error when the module had no `run`. It now builds the module
     and checks imports directly; `no_run` reports `missing_run_entry`.
  3. **a deploy without `init` was a trap** — `execution == None` fell into
     `Trap(Other)`, so a valid `run`-only contract could never be deployed. The
     receipt is now `Returned(0)` plus the host diff.
  4. **a refusal read as one opaque code** — `code()` returned
     `refused:censored` for a banned instruction and a limit breach alike; it
     now unwraps the cause (`refused:censored:float_instruction`,
     `refused:limit:init_arity`), which is what the tests assert.
- Gate: `make test` -> **221 passed / 0 failed** (208 X2 + 12 X3 gate + 1
  doctest), fmt/clippy clean, drift guard green, dependency gate **239 external
  crates** and **no new workspace edge either** (X3 is code inside the crate
  boundary), vector inventory **4 624** across 22 fixtures,
  `make audit-closeout` green.
- NC: `make nc-x3` -> **6/6 PASS** (version check removed, balance not preserved
  across upgrade, `migrate` never runs so storage is not migrated, code hash not
  stored, `init` never runs, census skipped at deploy). Total C0–C10 + S0–S7 +
  X0–X3: **129 NC**.
- Evidence: `docs/evidence/x3/gates.txt`, `nc.log`, `report.md` (program table +
  measured gas + per-defect analysis), `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/x3-dependency-addendum.txt`.
- **Honest scope:** hosted execution is still **wasmi-only** (the `wasmtime`
  backend answers `EngineUnavailable` for hosted calls and `entry_arity` is
  wasmi-implemented). **No authorization** — `deploy`/`upgrade` do not check
  `caller == owner`, because accounts and signatures are T-phase, so upgrade is
  *explicit* but not *permissioned*. No code archive or self-destruct (the
  previous logic is unrecoverable from state), no factory/proxy pattern, no
  cross-contract calls (X6), no SDK (X4), no fee market or refunds (T-phase).
- Deferred: SDK (X4), standards (X5), call isolation (X6), the decisive
  multi-node determinism gate (X7), the static verifier with 100 mutants (X8),
  EVM lane (X9, Stage 2).

### X4 receipt (2026-10-29)

- Deliverable: `patches/0024-X4-sdk.patch`. A contract is written **in Rust**:
  two new crates (`aura-sdk`, `aura-sdk-macros`), three reference contracts in
  `examples/`, and the three X0 rules a compiler output cannot live without,
  relaxed under the hosted policy only.
- Landed `aura-sdk` — a `no_std` guest library: the seven ABI v1 functions as
  typed Rust (`abi`), fixed-layout `codec` (`u8`/`u16`/`u32`/`u64`/`u128`/
  `i32`/`i64`/`bool`/`[u8; N]`, little-endian, no serde), typed storage
  (`Slot<T>` with a constant `at` key and a composed `Key`) and `events` with
  the published limits checked before the call. On the host the same source
  compiles against a **mock backend** that speaks the node's `extern "C"`
  contract, so the wrappers, the codec and the storage layer are unit-tested.
- Landed `aura-sdk-macros` — `#[entrypoint]`, `#[constructor]`, `#[migrate]`.
  Each renames the annotated function and emits the `extern "C"` export the X3
  lifecycle calls (`run`, `init`, `migrate`), rejecting a wrong arity or a
  non-`u32` return at compile time with a real message.
- **ABI-JSON generation:** `tools/build_contracts.py` writes
  `examples/<name>/abi.json` from the artifact — sha256, size, declared memory
  maximum, imports, entry points with their measured arity. It deliberately does
  not invent a storage schema: keys are the author's, and a declared-storage
  attribute is a later phase rather than a guess here.
- **DoD gate: 3 contracts — `store` 5 187 B, `kv` 5 290 B, `events` 5 033 B**,
  built for `wasm32-unknown-unknown` with release + LTO + `opt-level=z` +
  `panic=abort` + `strip` + `-C link-arg=--max-memory=16MiB`, **byte-identical
  across two independent target directories** at the same checkout path (the
  cross-path result is narrower and is written down below). The `.wasm`
  artifacts are committed and pinned by sha256; the engine gate deploys the
  artifact itself.
- Measured gas (pinned in `tests/fixtures/x4_contracts.txt` by `x4-record`):
  deploy `store` **58 495**, call `store` **13 115**, deploy `kv` **62 353**,
  call `kv` **8 506**, upgrade `kv` **62 261**, deploy `events` **51 330**,
  call `events` **12 940**. Deploy is exactly `1 000 + 10 × bytes` plus `init`.
- **Four defects found by the gates and fixed in this patch:**
  1. **the census rejected every Rust contract** — a mutable global
     (`__stack_pointer`), `0xFC` bulk memory (`memcpy` → `memory.copy`) and
     `call_indirect` (`core::fmt`) are all in *any* `rustc` output. The hosted
     policy now allows exactly one mutable global, `memory.copy`/`memory.fill`
     and `call_indirect` **only when the element table contains no import**, so
     a function pointer can never reach host code. `restrictive()` is untouched,
     so every X0 verdict stands.
  2. **`rustc` memory has no declared maximum** — and X0 refuses unbounded
     memory for good reason. The canonical build passes
     `-C link-arg=--max-memory=16MiB`; a contract now carries its own ceiling.
  3. **the engine was stricter than the census** — `wasmi`'s `Config` had
     mutable globals, bulk memory and reference types off, so a module the
     census accepted was refused by the backend (`bulk memory support is not
     enabled`, then `zero byte expected` at the `call_indirect`). Configuration
     flags inside the pinned engine version, not a version limit.
  4. **`storage` could silently truncate** — a slot written as `u64` and read as
     `u32` returned the low half. The codec is now length-exact and NC-3 proves
     the check is load-bearing.
- Gate: `make test` -> **253 passed / 0 failed** (221 X3 + 4 SDK integration + 13 SDK gate + 6
  codec unit + 3 census policy + 1 contracts doctest + 1 ABI-JSON gate + 4
doctests), fmt/clippy clean, drift guard green, dependency gate **239 external
  crates unchanged** (syn/quote/proc-macro2 were already in the lock) and **17
  local packages**, vector inventory **4 624** across 22 fixtures,
  `make audit-closeout` green.
- NC: `make nc-x4` -> **6/6 PASS** (`#[entrypoint]` exports the wrong name, the
  SDK ignores the absent-key status, the codec stops length-checking, the
  mutable-global allowance is removed, the element-table rule switched off, the
  pinned artifact hash not checked). Total C0–C10 + S0–S7 + X0–X4: **135 NC**.
- Evidence: `docs/evidence/x4/gates.txt`, `nc.log`, `report.md` (artifact table,
  measured gas, the three-rule analysis), `inventory.txt`, `SHA256SUMS`,
  `docs/evidence/x4-dependency-addendum.txt`.
- **Honest scope:** hosted execution is still **wasmi-only**; a contract cannot
  allocate (`no_std`, no allocator — `Vec`/`String` in a contract is a future
  decision); no `serde` and no derives beyond the three attributes; a change to
  the codec is a **contract-visible** change because a stored value does not
  record its own type; `call_indirect` is bounded but not verified (X8); the SDK
  has no cross-contract calls (X6) and no `caller` checks (T-phase);
  reproducibility is **path-scoped and measured** — a clean clone at another
  path reproduced `kv` but not `store`/`events`, and which contracts differ is
  not stable across experiments, because cargo folds the package path into
  `-C metadata`. The gate therefore claims only *same toolchain, same flags,
  same path*.
- Deferred: standards (X5), call isolation (X6), the decisive multi-node
  determinism gate (X7), the static verifier with 100 mutants (X8), EVM lane
  (X9, Stage 2).

### X5 receipt (2026-10-29)

- Deliverable: `patches/0025-X5-standards.patch`. Three standards written in Rust
  against `aura-sdk`, and **ABI v2** because a standard cannot exist without it.
- **The protocol decision of this phase:** ABI v2 lives in the import module
  **`env2`**, not in a new version of `env`. X3's lifecycle passes one `i32`, and a
  token needs a selector, arguments and an answer; but `env` is frozen by the X2
  receipts and 23 golden vectors. A module name carries a version the way
  `wasi_snapshot_preview1` does: a v1 contract is bit-for-bit unaffected, the v2
  whitelist is enforced separately by the census, and a module declares the surface
  it speaks by what it imports. See [`docs/ABI_V2.md`](../ABI_V2.md).
- Six functions: `calldata_len`, `calldata_read`, `return_write`, `caller`,
  `self_address`, `abi_version`; 20 gas base, 3 gas/byte for the two copying calls;
  1 KiB calldata and 1 KiB return data, both enforced **before** the engine runs.
- A contract needs `caller` because `transfer(from, …)` with a caller-supplied
  `from` is permission to spend anyone's tokens, and `self_address` because a
  multisig that cannot name itself cannot ask whether it can cover a payment —
  a gate failed until that function existed.
- **DoD gate — three standards, 41 225 bytes total:** `arc20` 12 733 B, `arc721`
  13 224 B, `multisig` 15 268 B, each its own workspace, each with a pinned
  `abi.json`, all three deployed and driven **through the real `Lifecycle`** by
  43 tests in five files (`x5_arc20` 11, `x5_arc721` 10, `x5_multisig` 9,
  `x5_abi2` 8, `x5_artifacts` 5 for the pins) — 296 in the workspace, 253 of them
  inherited from X4.
- Measured gas: ARC-20 deploy 140 694 / mint 56 410 / transfer 55 951 /
  balance_of 39 221; ARC-721 deploy 145 604 / mint 63 946 / transfer 71 514;
  ARC-M1 deploy 162 108 / policy 56 066 / propose 72 930 / approve 76 281.
- **The rule this phase had to name:** X2 commits any `Returned(_)`, negative
  included. So a standard's refusal is a status code **and zero writes** — every
  gate asserts the empty change set and unchanged tree entries, not "the balance
  looks right". A trap would throw the reason away; a write-then-fail would be a
  fund-loss bug.
- Defects the gate found: an ARC-721 `approve` that trapped on **every** call (a
  40-byte event buffer holding a 16-byte amount), the missing `self_address` above,
  `RUSTFLAGS` used for a wasm-only linker flag (breaks any host build script that
  recompiles), and a test harness that forgot to commit a diff between calls.
- Honest scope: no cross-contract calls, so no true cross-contract reentrancy test —
  the ordering and the executed-flag are the shape of the defence and are what X6
  builds on; the multisig policy is immutable; allowance and vote keys use 80 bits of
  BLAKE3 because a slot is 32 bytes; failures commit an empty diff and advance the
  version.
- Reproducibility, measured rather than asserted: the builder compiles each standard
  twice into independent target directories and refuses to pin a hash that does not
  reproduce. A **clean clone at another path** was tested on 2026-10-29 and reproduced
  `arc20` byte for byte, while `arc721` (13 224 B) and `multisig` (15 268 B) kept their
  size and permuted their type/function sections — cargo folds the SDK's absolute path
  into `-C metadata`. The claim is therefore "reproducible at the recorded path", the
  same shape as X4, and path-independence is X7's.
- Evidence: `docs/evidence/x5/` (gates, report, inventory), 6/6 NC, **141** NC
  total across C0–C10 + S0–S7 + X0–X5, 4 659 vectors across 23 fixtures, 239
  external crates unchanged (the standards add no lock entry).
- Deferred: call isolation (X6), the decisive multi-node determinism gate (X7),
  the static verifier with 100 mutants (X8), EVM lane (X9, Stage 2).

### X6 receipt (2026-10-29)

- Deliverable: `patches/0026-X6-call-isolation.patch`. Three hostile-and-honest
  contracts and **ABI v3**, because a call surface cannot exist without it.
- **The protocol decision of this phase:** ABI v3 lives in the import module
  **`env3`**, and it carries two rules enforced by the *host*, not the SDK:
  a frame may not call an address already on the stack (`-14`), and a stack may
  not exceed `MAX_CALL_DEPTH = 8` (`-15`). `env` and `env2` are untouched; a v1 or
  v2 contract is bit-for-bit unaffected. See [`docs/ABI_V3.md`](../ABI_V3.md) and
  [`docs/CALLS.md`](../CALLS.md).
- Four functions: `call` (100 + 3/byte), `returndata_len` (20), `returndata_read`
  (20 + 3/byte), `depth` (20). The `u128` value is passed as two `i64` halves —
  a pointer would let value alias with calldata.
- **DoD gate — three contracts, 30 459 bytes total:** `vault` 9 759 B, `evil`
  11 607 B, `recurse` 9 093 B, each its own workspace, each built twice into
  independent target directories and pinned by `sha256` **and** by the node's own
  BLAKE3 (`x6-record` computes it over the shipped bytes). 15 tests in
  `x6_calls`, 311 in the workspace.
- Measured gas, through the real `Lifecycle`: vault deploy 107 369 / evil deploy
  117 100 / recurse deploy 100 132 / fund 41 188 / **attack 87 697** / honest
  withdrawal after the attack 35 116 / nine-frame descent 28 414.
- **What the attack proves, measured:** the vault pays its receiver before it
  debits, then notifies it with `env3.call`; the node refuses the notification
  with `-14` because the receiver's frame is live, so the attacker's counter stops
  at **one** attempt, its ledger is debited once, the vault ends **empty rather
  than short**, and the honest depositor is paid her 9 800 in full.
- The depth limit is measured on a chain of **nine distinct copies** (a contract
  may not call itself, so self-recursion is not even expressible): the eighth copy
  runs at depth 8, its `down` gets `-15`, the ninth never starts.
- Defects the gate found: the `wasmi` engine's `check_imports` had its own copy of
  the allowed modules and refused `env3` (the census was right, the engine's copy
  had drifted); the child's status was being dropped, so a vault that answered
  `-14` looked like success to its caller; the vault's first draft paid *through*
  `env3.call`, which cannot pay an EOA at all — money and control are two
  different calls; and the first depth fixture called itself, which the reentrancy
  rule correctly refuses.
- Honest scope: no callbacks into a live frame (a Stage 1 restriction, not an
  oversight); gas is charged after the callee returns, so multi-frame fee
  accounting is the T-phase's; the successful contract→contract value path is
  `env.transfer` inside the vault, and a single call that both sends value and
  runs code is not tested and not claimed.
- Evidence: `docs/evidence/x6/` (gates, report, inventory), 6/6 NC, **147** NC
  total across C0–C10 + S0–S7 + X0–X6, 4 692 vectors across 24 fixtures, 239
  external crates unchanged, `docs/evidence/x6-dependency-addendum.txt` recording
  that X6 adds no dependency at all.
- Deferred: the decisive multi-node determinism gate (X7), the static verifier
  with 100 mutants (X8), EVM lane (X9, Stage 2).

### X7 receipt (2026-10-29)

- Deliverable: `patches/0027-X7-determinism-gate.patch`. A **gate**, not a
  subsystem: `aura_exec::detcheck` (scenario generator + replay driver), the
  `x7-run` binary, a pinned fixture and 11 tests. No new crate, no new dependency.
- **The claim, in one sentence and one constant:** the same ordered input, replayed
  by independent nodes, produces the same `state_root`, gas bill and events. The
  input is a pure function of `detcheck::X7_SEED` — SplitMix64, no clock, no
  entropy, no iteration order in generation.
- **DoD gate — 1 004 ordered steps (4 deploys + 1 000 transactions), 8 processes:**
  890 commits, 60 refusals, 54 failures (19 out of gas, 35 no-code, 0 traps), gas
  46 375 290, state root `efca4222d37347ff…`. All eight **separate** processes
  agree on the root, the gas, the events chain and the rolling digest — separate
  because Rust's per-process hasher seed and allocator history are exactly the kind
  of thing two in-process runs share.
- **Three configurations, honestly named:** `debug` (overflow checks on),
  `release` (opt-level 3) and `release-cgu16` (16 codegen units) all reproduce the
  same root, gas and digest, two processes each (`make x7-matrix`). Three *operating
  systems* cannot be measured from one sandbox, so the OS dimension is a CI job
  (`.github/workflows/ci.yml` → `determinism`) and is reported as **unproven** until
  it runs.
- The gate executes the **shipped** artifacts — `kv`, `store`, `events`, `vault` —
  and re-verifies their BLAKE3 on every run, so X7 also re-pins X4 and X6.
- Every step is classified as committed / refused / failed, and the failure split is
  kept (`machine=19 trap=0 lifecycle=35`), because two nodes disagreeing about
  *which* transactions were accepted would produce different roots: the counters
  exist to attribute a divergence, not merely to report one.
- Defects the gate found: the deployer constant was not a funded account (one byte,
  `InsuffientBalance` on the fourth deploy); passing an argument to `run()` was a
  lifecycle error, so a third of the workload never ran (890 commits instead of
  559 — the mix is now measured); the vault's `withdraw` was sent an address in
  front of its amount, so every withdrawal refused with `-21`; and the first
  negative control was green until it was rewritten to remove the *root* from the
  digest fold.
- Evidence: `docs/evidence/x7/` (gates, report, inventory), 6/6 NC, **153** NC
  total across C0–C10 + S0–S7 + X0–X7, 4 692 vectors across 24 fixtures, 239
  external crates unchanged.
- Honest scope: a gate is not a node (no consensus, mempool, block structure or
  P2P); concurrency is untested because it does not exist; the root includes the
  version, so the gate fixes the input rather than permuting blocks; different CPUs
  are not exercised — that is X8's job.
- Deferred: the static verifier with 100 mutants (X8), EVM lane (X9, Stage 2).

### X8 receipt (2026-10-29)

- Deliverable: `patches/0028-X8-static-verifier.patch`. `aura_exec::verify` — a
  **pure function** over bytes, not an engine: `verify(&bytes, &VerifyLimits) ->
  Report`. No new crate, no new dependency, no execution.
- **22 stable refusal codes**, each with a witness: `bad_magic`, `bad_version`,
  `module_too_large`, `section_order`, `section_unknown`, `section_truncated`,
  `float_type`, `import_not_allowed`, `global_type`, `memory_limits`,
  `function_code_mismatch`, `body_truncated`, `opcode_banned`, `stack_underflow`,
  `stack_too_deep`, `label_depth`, `local_index`, `global_index`, `call_index`,
  `opcode_budget`, `locals_budget`, `export_shape`. `Report` carries the verdict,
  the size, function and instruction counts, the peak stack depth and one
  `Problem { code, offset, detail }` per defect — the offset is a byte offset into
  the **module**, so a reader can check it against the file.
- **DoD gate — 100 mutants, 15 kinds of damage, zero mismatches.** The mutator
  (`tools/gen_x8_mutants.py`) is an independent Python section walker: it parses the
  module itself, damages exactly one thing and states the verdict it expects. It
  covers magic, version, float type, float local, import module, global type, memory
  maximum, opcode subset, call/local/label indices, export shape, function/code
  count, size ceiling and truncation — six mutants per kind over the six shipped
  artifacts, with a second round for the five kinds whose first six do not cover
  every artifact: 100 exactly.
- **The six shipped artifacts are re-verified, not remembered:** `kv` 33 functions /
  1 905 instructions, `store` 29 / 1 921, `events` 25 / 1 871, `vault` 53 / 3 692,
  `evil` 61 / 4 235, `recurse` 51 / 3 331 — all accepted, peak stack depth 7, peak
  body 630 instructions. Those numbers are pinned in the gate, so a change in a
  shipped contract fails the build instead of silently rewriting the evidence.
- **Depth checks, not a type check** — stated plainly in the module doc, in the
  report and here: the operand stack is tracked as a *length*, with callee
  signatures resolved out of the type section; the engines remain the authority on
  instantiation. X8 guarantees the engine is never asked to instantiate something
  outside the accepted subset; it does not replace the engine's validator.
- **Two real defects found while building it.** `call` originally popped nothing and
  pushed nothing, so `store.wasm` was refused with `stack_underflow` and every stack
  number was arithmetic on a fiction (NC-1 now reddens a named test if that
  regresses); and body offsets were relative to the body slice rather than the
  module, so every in-body problem pointed at the wrong byte.
- Evidence: `docs/evidence/x8/` (gates, report, inventory), 6/6 NC, **159** NC total
  across C0–C10 + S0–S7 + X0–X8.
- Honest scope: no phase calls the verifier yet — wiring it into the deploy path is
  D-phase work, so the claim is "a verifier exists and is measured", not "deploys are
  verified". The budget constants (200 000 instructions per body, 4 096 stack slots,
  1 024 locals) are policy, set far above the measured peaks, and will be tightened
  when a gas model can price them. There is no differential run against
  `wasmparser`; the corpus is the 100 mutants plus the 6 shipped modules.
- Deferred: EVM lane (X9, Stage 2), and the node phase (D) that actually calls this.

## Risks

- WASM fuel metering may differ between wasmtime versions → engine version pinned, a change = new `protocol_version` + migration.
- Gas prices not matching real cost → DoS. Calibration in `PERF_PLAN` P1/P2.
- Reentrancy/upgrade logic → formal review checklist in `SECURITY_PLAN` H4.

## Non-goals

ZK, privacy, EVM compatibility as an obligation (X9 is stretch), wallet
frontend (WALLET).

## Closure criteria

The same block of contract txs produces one `state_root` on 8 nodes and 3 OSes;
every nondeterministic input is either banned or canonicalized.

Stage 1 status: 8 processes × 3 build configurations agree (X7, measured); the OS
dimension is a CI job and stays **unproven** until it runs. The subset is closed by
the census (X0) and enforced statically by the verifier (X8), which is measured on
100 mutants. Both are prerequisites, neither is a node: wiring the verifier into the
deploy path belongs to the D phase.
