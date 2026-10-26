# EXEC_PLAN — deterministic WASM machine and contracts

**Status:** 📋 PLAN (X0–X9)  
**Stage:** Stage 1: X0–X8. ⏸ Stage 2: X9 (EVM compatibility).  
**Deliverable:** `patches/0020-X0-...` … `patches/0029-X9-...`
**Dependencies:** CORE C2–C5, STATE S1–S3.

## Problem

Smart contracts are the main source of nondeterminism: NaN in `f64`, WASI
clocks, iteration order, "random" host functions, different wasmtime behaviour
on different OSes. One differing NaN and the network diverges mid-block.
The plan first **measures the nondeterminism surface**, then closes it.

## Decisions

- **D-X1.** `wasmtime` with fuel metering, no WASI, no threads, no non-canonical `f64` NaNs (or float instructions banned entirely in the verifier).
- **D-X2.** Host ABI is versioned; every call costs gas; limits on memory/stack/call depth/log size.
- **D-X3.** A contract compiles to `wasm32-unknown-unknown` from the Rust SDK; the block stores `code_hash`, not "source".
- **D-X4.** A bytecode verifier is a mandatory gate before deploy.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| **X0** | Determinism census | table of "what wasmtime can expose nondeterministically" + restrictive config, report | 20 programs (NaN, clocks, imports) — either banned or deterministic |
| **X1** | Sandbox | engine, memory/stack/fuel limits, zero host imports | NC: contract with `wasi_snapshot_preview1` → `Err` at deploy |
| **X2** | Gas + ABI v1 | `storage_read/write`, `balance`, `transfer`, `call`, `emit`, `hash`, `block_ctx`; gas table | OutOfGas → revert (S3); NC: remove gas charging → infinite loop passes, test red |
| **X3** | Lifecycle | deploy/instantiate/upgrade/code_hash/version, `init`, storage migration | upgrade preserves balance, changes logic; NC: upgrade without version check → red |
| **X4** | `aura-sdk` | Rust macros `#[entrypoint]`, typed storage, events, ABI-JSON generation | 3 example contracts build to WASM, size and hash reproducible |
| **X5** | Reference standards | ARC-20, ARC-721, minimal multisig | vector table: transfer/mint/burn/approve/ownerOf + reentrancy test |
| **X6** | Call isolation | call depth, reentrancy guard, value transfer, atomic revert of the tree | "evil" contract cannot drain others' funds; NC: guard off → test red |
| **X7** | Determinism gate | 8 nodes × 1000 contract txs × 3 platforms → byte-identical `state_root` | decisive M0 gate; any divergence = ⛔ NO-GO for mainnet |
| **X8** | Static verifier | ban floats/forbidden imports/size/stack, error report | 100 mutants: all invalid rejected, valid accepted |
| ⏸ **X9** | Optional EVM-lane (stretch) | `revm` behind a feature flag, shared state/gas, address mapping | compatibility with 10 known ERC-20 contracts; **not the critical path**, can be moved after M4 |

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
