# NODE_PLAN — `aurad`, mempool, RPC, metrics, devnet

**Status:** 📋 PLAN (D0–D8)  
**Stage:** Stage 1: D0–D8 (all phases active).  
**Deliverable:** `patches/0050-D0-...` … `patches/0058-D8-...`
**Dependencies:** CORE, STATE, EXEC, CONSENSUS, NET (all lower layers).

## Problem

Separate crates ≠ a runnable node. We need one binary entry point, a
deterministic tx pipeline, an API for clients, observability and a **devnet in
one command** — otherwise nobody (including the author) can verify the network.

## Decisions

- **D-D1.** `aurad` is a thin composition of crates; all logic lives in libraries.
- **D-D2.** Config: TOML + env + CLI, validated at startup, `chain-id` mandatory.
- **D-D3.** RPC never bypasses validation and never holds a state lock for the duration of a network call.
- **D-D4.** Metrics/tracing from the first devnet: without measurements there is no PERF_PLAN.


**C8 note (2026-10-26).** `CORE_PLAN` C8 shipped the `aura genesis`
subcommand (`hash` / `verify` / `show`) on the `aura` binary in
`aura-genesis`. D8 still owes the remaining CLI surface (node ops, query,
tx, keys, snapshot, upgrade) and its 30 vectors.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **D0** | Baseline | no binary; report "libraries exist, node does not" | `cargo run -p aurad` → meaningful error/stub, NC recorded |
| ✅ **D1** | `aurad` skeleton | config, tracing, graceful shutdown, signals, PID/health | start/stop without state loss; NC: SIGTERM without shutdown → red |
| ✅ **D2** | Tx pipeline | validated native transfers → deterministic execution → C5 block → B3 finality → S2 storage → commit event | 1000 txs end-to-end, receipts match balances/fees; no event before durable 3-chain commit |
| ✅ **D3** | Mempool | bounded fee-priority nonce lanes, replacement, eviction, reorg reinsertion; harden D2 nonce/balance/gas admission | 10 000 txs: no double spend, no stuck nonces; NC: nonce check off → red |
| ✅ **D4** | JSON-RPC 2.0 | HTTP + WebSocket API over validated D2/D3 state; durable commit subscriptions | contract tests + 50 wire vectors + 6 mutation controls; H3 RPC-parser fuzz shard (60 s, 0 crashes) |
| ✅ **D5** | gRPC + REST | protobuf schema, versioned gateway, OpenAPI, dedicated configurable gRPC listener | v1 → v1.1 additive compatibility; unknown REST request field → error; unsupported operations fail explicitly |
| ✅ **D6** | Observability | Prometheus, `/health`, `/metrics`, OpenTelemetry traces, structured logs | dashboard: height, finality lag, mempool size, peer count, gas/s |
| **D7** | Devnet | `make devnet` / `docker compose`: 4 validators, genesis, faucet key, logs | M1: 100 blocks, identical head, RPC returns a balance; NC: one validator down → network alive |
| **D8** | CLI `aura` | node ops, query, tx, keys, genesis, snapshot, upgrade | 30 CLI vectors; `aura --help` fully documented |

### D7 readiness note

D7 is not ready to mark complete: the four-validator M1 acceptance depends on
real N1/N2 transport and gossip, plus the B5 consensus connection. The current
`aura-p2p` layer is still a placeholder. See the staged dependency and test
plan in [`D7_READINESS.md`](D7_READINESS.md); four unrelated local daemons do
not constitute a devnet.

### D0 receipt (2026-10-29)

- Added the `aurad` baseline/report command: it measures the layer inventory and refuses to claim a working node when capabilities are absent.
- Evidence: `docs/evidence/d0/`; the refusal behavior and the measured fixture are pinned. D0 is a baseline, not a running node.

### D1 receipt (2026-10-29)

- Added configuration loading (TOML, environment and CLI), mandatory chain ID, startup tracing, PID/health bookkeeping and graceful shutdown for the skeleton process.
- The node still has no transaction pipeline, mempool, RPC, P2P transport or devnet; those remain later phases.
- Evidence: `docs/evidence/d1/`; the config/lifecycle tests and negative-control log are recorded there.

### D2 receipt (2026-10-29)

- Deliverable: `patches/0035-D2-transaction-pipeline.patch`.
- Added `aura_node::pipeline`: signed transaction admission, deterministic native-transfer execution, C5 block construction, BLS/QC voting through the B3 finality engine, S2 WAL-backed account/receipt commits, and post-commit events.
- D2 transfer payload is fixed at `recipient[20] || amount_be[16]`. The deterministic schedule is 21,000 gas at base fee 1; base fee is burned and tips go to the local proposer. Nonce, fee-cap, gas-floor, balance, chain and signature failures reject submission.
- The integration gate processes 1,000 transfers, waits for the 3-chain commit, verifies every receipt and balance, checks the storage root against the C5 header, and proves events are withheld until persistence. Six negative controls cover replay, nonce, balance, gas, fee cap and durable receipt enforcement.
- The D2 consensus composition is explicitly one-validator/local. Multi-validator networking and the full mempool policy remain later phases. Contract transaction kinds are rejected rather than acknowledged without execution.
- Recovery boundary: committed S2 state is durable, but D2 does not yet persist a replayable finality checkpoint. Reopening a store with transaction receipts fails closed with `RecoveryCheckpointRequired`; it never resets consensus over existing state.
- Evidence: `docs/evidence/d2/`; the 1,000-transaction integration test, six negative controls and final workspace gates are recorded there.

### D3 receipt (2026-10-30)

- Deliverable: `patches/0036-D3-fee-priority-mempool.patch`.
- Added `aura-mempool`: 16,384-entry default capacity, sender/nonce lanes, tip-priority selection with deterministic ties, 10% minimum replacement bump (ceil, minimum one unit), strictly-higher-tip eviction, duplicate rejection and a reorg reinsertion hook.
- Integrated pool admission with the D2 transfer executor. Chain/signature/expiry, nonce, balance, gas-floor and fee-cap checks fail closed; eligible nonce lanes are selected by tip and blocks remain canonically ordered by `(signer, nonce)`.
- The D3 acceptance test submits 10,000 signed sequential transfers and checks unique durable receipts, 10,000 nonce increments, exact balances and gas, state-root agreement and no event before the delayed three-chain commit.
- Reorg reinsertion is an explicit mempool boundary: callers must revalidate transactions against the new chain/account view. D3 does not add a multi-validator reorg engine or persist a finality checkpoint.
- Six unit-test-based negative controls cover nonce, priority, replacement, eviction, reorg reinsertion and gas-floor enforcement. Evidence: `docs/evidence/d3/`.

### D4 receipt (2026-10-30)

- Deliverable: `patches/0037-D4-json-rpc.patch`.
- Added JSON-RPC 2.0 methods for transaction submission, balances, transaction status, committed blocks, gas estimation, explicit unsupported contract calls, and WebSocket commit subscriptions. HTTP bodies and WS messages are capped at 1 MiB; connection count, batch size, and incomplete-request time are bounded.
- Pipeline-backed responses are owned before network writes. Commit subscriptions unregister on drop and are pruned without requiring another block commit. Native transfers are the only executable transaction kind; contract calls and unsupported execution return errors.
- Contract coverage: `make test-d4`; all 50 golden wire vectors pass; six unit-test-based mutation controls pass. The added `fuzz_rpc` target exercises both HTTP and WebSocket JSON parsers against a state-free backend. The recorded 60-second libFuzzer shard completed 733,119 executions without a crash; this is an RPC parser shard, not the full H3 campaign.
- Evidence: `docs/evidence/d4/`. Full H3 remains separate: a 24-hour campaign and the planned p2p-frame, WASM-verifier and wallet-keystore targets are not claimed by D4.

### D5 receipt (2026-10-30)

- Deliverable: `patches/0038-D5-grpc-rest.patch`.
- Added the `aura-grpc-rest` crate: protobuf `AuraNode` service, vendored-protoc build, gRPC listener, bounded REST gateway, and OpenAPI YAML/JSON endpoints. The REST gateway and gRPC service call the existing D4 service; neither bypasses D2/D3 validation, admission, finality or storage.
- REST v1.1 is additive to v1 (`apiVersion` metadata, unchanged `result`). Unknown JSON request fields and unsupported REST versions are rejected. New protobuf response metadata uses an additive field number, and a v1 field-layout decode test pins compatibility.
- Daemon config supports `grpc_addr` with CLI/environment/TOML precedence. gRPC shutdown cancels active serving cleanly. Unsupported contract calls remain explicit errors; no unsupported transaction or execution path is acknowledged as successful.
- `make test-d5`: 7 library tests, 1 live REST + gRPC test, 4 daemon CLI tests. `make nc-d5`: 6/6 exact unit-test mutations red, restoration green. `make test`: all workspace gates passed. `make nc-all`: 219/219 cumulative controls. Clean D4-baseline application and revalidation are recorded in `docs/evidence/d5/clean-apply.txt`.
- Evidence: `docs/evidence/d5/`; protocol contract: `docs/spec/grpc-rest.md`; OpenAPI: `docs/api/openapi-d5.yaml` and `.json`.

### D6 receipt (2026-10-30)

- Deliverable: `patches/0039-D6-observability.patch`.
- Added JSON structured logs, optional OTLP/gRPC span export with shutdown flush, Prometheus exposition, `/health` and `/metrics` on the existing shared HTTP listener, plus pipeline height/finality/mempool and committed gas/transaction instrumentation.
- Added an importable Grafana dashboard for committed height, finality lag, mempool size, peer count and committed gas/s. P2P remains unimplemented: peer count and availability are explicitly zero.
- `make test-d6`: 4 observability unit tests, JSON-log unit test, OTLP in-memory exporter unit test and 2 live HTTP contract tests passed. `make nc-d6`: 6/6 exact library-unit-test mutations reddened their named tests; restored library suite passed.
- Clean D5-baseline application and complete gates are recorded in `docs/evidence/d6/clean-apply.txt` and `gates.txt`.

## Risks

- "RPC-only" validation (bypassing consensus) — forbidden by D-D3.
- Lock leaks/deadlocks between exec and storage — detected by the D2 soak test.
- Different config versions across validators — schema + D1 validation.

## Non-goals

Explorer UI (ECOSYSTEM), end-user wallet (WALLET), mainnet deployment (OPS) —
D7 delivers only a local/test devnet.

## Closure criteria

`make devnet` starts 4 validators; `aura tx transfer` changes a balance;
`docs/evidence/d7/` contains logs from all four nodes with the same
`state_root`.
