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
| **D0** | Baseline | no binary; report "libraries exist, node does not" | `cargo run -p aurad` → meaningful error/stub, NC recorded |
| **D1** | `aurad` skeleton | config, tracing, graceful shutdown, signals, PID/health | start/stop without state loss; NC: SIGTERM without shutdown → red |
| **D2** | Tx pipeline | `rpc → mempool → exec → block → consensus → storage → event` | 1000 txs travel end-to-end, receipts match expectations |
| **D3** | Mempool | nonce/balance/gas checks, `effective_tip` priority, eviction, reorg | 10 000 txs: no double spend, no stuck nonces; NC: nonce check off → red |
| **D4** | JSON-RPC 2.0 | `submit_tx`, `get_balance`, `get_tx`, `get_block`, `call`, `estimate_gas`, `subscribe` (WS) | spec contract test + 50 vectors; fuzz the RPC parser (H3) |
| **D5** | gRPC + REST | protobuf schema, gateway, OpenAPI, versioning | backward compatibility v1 → v1.1; unknown field → error |
| **D6** | Observability | Prometheus, `/health`, `/metrics`, OpenTelemetry traces, structured logs | dashboard: height, finality lag, mempool size, peer count, gas/s |
| **D7** | Devnet | `make devnet` / `docker compose`: 4 validators, genesis, faucet key, logs | M1: 100 blocks, identical head, RPC returns a balance; NC: one validator down → network alive |
| **D8** | CLI `aura` | node ops, query, tx, keys, genesis, snapshot, upgrade | 30 CLI vectors; `aura --help` fully documented |

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
