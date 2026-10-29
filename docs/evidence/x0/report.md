# X0 — determinism census (measured)

Generated (UTC): 2026-10-28T17:39:22+00:00. The verdicts below are produced by
`crates/aura-exec/src/census.rs` and pinned by
`tools/gen_x0_vectors.py`, an independent Python WASM assembler.

This report lives in `docs/evidence/x0/`, not `docs/spec/`, because
`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.

## What landed

* `aura_exec::census` — a hand-written WASM binary classifier: magic,
  version, section framing and order, imports, tables, memories,
  globals, and **every instruction of every function body**.
* `CENSUS` — the published table of nondeterminism sources with policy
  and enforcement layer.
* `ModuleLimits::restrictive()` — the restrictive configuration the rest
  of the stack must use.
* `Ban` (18 variants), `Verdict`, `CensusReport`.

**No engine is linked in X0.** The census is static: it makes claims
about classification, not execution. `wasmtime` is pinned in X1 with
fuel metering, memory ceilings and the zero-import sandbox.

## Census table (published policy)

| source | feature | policy | enforced by | why |
|---|---|---|---|---|
| `float_instruction` | f32/f64 arithmetic, comparison, conversion, reinterpret, const | **BAN** | X0 + X8 | NaN bit patterns are not fully specified |
| `host_import` | import section (any module, incl. WASI) | **BAN** | X0 + X1 | the only route to clocks, RNG or the OS |
| `start_section` | start function (implicit run on instantiate) | **BAN** | X0 | executes without a transaction or gas |
| `shared_memory` | threads proposal, shared memory flag 0x03 | **BAN** | X0 | concurrent writes make order observable |
| `atomics` | 0xFE atomic ops, memory.atomic.wait | **BAN** | X0 | exists only to coordinate threads; can block on the clock |
| `simd` | 0xFD v128 instructions | **BAN** | X0 | extra divergence surface, unused in Stage 1 |
| `bulk_memory` | 0xFC bulk ops, saturating truncation | **BAN** | X0 | trunc_sat hides a float conversion; bulk copies escape per-byte metering |
| `call_indirect` | typed indirect call through a table | **BAN** | X0 + X6 | call graph, gas and depth limits stop being statically checkable |
| `unbounded_memory` | memory without a declared maximum | **BAN** | X0 + X1 | memory.grow could eat the node |
| `memory_too_large` | memory maximum above MAX_MEMORY_PAGES | **BAN** | X0 | bounds must be visible before execution |
| `table_too_large` | table without a maximum, or above the cap | **BAN** | X0 | same argument as memory |
| `mutable_global` | mutable module-level global | **BAN** | X0 | state outside versioned storage |
| `unknown_opcode` | any opcode outside the whitelist | **BAN** | X0, fail-closed | unreviewed opcodes are rejected, not trusted |
| `module_too_large` | module above MAX_MODULE_BYTES | **BAN** | X0 | deploy cost must be bounded |
| `bad_binary` | magic, version, section framing and order | **BAN** | X0 | malformed input never reaches the engine |
| `integer_arithmetic` | i32/i64 arithmetic, compare, wrap/extend, load/store, locals, blocks, loops, branches, direct calls | **ALLOW** | X0 whitelist; X1 executes with fuel | fully specified by the WASM spec |

## Restrictive configuration

* module size <= **512 KiB** (`MAX_MODULE_BYTES`)
* linear memory must declare a maximum <= **4096 pages = 256 MiB**
  (`MAX_MEMORY_PAGES`)
* tables must declare a maximum <= **1024 entries**
* zero imports, no start function, no mutable globals, no floats,
  no SIMD/atomics/bulk memory, no indirect calls

## DoD gate — 21 programs, each banned or deterministic

| program | verdict |
|---|---|
| `atomic_wait` | `banned:atomics` |
| `block_loop_branch` | `allowed` |
| `bulk_memory_copy` | `banned:bulk_memory` |
| `call_indirect` | `banned:call_indirect` |
| `empty` | `allowed` |
| `f32_add` | `banned:float_instruction` |
| `f64_add` | `banned:float_instruction` |
| `f64_nan_const` | `banned:float_instruction` |
| `f64_reinterpret` | `banned:float_instruction` |
| `i32_arith` | `allowed` |
| `i64_memory_roundtrip` | `allowed` |
| `import_env_random` | `banned:host_import` |
| `import_wasi_clock` | `banned:host_import` |
| `locals_and_calls` | `allowed` |
| `memory_no_max` | `banned:unbounded_memory` |
| `memory_too_large` | `banned:memory_too_large` |
| `mutable_global` | `banned:mutable_global` |
| `shared_memory` | `banned:shared_memory` |
| `simd_v128` | `banned:simd` |
| `start_section` | `banned:start_section` |
| `unknown_opcode` | `banned:unknown_opcode` |

**5 allowed, 16 banned.** The
allowed set proves the whitelist is usable (integer arithmetic, memory
round-trip, locals, direct calls, blocks/loops/branches); the banned
set covers NaN/floats, a WASI clock, an env RNG, shared memory, atomics,
SIMD, bulk memory, indirect calls, the start section, mutable globals,
unbounded and oversized memory, and an opcode outside the whitelist.

## Negative controls

NC-1 imports, NC-2 the single float choke point, NC-3 start section,
NC-4 memory maximum, NC-5 shared memory, NC-6 fail-closed whitelist.
6/6 PASS; total **110** across C0–C10 + S0–S7 + X0.

## Honest scope

* X0 does not execute anything and does not link a VM: no fuel, no
  memory isolation, no engine-level determinism claim. That is X1's gate.
* The whitelist is a whitelist, not a full WASM validator: an opcode we
  did not review is rejected, so the risk of the scanner is false
  rejection, not false acceptance.
* Instruction-level verification with a full decoder, floats in any
  position and 100 mutants arrive in X8.
* No consensus, P2P, node, wallet or devnet exists.

Validators / blocks / transactions remain **0 / 0 / 0**.
