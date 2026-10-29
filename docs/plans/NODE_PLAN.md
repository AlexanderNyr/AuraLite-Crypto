# NODE_PLAN — `aurad`, mempool, RPC, metrics, devnet

**Status:** 🚧 IN PROGRESS (D0 ✅ 2026-10-29; D1 next)  
**Stage:** Stage 1: D0–D8 (all phases active).  
**Deliverable:** `patches/0029-D0-...` … `patches/0037-D8-...`
**Dependencies:** CORE, STATE, EXEC, CONSENSUS, NET (all lower layers). D0 depends on
CORE + STATE + EXEC only; D2 and D7 additionally block on B (consensus) and N (network).

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
| ✅ **D0** | Baseline | no binary; report "libraries exist, node does not" | `aurad` refuses with exit 2 and names every missing capability; survey measured and pinned |
| **D1** | `aurad` skeleton | config, tracing, graceful shutdown, signals, PID/health | start/stop without state loss; NC: SIGTERM without shutdown → red |
| **D2** | Tx pipeline | `rpc → mempool → exec → block → consensus → storage → event` | 1000 txs travel end-to-end, receipts match expectations |
| **D3** | Mempool | nonce/balance/gas checks, `effective_tip` priority, eviction, reorg | 10 000 txs: no double spend, no stuck nonces; NC: nonce check off → red |
| **D4** | JSON-RPC 2.0 | `submit_tx`, `get_balance`, `get_tx`, `get_block`, `call`, `estimate_gas`, `subscribe` (WS) | spec contract test + 50 vectors; fuzz the RPC parser (H3) |
| **D5** | gRPC + REST | protobuf schema, gateway, OpenAPI, versioning | backward compatibility v1 → v1.1; unknown field → error |
| **D6** | Observability | Prometheus, `/health`, `/metrics`, OpenTelemetry traces, structured logs | dashboard: height, finality lag, mempool size, peer count, gas/s |
| **D7** | Devnet | `make devnet` / `docker compose`: 4 validators, genesis, faucet key, logs | M1: 100 blocks, identical head, RPC returns a balance; NC: one validator down → network alive |
| **D8** | CLI `aura` | node ops, query, tx, keys, genesis, snapshot, upgrade | 30 CLI vectors; `aura --help` fully documented |

### D0 receipt (2026-10-29)

- Deliverable: `patches/0029-D0-node-baseline.patch`. The phase ships the **opposite
  of a fake node**: `aura_node::baseline` (survey + refusal) and the `aurad` binary,
  no new crate and no new dependency (`aura-node` already existed as an empty
  placeholder since C0).
- **The binary refuses.** `aurad` with no arguments exits **2** and prints the nine
  missing capabilities with the phase that owes each one; `--config`, `--chain-id`,
  `--data-dir`, `--rpc-addr` and `--validator-key` are recognised and refused the same
  way; an unknown command exits **64** with the usage text; `report` exits **3** when
  asked to measure a directory that has no `crates/`. `--version` prints
  `(D0 baseline — the node does not exist)`, so even a version probe cannot be
  mistaken for a running node.
- **The survey is measured, not typed.** `aurad report` walks `crates/<layer>/src` and
  counts files, lines and public item declarations for the thirteen library layers,
  classifying each as `implemented`, `placeholder` or `missing`. Measured now: **9
  implemented, 4 placeholders, 0 missing** — `aura-consensus` (B1), `aura-p2p` (N1),
  `aura-mempool` (D3) and `aura-rpc` (D4) declare no public item at all, which is the
  C0 skeleton convention rather than a claim.
- The result is pinned in `crates/aura-node/tests/fixtures/d0_baseline.txt`, written by
  `tools/gen_d0_vectors.py` from the binary's own `report --fixture` output. The gate
  compares live survey to pin, so landing consensus code (B1) fails the build until
  the plan and the fixture change in the same patch.
- DoD gate: 13 integration tests + 2 unit tests (refusal text covers every capability,
  phase and description; exit codes through the real binary; the survey is
  deterministic and equals the fixture; placeholders are exactly the four unlanded
  layers).
- Evidence: `docs/evidence/d0/` (gates, report, inventory), 6/6 NC, **165** NC total
  across C0–C10 + S0–S7 + X0–X8 + D0. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: a static count is not an audit — a crate with one public item measures
  like a crate with two hundred; "implemented" says nothing about correctness, which is
  what the other phases' gates measure. Nothing here starts a process, binds a port or
  parses a config file, on purpose.
- What M0 still needs: a process (D1), a pipeline (D2, D3), an API (D4, D5),
  observability (D6) and a devnet (D7) — and, before most of that, **B (consensus) and
  N (network)**, whose crates are still empty.

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
