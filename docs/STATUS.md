# Implementation status — AuraLite-Crypto

> Updated **in the same patch** as the phase's code (rule D9).
> Snapshot date: **2026-10-29**, tree: "D0 landed — node baseline, **NODE_PLAN**
> started; EXEC_PLAN complete". See [`CLOSEOUT_AUDIT.md`](CLOSEOUT_AUDIT.md) (CORE),
> `docs/evidence/s7/report.md` (S7), `docs/evidence/x0/report.md` (X0),
> `docs/evidence/x1/report.md` (X1), `docs/evidence/x2/report.md` (X2),
> `docs/evidence/x3/report.md` (X3), `docs/evidence/x4/report.md` (X4),
> `docs/evidence/x5/report.md` (X5), `docs/evidence/x6/report.md` (X6) and
> `docs/evidence/x7/report.md` (X7), `docs/evidence/x8/report.md` (X8) and
> `docs/evidence/d0/report.md` (D0).

## Locked decisions (2026-10-25)

| Parameter | Value |
|---|---|
| Project / network | `AuraLite-Crypto` / `AuraLite-Crypto Network` |
| Ticker | `ALCRY` (18 decimals, `aalcry`) — see [`TICKER_AUDIT.md`](TICKER_AUDIT.md) |
| chain-id Stage 1 | `auralite-test-1` (mainnet `auralite-1` reserved) |
| Consensus | BFT-PoS, 2f+1 QC, VRF |
| VM | WASM (`wasmtime`) + our ABI; EVM optional in X9 |
| Current stage | **Stage 1 = M0–M2** (core → devnet → public testnet) |
| ⏸ Stage 2 | mainnet, white paper, external audit, DeFi, bridge — separate decision |
| Docs language | English (owner request, 2026-10-25) |

**Phase ledger:** C0–C10 ✅ (`patches/0001-C0-...` … `patches/0011-C10-...`)
— **CORE_PLAN is complete**; S0 ✅ (`patches/0012-S0-state-baseline.patch`),
S1 ✅ (`patches/0013-S1-merkle-tree.patch`), S2 ✅
(`patches/0014-S2-rocksdb-wal.patch`) and S3 ✅
(`patches/0015-S3-execution-overlay.patch`) and S4 ✅
(`patches/0016-S4-snapshots.patch`), S5 ✅
(`patches/0017-S5-state-sync.patch`), S6 ✅
(`patches/0018-S6-pruning-archive.patch`) and S7 ✅
(`patches/0019-S7-recovery-backup.patch`) done — **STATE_PLAN is COMPLETE** —
X0 ✅ (`patches/0020-X0-determinism-census.patch`) opened **EXEC_PLAN** and
X1 ✅ (`patches/0021-X1-sandbox.patch`) gives it a real engine and
X2 ✅ (`patches/0022-X2-gas-abi.patch`) gives it state access, value transfer,
events and a gas model, X3 ✅ (`patches/0023-X3-lifecycle.patch`) makes a
contract a **record in state**: deploy, call by address, upgrade, migrate, and
X4 ✅ (`patches/0024-X4-sdk.patch`) makes a contract **Rust**: `aura-sdk` +
`aura-sdk-macros`, three reference contracts, pinned artifacts and ABI-JSON, and
X5 ✅ (`patches/0025-X5-standards.patch`) makes the chain **useful**: ABI v2 in the
module `env2` (calldata, return data, `caller`, `self_address`) plus three
standards — ARC-20, ARC-721, ARC-M1 multisig — written in Rust, pinned by hash and
deployed by the gate, X6 ✅ (`patches/0026-X6-call-isolation.patch`) makes
contracts **compose**: ABI v3 in the module `env3` (`call`, `returndata_len`,
`returndata_read`, `depth`) with two host-enforced rules — no call to an address
already on the stack (`-14`) and no stack deeper than 8 frames (`-15`) — plus three
contracts that attack each other (a vault that pays before it debits, an attacker
that re-enters it, a nine-copy chain), pinned by hash and BLAKE3, and X7 ✅
(`patches/0027-X7-determinism-gate.patch`) turns the whole machine into a
**measured** claim: 1 004 ordered transactions replayed by eight separate processes
and three build configurations produce one `state_root`, one gas bill and one
events chain — the decisive M0 gate, and X8 ✅
(`patches/0028-X8-static-verifier.patch`) closes the subset statically: 22 stable
refusal codes, 100 golden mutants from an independent mutator, every one refused
with the code its damage deserves and every shipped artifact accepted.
**Stage 1 (X0–X8) is complete.** D0 ✅ (`patches/0029-D0-node-baseline.patch`) opens
**NODE_PLAN** with a measurement rather than a feature: the libraries exist, the node
does not, and `aurad` refuses to pretend otherwise (exit 2, every missing capability
named with the phase that owes it).
Next in dependency order: D1 (`aurad` skeleton: config, tracing, shutdown), but
D2/D7 additionally need **B (consensus)** and **N (network)** — both crates are still
empty placeholders. ⏸ X9 (EVM lane), W/E/T/H/P/O remain out of scope for now.

## Maturity level

| Level | Definition | Now |
|---|---|---|
| L0 | Docs/plan, no code | ✅ done at C0 |
| L1 | Core libraries + test rig, no network | ✅ **we are here** (CORE C0–C10 + STATE S0–S7 + EXEC X0–X4; B/N/D phases still open) |
| L2 | Devnet: 4 validators in one process/host | ❌ |
| L3 | Public testnet with external validators | ❌ (Stage 1 target) |
| L4 | Mainnet candidate: audit, freeze, white paper | ⏸ Stage 2 |
| L5 | Mainnet + live application ecosystem | ⏸ Stage 2 |

L3 is the ceiling of **Stage 1**. L4–L5 belong to ⏸ Stage 2 (decision Q5).

## Working now

- `make test` is green: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (**193 tests**),
  the docs drift guard, the dependency gate, the vector inventory, the
  test-rig wiring check and the plan↔tree close-out audit.
  Evidence receipts for every phase live in `docs/evidence/`; the newest are
  `docs/evidence/s2/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `native.txt`,
  `docs/evidence/s3/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s4/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s5/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s6/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/s7/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/x0/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/x1/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/x2/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/x3/gates.txt`, `nc.log`, `report.md`, `inventory.txt`,
  `docs/evidence/x4/gates.txt`, `nc.log`, `report.md`, `inventory.txt`.
  ⚠️ AUDIT: the §3 crate graph in `docs/ARCHITECTURE.md` had a cycle
  (`aura-state <- aura-storage` while `aura-storage <- aura-state`); S3
  corrected it to match the real `Cargo.toml` edges.
- **D0 (current).** `NODE_PLAN` opens with the measurement the other plans started
  with: **the libraries exist, the node does not.** `aura_node::baseline` surveys
  `crates/<layer>/src` — files, lines, public item declarations — and classifies each
  of thirteen layers as `implemented`, `placeholder` or `missing`: measured now **9
  implemented, 4 placeholders, 0 missing**, the placeholders being `aura-consensus`
  (B1), `aura-p2p` (N1), `aura-mempool` (D3) and `aura-rpc` (D4), which declare no
  public item at all. The `aurad` binary ships **the opposite of a fake node**: no
  arguments → exit **2** with all nine missing capabilities and their phases;
  `--config`/`--chain-id`/`--data-dir`/`--rpc-addr`/`--validator-key` → recognised and
  refused the same way; an unknown command → exit **64**; `report` measures the tree
  and exits **3** when there is no workspace; `--version` says
  `(D0 baseline — the node does not exist)`. The survey is pinned in
  `crates/aura-node/tests/fixtures/d0_baseline.txt` from the binary's own
  `report --fixture` output, so landing consensus code fails the build until the plan
  and the fixture move together. 15 tests in the D0 gate, 374 in the workspace, 4 857
  vectors, 165 NC. Report: `docs/evidence/d0/report.md`; doc: [`NODE.md`](NODE.md).
  ⚠️ AUDIT: **a static count is not an audit** — "implemented" says nothing about
  correctness (that is what the other 28 phases' gates measure), and the survey trusts
  `crates/<dir>/src` to exist. Nothing here starts a process, binds a port or parses a
  config file, on purpose: a node that opens a socket before D1 owns shutdown leaks a
  socket on every failed start. **M0 still needs B (consensus), N (network) and the
  rest of D.**
- **X8.** The subset is now **enforced, not just documented**.
  `aura_exec::verify` is a pure function over bytes — no engine, no execution — that
  answers with a `Report`: verdict, size, functions, instructions, peak stack depth,
  peak body, and one `Problem { code, offset, detail }` per defect, with the offset
  pointing into the **module**. **22 stable codes.** It checks section order and
  uniqueness, `function`/`code` counts against each other, every value type and
  local, the import whitelist (three ABI surfaces, functions only, plus a restrictive
  configuration that refuses all imports), globals, the memory maximum against the
  census ceiling, the export shape, and — per body — the instruction and local
  budgets, the operand-stack depth with **callee signatures resolved out of the type
  section**, label depth, and the `local`/`global`/`call`/`call_indirect` index
  spaces. **Measured on the 100 golden mutants** produced by an independent Python
  mutator (`tools/gen_x8_mutants.py`, 15 kinds of damage, 100 exactly): every one
  refused with the expected code, zero mismatches; all six shipped artifacts
  accepted — `kv` 33 functions / 1 905 instructions, `store` 29 / 1 921, `events`
  25 / 1 871, `vault` 53 / 3 692, `evil` 61 / 4 235, `recurse` 51 / 3 331, peak
  stack depth 7, peak body 630 instructions. Those numbers are pinned in the gate.
  37 tests in `x8_verify`, 359 in the workspace, 4 826 vectors, 159 NC. Report:
  `docs/evidence/x8/report.md`.
  ⚠️ AUDIT: **this is a depth check, not a type check** — the operand stack is
  tracked as a length, so a body that type-confuses two integer widths can pass X8
  and still be refused by the engine at instantiation. That is the intended division
  of labour: the engine remains the authority, X8 guarantees the engine is never
  asked to instantiate something outside the subset. Two real defects were found
  building it: `call` originally read no callee signature (so `store.wasm` was
  refused and every stack number was fiction), and body offsets were relative to the
  body slice rather than the module. **No phase calls the verifier yet**: wiring it
  into the deploy path is D-phase work, so the claim is "a verifier exists and is
  measured", not "deploys are verified".
- **X7.** Determinism is **measured, not asserted**. `aura_exec::detcheck`
  generates a 1 004-step workload from one constant (`SplitMix64` seeded by
  `X7_SEED`; no clock, no entropy, no iteration order in generation) and replays it
  over the X3 lifecycle: 890 commits, 60 refusals, 54 failures (19 out of gas, 35
  no-code, 0 traps), gas **46 375 290**, state root `efca4222d37347ff…`. The gate
  spawns **eight separate processes** — different hasher seeds, allocator histories
  and address layouts — and all eight reproduce the root, the gas, the events chain
  and the rolling digest; `make x7-matrix` reproduces it again under `debug`,
  `release` and `release-cgu16`. The gate executes the **shipped** `kv`, `store`,
  `events` and `vault` bytecode and re-verifies their BLAKE3 on every run, so X7
  also re-pins X4 and X6. 11 tests in `x7_determinism`, 322 in the workspace, 4 692
  vectors, 153 NC. Report: `docs/evidence/x7/report.md`; doc:
  [`DETERMINISM.md`](DETERMINISM.md).
  ⚠️ AUDIT: the gate found four defects — the deployer constant was not a funded
  account (one byte, `InsuffientBalance` on the fourth deploy), an argument was
  passed to `run()` of contracts that take none so a third of the workload never
  ran (890 commits instead of 559), the vault's `withdraw` was sent an address in
  front of its amount so every withdrawal refused `-21`, and the first negative
  control was green until it removed the *root* from the digest fold rather than the
  index. **Honest limits: three build configurations on one machine, not three
  operating systems — the OS matrix is a CI job and is unproven until it runs;
  concurrency is untested because it does not exist; the root includes the version,
  so the gate fixes the input instead of permuting blocks; different CPUs are not
  exercised (X8's job).**
- **X6.** A contract can **call another contract**. ABI v3 is module
  `env3`, version 3, four functions: `call` (100 + 3/byte), `returndata_len` (20),
  `returndata_read` (20 + 3/byte), `depth` (20); the value is a `u128` passed as two
  `i64` halves so calldata cannot alias with it. Two rules live in the **host**, not
  the SDK: a frame may not call an address already on the stack (`-14 reentrancy`)
  and a stack may not exceed `MAX_CALL_DEPTH = 8` (`-15 call_depth`), plus
  `-16 no_code` and `-17 callee_failed`. A child frame gets its own store, its own
  budget drawn from the parent's remainder, and its own base version; on success
  its diff is merged and its events appended, on failure its writes, events and
  value are gone and the caller reads the status. **Three contracts, 30 459 bytes:
  `vault` 9 759, `evil` 11 607, `recurse` 9 093**, built twice into independent
  target directories and pinned by `sha256` and by the node's own BLAKE3. Measured
  gas: vault deploy 107 369 / evil deploy 117 100 / recurse deploy 100 132 / fund
  41 188 / attack 87 697 / honest withdrawal after the attack 35 116 / nine-frame
  descent 28 414. The attacker gets **one** attempt, its ledger is debited once, the
  vault ends **empty rather than short**, and the honest depositor is paid her
  9 800 in full; the depth limit is measured on a chain of nine *distinct* copies
  because a contract may not call itself. 15 tests in `x6_calls`; 311 tests,
  4 692 vectors, 147 NC, 239 external crates — X6 adds no dependency at all.
  Report: `docs/evidence/x6/report.md`; docs: [`ABI_V3.md`](ABI_V3.md),
  [`CALLS.md`](CALLS.md).
  ⚠️ AUDIT: the gates caught four defects — the `wasmi` backend's `check_imports`
  kept its own copy of the allowed modules and refused `env3` (the census was
  right), a child's status was dropped so a callee's `-14` reached its caller as
  `0`, the vault's first draft paid *through* `env3.call` (which cannot pay an
  account with no code at all — money and control are two different calls), and the
  first depth fixture called itself, which the guard correctly refuses. **Honest
  limits: no callbacks into a live frame (Stage 1 restriction), gas is charged after
  the callee returns so multi-frame fee accounting is the T-phase's, and a single
  call that both sends value and runs code is not tested and not claimed.**
- **X5.** The chain is **useful**: three standards in Rust — ARC-20, ARC-721,
  ARC-M1 multisig — on **ABI v2**, module `env2`: `calldata_len`, `calldata_read`,
  `return_write`, `caller`, `self_address`, `abi_version`; 20 gas base, 3 gas/byte
  for the copying calls, calldata and return data both 1 KiB and both refused
  **before** the engine runs. A standard's refusal is a status code **and zero
  writes** — every gate asserts the empty change set and unchanged tree entries, not
  "the balance looks right". Measured gas: ARC-20 deploy 140 694 / mint 56 410 /
  transfer 55 951 / balance_of 39 221; ARC-721 deploy 145 604 / mint 63 946 /
  transfer 71 514; ARC-M1 deploy 162 108 / policy 56 066 / propose 72 930 / approve
  76 281. `dispatch!` gives every standard the same `-20` unknown selectors and
  `-21` trailing bytes. 43 tests over the shipped artifacts in five files. Report:
  `docs/evidence/x5/report.md`; docs: [`ABI_V2.md`](ABI_V2.md),
  [`STANDARDS.md`](STANDARDS.md).
  ⚠️ AUDIT: the gates caught an ARC-721 `approve` that trapped on **every** call (a
  40-byte event buffer holding a 16-byte amount), a missing `self_address` that made
  a "can I cover this?" check impossible, `RUSTFLAGS` used for a wasm-only linker
  flag (it breaks any host build script that recompiles), and a harness that read a
  stale tree between calls. **Honest limits: the multisig policy is immutable,
  allowance and vote keys use 80 bits of BLAKE3 because a slot is 32 bytes, and
  reproducibility is path-scoped.**
- **X2.** `aura_exec::abi` + `aura_exec::contract` make a contract
  able to *do* something: **ABI v1** is module `env`, version 1, exactly seven
  functions (`storage_read`, `storage_write`, `balance`, `transfer`, `emit`,
  `hash`, `block_ctx`), one dispatcher, a closed status-code set (`0` success,
  `-1`…`-8` errors, `-9` absent key, `-10` unknown block field, `-11` too many
  events) and published limits (key 32 B, value 4 KiB, topic 64 B, 64 KiB per
  buffer). **Gas is 1 fuel = 1 gas with one counter**: the instruction meter is
  reset to `gas_left` at every host-call boundary, so the X1 under-report is
  gone. A successful call commits a canonical `StateDiff`; a trap, an
  out-of-gas and a refusal leave the root byte-identical and the events empty,
  and a contract that catches `-8` and returns is still out of gas. The gas
  table is checked row by row against a second, hand-written copy in Python.
  **23 vectors: 13 returned, 5 out of gas, 3 trapped, 3 refused**; 7 X2 negative
  controls. 208 tests, 4,555 vectors, 123 NC, 239 external crates (X2 adds zero
  external crates — three *workspace* dependencies instead). Report:
  `docs/evidence/x2/report.md`.
  ⚠️ AUDIT: the gates caught three defects — the rollback was not load-bearing
  (the failure receipt used a hard-coded empty event vector, so deleting
  `HostState::rollback` left every test green), a fuel trap was reported as a
  plain trap although 1 fuel = 1 gas, and `slot_of` zero-pads keys so `b"a"`
  and `b"a\0"` collide (now documented and asserted). **Hosted execution is
  wasmi-only**: the `wasmtime` backend returns `EngineUnavailable` for it rather
  than pretending, so no cross-engine *gas* claim is made here.
- **X4.** A contract is written **in Rust**. `aura-sdk` is a `no_std` guest
  library: the seven ABI v1 functions as typed Rust, a **length-exact**
  fixed-layout codec, typed storage (`Slot<T>`, composed `Key`), events with the
  published limits checked before the call — and on the host it compiles against
  a mock backend that speaks the node's `extern "C"` contract, so the wrappers
  are tested rather than trusted. `aura-sdk-macros` turns a function into an
  export (`run`, `init`, `migrate`) and rejects a wrong arity or return type at
  compile time. Three reference contracts (`store`, `kv`, `events`) are built for
  `wasm32-unknown-unknown`, **committed, pinned by sha256 and deployed by the
  gate**: 5 187 / 5 290 / 5 033 bytes, byte-identical across two independent
  target directories; `examples/<name>/abi.json` describes each artifact. Measured
  gas: deploy 58 495, call 13 115 (`store`); deploy 62 353, call 8 506, upgrade
  62 261 (`kv`); deploy 51 330, call 12 940 (`events`). 253 tests, 4 624 vectors,
  135 NC, 239 external crates unchanged (17 local packages). Report:
  `docs/evidence/x4/report.md`.
  ⚠️ AUDIT: the gate found that the **X0 census rejected every Rust contract** —
  a mutable `__stack_pointer`, `memcpy` → `memory.copy`, and `call_indirect` from
  `core::fmt`. The hosted policy now allows exactly one mutable global, only the
  two deterministic byte ops, and `call_indirect` **only when the element table
  contains no import** (so a function pointer can never reach host code);
  `restrictive()` is untouched, so every X0 verdict stands. Also found: `rustc`
  memory has no declared maximum (the build now passes `--max-memory`), `wasmi`'s
  `Config` was stricter than the census, and the codec could silently truncate a
  wider value. **A contract cannot allocate, `call_indirect` is bounded but not
  verified (X8), and reproducibility is path-scoped** (`events` differs at a
  different checkout path because cargo folds the package path into
  `-C metadata`, and which contracts drift is not stable across experiments).
- **X3.** `aura_exec::lifecycle` turns bytecode into a **state record**:
  `CodeRecord` stored under `StateKey::code` with layout `magic(8) "ALCRYCD1"
  || format || abi || version || len || code`, strict decoding (bad magic, bad
  format, bad length, trailing bytes — four different errors), and
  `Account::code_hash = BLAKE3(code)` as the identity of the logic, so an
  account without code is `Hash::ZERO` and "is this a contract?" needs no side
  table. **Deploy is atomic with instantiation** — one session stores the code,
  funds the account and runs the optional `init`, so a trapping or out-of-gas
  `init` leaves no account, no code and no moved value. **Upgrade is optimistic
  and explicit** — `upgrade(expected_version)` refuses a stale version, rewrites
  only the code record and `code_hash`, and never touches balance or storage;
  migration is contract logic (`migrate(from, to)`), not a chain operation.
  Code is **priced, not capped** (`1 000 + 10 ×` bytes ⇒ ≤ ~100 KiB per
  transaction at the Stage 1 budget). The X0 census is enforced at deploy with
  `ModuleLimits::hosted()`, which permits exactly the seven ABI imports.
  **9 programs, 12 tests, measured gas: deploy 3 560, call 30, upgrade 4 527,
  call-after-upgrade 30, `run`-only deploy 2 860**; 6 X3 negative controls.
  221 tests, 4 595 vectors, 129 NC, 239 external crates and no new workspace
  dependency. Report: `docs/evidence/x3/report.md`.
  ⚠️ AUDIT: the gates caught four defects — the X0 import ban made the X2 ABI
  unloadable (fixed with `ModuleLimits::allow_abi_imports`, X0 verdicts intact),
  `entry_arity` could not go through `prepare` (it requires a `run` export),
  a deploy without `init` was reported as a trap, and `LifecycleError::code()`
  collapsed every refusal into `refused:censored` instead of naming the cause.
  **Still wasmi-only for hosted execution, and `deploy`/`upgrade` are explicit
  but not permissioned** (no `caller == owner` check — accounts and signatures
  are T-phase).
- **X1.** `aura_exec::sandbox` is the runtime half of the verifier:
  size → X0 census → compile → **zero imports** → instantiate with store limits
  → `run` → closed trap set. `SandboxLimits::restrictive()` publishes fuel
  1,000,000, memory 4096 pages, module 512 KiB, stack 512 KiB, trace 4096
  events, and `validate()` refuses anything looser than the census ceiling.
  The engine is behind a trait: **`wasmi` 2.0.0** by default (deterministic
  interpreter, `portable-dispatch`) and `wasmtime` 50.0.0 behind the
  `wasmtime-engine` feature. **19 programs: 6 executed, 5 trapped, 8 refused**;
  fuel pinned per program (`const42` = 30, `sum_to_n(1000)` = 13 246, `spin` =
  exactly the 1,000,000 budget); both engines agree on all 19 outcomes. 6 X1
  negative controls. 193 tests, 4,532 vectors, 239 external crates in the lock
  (95 + 144; the default build compiles 10 of them). Report:
  `docs/evidence/x1/report.md`.
  ⚠️ AUDIT: the gates caught three real defects — a missing `wasmi` dispatch
  feature that turned `loop { br 0 }` into a native stack overflow instead of
  an out-of-fuel trap, an under-specified trap classifier, and a
  component-model/`reference-types` conflict that made `wasmtime` reject every
  module. All three are fixed in the X1 patch.
- **X0.** `aura_exec::census` is the determinism census: a static,
  fail-closed WASM classifier (magic/version, section framing, imports,
  tables, memories, globals and **every instruction**) plus the published
  `CENSUS` table (16 rows: source, feature, policy, enforcement, why) and
  `ModuleLimits::restrictive()` — module ≤ 512 KiB, memory must declare a
  maximum ≤ 4096 pages, zero imports, no start section, no mutable globals, no
  floats, no SIMD/atomics/bulk memory, no indirect calls. **21 programs:
  5 allowed, 16 banned**, each assembled by an independent Python assembler.
  **No engine is linked**: X0 makes no runtime claim, `wasmtime` is pinned in
  X1. 72 fixture lines; 6 X0 negative controls. 184 tests, 4,415 vectors.
  Report: `docs/evidence/x0/report.md`.
- **S7.** `aura-storage::backup` gives recovery: a self-verifying
  bundle (`u32le(manifest_len) || manifest || WAL blob`, BLAKE3 manifest
  checksum + SHA-256 blob + per-record BLAKE3) and a restore that **replays**
  every record through the real commit path, so a lying manifest fails with
  `RootMismatch`. **50 `kill -9` + restore cycles** (17 inside the WAL/head
  crash window) -> **0 head, 0 restore, 0 continuation mismatches**, 338
  records replayed. 28 fixture lines; 6 S7 negative controls. 177 tests,
  4,349 vectors. Report: `docs/evidence/s7/report.md`.
  **STATE_PLAN is COMPLETE**: recover from zero to the current `state_root`,
  prove any balance, survive a sudden `kill -9`.
- **S6.** `aura-state::history` + `aura-storage::prune` give
  version retention. `HistoryTree` is an incremental compressed sparse
  Merkle tree with **S1-identical roots** (measured: S1 rebuild = 16.4 ms
  per commit at 1 024 leaves, ~27 min for 100 000 blocks; incremental =
  ~12 hashes per write, ~6.6 us). `RetentionStore` keeps a materialised
  base plus the last `keep_versions` deltas (prune mode) or everything
  (archive mode), and folds deltas into the base on `compact()`.
  **100 000 blocks:** archive 36,500,045 bytes serving every historical
  root and proof; prune (`keep = 128`) peaks at 113,326 bytes —
  **322.1x** smaller and under the analytic bound. A pruned version returns
  `PruneError::Pruned`; archive mode refuses to compact. 35 fixture lines;
  6 S6 negative controls. 169 tests, 4,326 vectors. Report:
  `docs/evidence/s6/report.md`. Build profile: `blake3`/`sha2` are
  `opt-level = 3` in dev builds (measured 5.785 us -> 0.178 us per hash).
- **S5.** `aura-state::sync` gives state sync: a fresh node
  downloads one package (3-head quorum + S4 snapshot + compact delta),
  verifies all SHA-256 checksums, applies the sorted delta and recomputes
  the S1 `state_root`. Package = 16,992 bytes vs full history 998,400
  bytes (58.8x smaller). Substituted diff chunk rejected; 3/3 heads must
  agree. 11 rejection vectors; 6 S5 negative controls. 154 tests, 4,309
  vectors. Report: `docs/evidence/s5/report.md`.
- **S4.** `aura-state::snapshot` gives deterministic snapshots:
  SHA-256 chunk digests, a checksummed manifest with descriptors, a BLAKE3
  `chunk_root` and an S1 `state_root`. Export/import hash chunks in
  parallel (1..8 workers, deterministic output). 1,000,000 keys reimport
  byte-identically; a substituted chunk is rejected. 11 rejection
  vectors; 6 S4 negative controls. Report: `docs/evidence/s4/report.md`.
  146 tests, 4,254 vectors. `sha2 = 0.10.9` became a DIRECT dep of
  `aura-state` (already in the C3 lock via `k256`; no new external crate).
- **S3.** `aura-state::overlay` gives `StateView` +
  `StateDiff` + rollback. A `Revert` or `OutOfGas` leaves `base_root`
  unchanged and the account balance byte-identical; 10 canonical diff
  rejection vectors; 6 S3 negative controls. Report:
  `docs/evidence/s3/report.md`. 138 tests, 4,205 vectors.
- **S2.** `aura-storage` (real API since it was an honest
  placeholder at C0) gives durable state. A `StateBackend` trait with two
  implementations: a pure-Rust WAL + `BTreeMap` backend by default
  (reproducible in any CI), and real RocksDB column families (`state`,
  `nonce`, `meta`, `wal_index`) with one atomic `sync = true` `WriteBatch`
  behind the optional `rocksdb` feature. A framed, BLAKE3-checksummed WAL
  record is fsynced BEFORE head publication; `kill -9` at 10 commit points
  and 10 WAL/head crash-window points all recover the same head and root.
  8 independent WAL rejection vectors; 6 S2 negative controls; 9 new tests.
  Report: `docs/evidence/s2/report.md`.
- **S1.** `aura-state::tree` is a deterministic versioned sparse binary
  Merkle tree over 256-bit `StateKey::digest` paths, with single-leaf
  compression, depth-dependent empty hashes and inclusion proofs bound to a
  block height. 10 000 operations matched an independent Python reference at
  12 versioned roots; 1 000 proofs verified. Report: `docs/evidence/s1/report.md`.
- **S0.** `aura-state::baseline` is a deliberately naive append-only write
  log with last-write-wins lookup, kept as a MEASURED "before" picture:
  `state_root()` → `Err(NoRoot)`, `persist()`/`load()` →
  `Err(NoPersistence)`, `report().deterministic == false`. Two nodes with
  the same writes in opposite order get different balances (300 vs 100
  aalcry in scenario 1). Report: `docs/evidence/s0/baseline-report.md`.
  **Defect intentional** — S1 replaced it. Do not build consensus on it.
- **CORE C0–C10.** Workspace, CI, gates, D0 identity, canonical types
  (`aura-types`), `aura-codec` canonical serialization, `aura-crypto`
  (BLAKE3/Ed25519/secp256k1), `aura-tx` format with replay protection,
  `aura-block` header/body + `tx_root`, compact Merkle proofs (C6), account
  model (C7), genesis CLI (C8), `aura-testkit` (C9) and the plan↔tree
  close-out audit (C10). Every ✅ row has a numbered patch, a `### Cx
  receipt`, a CHANGELOG entry, an NC driver/log and checksummed evidence.
- **Dependencies:** 95 external crates, all with exact `=x.y.z` pins,
  crates.io sources and sha256 checksums. The C3 baseline
  (`docs/spec/c3-dependency-allowlist.txt`) is **frozen** historical
  evidence; C8/C9 addenda live in `docs/spec/`, the S2 addendum in
  `docs/evidence/s2-dependency-addendum.txt` (S2 must not add files to
  `docs/spec/` because its index is checksummed C10 evidence). RocksDB is
  **optional**: default builds need no native library; the `rocksdb` feature
  needs `librocksdb-dev >= 9.9` + `libclang` (Ubuntu 24.04 = 8.9.1 is
  incompatible; CI uses a Debian trixie job). `cargo deny` (licences/
  advisories) is still owed to H1.
- 5 placeholder crates still compile but expose **no API** (an honest
  empty crate, not a fake `Ok(())`): `aura-consensus`, `aura-mempool`,
  `aura-node`, `aura-p2p`, `aura-rpc`. CI runs the same commands as
  `make test`.
- `aura-exec` is the only crate with an external dependency (the engine).
  `wasmi` is default and always compiled; `wasmtime` is behind the
  `wasmtime-engine` feature and gated by the `exec-wasmtime` CI job. Bumping
  either engine changes fuel metering, so it is a protocol decision with its
  own patch.

## Measured numbers (2026-10-29)

```
find crates -name Cargo.toml       -> 17 crates (+1 workspace root)
ls docs/plans/*_PLAN.md           -> 12 plans / 112 phase rows
cargo test --workspace            -> 374 passed; 0 failed
make test                         -> ALL GATES GREEN (incl. close-out audit)
make vector-inventory             -> 4857 vectors across 27 fixtures
make audit-closeout               -> every ✅ has a receipt
make nc-all                       -> 165 NC PASS across 29 phases
make check-deps                   -> 239 external crates allowlisted
revert / OutOfGas                -> root unchanged, balance unchanged
snapshot 1M keys                 -> byte-identical reimport
state sync 3 heads               -> new node matches their root
kill -9 commit points             -> 10 + 10 crash-window, all recover
backup chaos (S7)                 -> 50 kill -9 + restore, 0 mismatches
retention 100 000 blocks (S6)     -> prune 113,326 B vs archive 36,500,045 B (322.1x)
determinism census (X0)           -> 21 programs: 5 allowed, 16 banned
sandbox (X1)                      -> 19 programs: 6 executed, 5 trapped, 8 refused
sandbox fuel (X1)                 -> spin burns exactly 1,000,000; fuel pinned per program
cross-engine (X1)                 -> wasmi and wasmtime agree on 19/19 outcomes
ABI v1 (X2)                       -> 7 host functions, 23 vectors, gas table vs independent copy
lifecycle (X3)                    -> CodeRecord + code_hash, deploy/upgrade/migrate, 9 programs, 12 tests
SDK (X4)                          -> #[entrypoint]/#[constructor]/#[migrate], typed slots, codec, 3 contracts, pinned hashes
standards (X5)                    -> ABI v2 (env2), ARC-20/ARC-721/ARC-M1, 41 225 B, 43 tests over the artifacts
call isolation (X6)               -> ABI v3 (env3), depth 8, no re-entry, 30 459 B, 15 tests, attacker gets 1 attempt
determinism (X7)                  -> 1 004 steps, 8 processes, 3 build configs, one state_root, gas 46 375 290
static verifier (X8)              -> 100 mutants / 15 kinds, 0 mismatches, 6/6 artifacts accepted (peak stack 7, peak body 630)
node baseline (D0)                -> aurad exits 2 and names 9 missing capabilities; 9 layers implemented, 4 placeholders, 0 missing
gas (X2)                          -> 1 fuel = 1 gas; out-of-gas burns the whole budget, root unchanged
validators / blocks / tx          -> 0 / 0 / 0
```

Pre-C0 baseline and post-C0/C1/C2/C3/C4/C5/C6/C7/C8/C9/C10/S0/S1/S2
numbers with raw command output are in `docs/evidence/c0-baseline.txt`,
`docs/evidence/c1/gates.txt` … `docs/evidence/c10/gates.txt`,
`docs/evidence/s0/gates.txt`, `docs/evidence/s1/gates.txt` and
`docs/evidence/s2/gates.txt`.

## What is missing (and must not be lost)

- No fee market: gas is a cost model, not a price (T-phase). Floats are banned
  statically by X8, but **the verifier is not yet on the deploy path** — the D-phase
  node is what calls it — and a multi-frame transaction is charged after the fact
  rather than reserved before it.
- No **callbacks** into a live frame (X6 keeps them unrepresentable on purpose), so
  no receiver logic in the same transaction as the payment, and no flash-loan shape.
- Determinism is proven on **one machine in three build configurations**; the
  three-OS matrix is a declared CI job (`determinism`), not a measurement yet.
- No Miri/ASan/UBSan or 24 h libFuzzer campaign (H3), no BLS aggregation (B2), no PQ
  migration (H2), no `cargo deny` (H1). The decisive determinism gate (X7) and the
  static verifier (X8) are shipped, but **M0 still needs the D-phase node**, and the
  three-OS determinism matrix is still a CI job that has never run.
- No consensus (B1), no P2P (N1), no mempool (D3), no RPC (D4): the four layer
  crates are empty placeholders, measured by `aurad report` and pinned in D0's gate.
- No node **process**: `aurad` is a refusal plus a survey; nothing starts, serves or
  shuts down until D1.
- No wallet, explorer, contracts, token.
- No audit, no bug bounty, no legal opinion.
- No users and no validators.

Each item above becomes a "✅ with receipt" line only in the patch of its
phase. If a line appears without a receipt, that is a documentation defect and
is fixed by a separate NC (following the example of `MATURITY_AUDIT.md` in
AuraLite-OS).

## How to read plan statuses

| Label | Meaning |
|---|---|
| 📋 PLAN | written, not started |
| 🚧 IN PROGRESS | open phases remain, `make test` green at the last closed one |
| ✅ CLOSED | all phases closed, receipts, NCs, CHANGELOG line present |
| ⚠️ AUDIT | external check found a plan/tree mismatch |
| ⛔ NO-GO | measurement showed the approach does not work (like PERF P4 in the Emulator) |

## Target platforms (goal, not fact)

Linux x86_64 (primary), macOS aarch64 (development), Docker (validators),
WASM (browser light client), later AuraLite OS as a light client
(`ECOSYSTEM_PLAN` E9). None of this has been verified on any platform yet.
