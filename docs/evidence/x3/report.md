# X3 — contract lifecycle (measured)

Generated (UTC): 2026-10-28T20:17:05+00:00. Every number below comes from the fixture
`crates/aura-exec/tests/fixtures/x3_golden.txt`: the modules are assembled
by `tools/gen_x3_vectors.py` (independent Python assembler) and the gas
columns are measured by `cargo run -p aura-exec --bin x3-record` through
the real `Lifecycle`/`Executor` path.

## What landed

* `aura_exec::lifecycle` — `CodeRecord` (the bytecode record), `Lifecycle`
  (`deploy`, `upgrade`, `call_stored`), `LifecycleReceipt`, `LifecycleStatus`
  (`Deployed`, `Upgraded`, `Reverted`) and `LifecycleError` (eight typed
  refusals, each with a stable `code()` string).
* `CodeRecord` layout:
  `magic(8) ALCRYCD1 || format u32 || abi u32 || version u32 || len u32 || code`.
  Decoding is strict: wrong magic, wrong format version, a length that
  disagrees with the payload and trailing bytes are four separate errors.
* `Account::code_hash = BLAKE3(code)` — the identity of the logic. An account
  without code has `Hash::ZERO`, so the question 'is this a contract?'
  needs no side table, and two identical deployments share one hash.
* `init` and `migrate` are optional entry points resolved through
  `Engine::entry_arity`, which answers absent / wrong arity / arity n
  *before* any state is touched.
* Deploy and instantiation are **one session**: a trapping or out-of-gas
  `init` leaves no account, no code and no moved value.
* `upgrade(expected_version)` is optimistic and explicit: it refuses unless
  the caller names the version it upgrades *from*, and it never rewrites
  balance or storage — migration is contract logic, not a chain operation.
* The census is enforced at deploy: `ModuleLimits::hosted()` allows exactly
  the seven X2 ABI imports and bans everything else, so forbidden bytecode
  never reaches state.

## Code record cost

`gas = 1000 + 10 * len(code)`.
Code is priced, not capped: with the Stage 1 budget of 1,000,000 gas a
single transaction can store at most ~100 KiB, so 'unlimited code' is
economically impossible instead of a constant a later phase must renegotiate.

## Programs in the vector set

| program | exports | bytes |
|---|---|---|
| `v1` | `init,run` | 207 |
| `v2` | `init,migrate,run` | 248 |
| `v3` | `migrate,run` | 227 |
| `run_only` | `run` | 186 |
| `trap_init` | `init,run` | 209 |
| `bad_init_arity` | `init,run` | 214 |
| `no_run` | `other` | 160 |
| `float` | `run` | 162 |
| `oog_init` | `init,run` | 179 |

9 programs, 1792 bytes of WASM, 4 without `init`, 2 with `migrate`.

## Measured lifecycle gas

| operation | gas |
|---|---|
| `deploy_v1` | 3560 |
| `call_v1` | 30 |
| `upgrade_v1_v2` | 4527 |
| `call_v2` | 30 |
| `deploy_run_only` | 2860 |

* upgrade costs more than the first deploy (4527 vs 3560) because it runs `migrate`.
* a call is cheap and identical before and after the upgrade (30 vs 30): code version is not rent.
* a code-light deploy is not free (2860): the base price is the deploy, not the bytes.

The recorder writes these numbers **into the fixture**, and
`measured_gas_is_pinned` compares them against a fresh run, so a change in
the price of deploy, migration or a call is a visible fixture diff rather
than a quietly re-baselined number.

## Negative controls

NC-1 version check removed, NC-2 balance not preserved across upgrade,
NC-3 `migrate` never runs (storage not migrated), NC-4 code hash not stored,
NC-5 `init`
never runs, NC-6 census skipped at deploy.
6/6 PASS; total **129** across C0–C10 + S0–S7 + X0–X3.

## Defects the gate found (all fixed in this phase)

1. **The X0 import ban and the X2 ABI could not both be true.** `census`
   banned every import, so the first hosted deploy was refused with
   `censored:host_import(storage_write)` — the ABI could never load. The
   flag `ModuleLimits.allow_abi_imports` keeps the X0 verdicts intact
   (`restrictive()` still bans everything, and the X0 vectors still expect
   `host_import`) while `hosted()` allows exactly the seven ABI functions.
2. **`entry_arity` could not go through `prepare`.** `prepare` requires a
   `run` export, so asking 'does this module have `init`?' about a module
   without `run` returned an instantiation error instead of the answer. It
   now builds the module and checks imports directly; `no_run` reports
   `refused:missing_run_entry` and not a stringly engine error.
3. **A deploy without `init` was a trap.** `execution == None` fell into
   `Trap(Other)`, so a perfectly valid `run`-only contract could never be
   deployed. The receipt for a missing entry point is now `Returned(0)` plus
   the host diff, which is what 'the constructor does nothing' means.
4. **A refusal read as one opaque code.** `LifecycleError::code()` returned
   `refused:censored` both for a banned instruction and for a limit breach.
   It now unwraps the cause (`refused:censored:float_instruction`,
   `refused:limit:init_arity`), which is what the tests assert.

## Honest scope

* **Hosted execution is wasmi-only.** The wasmtime backend still answers
  `SandboxError::EngineUnavailable` for hosted calls; `entry_arity` is
  implemented on wasmi. A cross-engine lifecycle claim needs a wasmtime host
  adapter (X7), not a promise here.
* **No code archive, no self-destruct.** An upgrade overwrites the record and
  the previous logic is gone — there is no on-chain history of code versions.
  Auditing an old version is an indexer problem, not a state one.
* **No authorization.** `deploy`/`upgrade` do not check who calls: there is no
  `caller == owner` rule yet, because accounts and signatures are T-phase.
  `upgrade` is therefore *explicit* but not *permissioned* — written down here
  rather than implied.
* **No factory, no proxy pattern, no sleeps.** Single contract, single pass.
* **Gas is a cost model, not a market.** No refunds, no priority pricing.
* **No SDK.** Contracts are hand-written WASM or generated by the fixture
  assembler (X4 owns the Rust SDK).
* Validators / blocks / transactions remain **0 / 0 / 0**.
