# X1 — sandbox (measured)

Generated (UTC): 2026-10-28T18:55:50+00:00. Every number below comes from the fixture
`crates/aura-exec/tests/fixtures/x1_golden.txt`: the programs are
assembled by `tools/gen_x1_vectors.py` (independent Python assembler) and
the fuel/trace columns are measured by
`cargo run -p aura-exec --bin x1-record` through the real sandbox.

## What landed

* `aura_exec::sandbox` — the pipeline: size check → X0 census → compile →
  zero-import check → instantiate with store limits → call `run` → closed
  trap set. Every step can only reject.
* `aura_exec::limits::SandboxLimits::restrictive()` — fuel 1,000,000,
  memory 4096 pages, module 512 KiB, stack 512 KiB, trace 4096 events.
  `validate()` refuses any value looser than the X0 census ceiling.
* `Engine` trait + two backends: `wasmi` (default) and `wasmtime`
  (feature `wasmtime-engine`). Both are configured from the same
  `SandboxLimits` and return the same `Execution` shape.
* `Execution { outcome, fuel_used, trace, exports }` with
  `Outcome = Returned(i64) | Trap(TrapKind)`.

## Engine decision (D-X1 corrected, measured)

The plan named `wasmtime`. Measured on the development machine (2 GB RAM):

| engine | build | result |
|---|---|---|
| `wasmtime` | Cranelift codegen | `rustc` killed by the OOM killer at
|          | 2 GB, even with `-j1` + `debug = 0`; builds with a 4 GB swap |
|          | file and then passes the whole X1 gate |
| `wasmi` | interpreter | builds in ~20 s, runs every gate without swap |

So the default backend is **wasmi** and `wasmtime` is a feature-gated
second backend, exercised locally with swap and in the `exec-wasmtime` CI
job. Both are configured from the same `SandboxLimits`. Build graph: the
default `aura-exec` graph is 10 crates, the `wasmtime-engine` graph is 307;
`Cargo.lock` pins all 239 external crates either way.

**Cross-engine result (measured):** all 19 fixture programs produce the
same OUTCOME class on both engines
(`wasmtime_backend_agrees_on_outcomes`, run with `--features
wasmtime-engine`). Fuel numbers are `wasmi`-specific and pinned for
`wasmi` only; a shared gas scale is X2 and byte-identical state roots are
X7.

### Defects the gate found (all fixed in this phase)

1. **Missing dispatch feature.** With `wasmi` 2.0.0 built without
   `auto-dispatch` or `portable-dispatch`, a plain `loop { br 0 }`
   overflows the native stack instead of consuming fuel. The fuel gate went
   red on it. The workspace now pins `portable-dispatch` +
   `deterministic`, so an x86 node and an ARM node run the same bytecode the
   same way.
2. **Trap classification was under-specified.** The first classifier missed
   `wasmtime`'s real strings — `"all fuel consumed by WebAssembly"` and
   `"memory fault at wasm address … in linear memory of size …"` — and
   reported a trap as an instantiation error. The cross-engine test caught
   it before the phase closed.
3. **Component model vs reference types.** Disabling `reference-types` while
   the component model stays on makes `wasmtime` reject EVERY module
   ("the component-model-async feature requires the wasm reference-types
   proposal"). The engine config now switches the whole component model
   off, which is also the restrictive choice.

## Program verdicts

| program | input | outcome | fuel | trace |
|---|---|---|---|---|
| `add_mul` | 10 | `return:31` | 76 | 2 |
| `branchy` | 0 | `return:5` | 331 | 2 |
| `const42` | — | `return:42` | 30 | 2 |
| `deep_recursion` | 0 | `trap:trap_stack_exhausted` | 5063 | 2 |
| `div_zero` | — | `trap:trap_div_by_zero` | 53 | 2 |
| `f32_add` | — | `censored:float_instruction` | 0 | 0 |
| `import_env_random` | — | `censored:host_import` | 0 | 0 |
| `mem_grow` | — | `return:2` | 1091 | 2 |
| `mem_oob` | — | `trap:trap_memory_oob` | 66 | 2 |
| `mem_roundtrip` | — | `return:7` | 112 | 2 |
| `mutable_global` | — | `censored:mutable_global` | 0 | 0 |
| `no_run_export` | — | `instantiate` | 0 | 0 |
| `run_two_params` | — | `limit:run_arity` | 0 | 0 |
| `spin` | — | `trap:trap_out_of_fuel` | 1000000 | 2 |
| `start_section` | — | `censored:start_section` | 0 | 0 |
| `sum_to_n` | 1000 | `return:500500` | 13246 | 2 |
| `truncated` | — | `censored:truncated` | 0 | 0 |
| `unreachable` | — | `trap:trap_unreachable` | 37 | 2 |
| `wasi_clock` | — | `censored:host_import` | 0 | 0 |

**6 executed, 5 trapped, 8 refused.** The trapped set is exactly the closed trap vocabulary of X1:
`trap_out_of_fuel`, `trap_unreachable`, `trap_memory_oob`,
`trap_div_by_zero`, `trap_stack_exhausted`. The refused set splits into
X0 census bans (`host_import`, `float_instruction`, `start_section`,
`mutable_global`, `truncated`) and structural failures
(`instantiate`, `limit:run_arity`).

## Negative controls

NC-1 census bypassed, NC-2 fuel budget ignored, NC-3 memory ceiling
removed, NC-4 trap misclassified, NC-5 module-size check removed,
NC-6 trace cap removed. 6/6 PASS; total **116** across
C0–C10 + S0–S7 + X0–X1.

## Honest scope

* **No gas.** Fuel is a metering budget, not a price. X2 introduces the
  gas table, the host ABI v1 and the OutOfGas → revert path (S3).
* **Fuel under-reports on an early trap.** When the budget runs out at
  call entry, `wasmi` keeps the remainder in its interpreter frame and the
  store still reports the full budget (measured: 100-fuel `sum_to_n`
  reports 0 used). It never over-reports, which is the safe direction, but
  X2 must derive gas from a value the store owns and re-measure this case.
* **wasmtime is exercised, but only for outcomes.** Its fuel numbers are
  not comparable to `wasmi`'s and are not pinned; the backend needs a 4 GB
  swap file (or the CI runner) to compile. X7 owns the byte-identical
  cross-engine claim, X2 owns a shared gas scale.
* **No cross-node determinism claim.** X1 shows a single process is
  reproducible (100 identical runs). 8 nodes × 1000 transactions × 3
  platforms is X7.
* **No storage, no lifecycle, no SDK.** A contract cannot read state yet:
  X2 (ABI), X3 (lifecycle), X4 (SDK).
* No consensus, P2P, node, wallet or devnet exists.

Validators / blocks / transactions remain **0 / 0 / 0**.
