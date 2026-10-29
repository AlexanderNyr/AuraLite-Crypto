# EXEC_PLAN — deterministic WASM machine and contracts

**Status:** 🚧 IN PROGRESS (X0 ✅ 2026-10-28, X1 ✅ 2026-10-28, X2 ✅ 2026-10-28, X3 ✅ 2026-10-28; X4 next)  
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
| **X4** | `aura-sdk` | Rust macros `#[entrypoint]`, typed storage, events, ABI-JSON generation | 3 example contracts build to WASM, size and hash reproducible |
| **X5** | Reference standards | ARC-20, ARC-721, minimal multisig | vector table: transfer/mint/burn/approve/ownerOf + reentrancy test |
| **X6** | Call isolation | call depth, reentrancy guard, value transfer, atomic revert of the tree | "evil" contract cannot drain others' funds; NC: guard off → test red |
| **X7** | Determinism gate | 8 nodes × 1000 contract txs × 3 platforms → byte-identical `state_root` | decisive M0 gate; any divergence = ⛔ NO-GO for mainnet |
| **X8** | Static verifier | ban floats/forbidden imports/size/stack, error report | 100 mutants: all invalid rejected, valid accepted |
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
  boundary), vector inventory **4 595** across 21 fixtures,
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
