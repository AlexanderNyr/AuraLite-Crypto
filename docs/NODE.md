# The node — `aurad`

> Status: **D0 ✅, D1 ✅, D2 ✅, D3 ✅ (bounded fee-priority mempool), D4 ✅ (JSON-RPC 2.0), D5 ✅ (versioned REST + protobuf gRPC), B3 ✅ (single-process BFT finality primitives)**.
> The local pipeline admits and finalizes native transfers; D3 adds nonce-lane priority, replacement, eviction and reorg reinsertion. D4 adds bounded HTTP JSON-RPC and WebSocket commit subscriptions; D5 exposes that same backend through versioned REST and gRPC.
> Numbers: [`evidence/d3/report.md`](evidence/d3/report.md),
> [`evidence/d1/report.md`](evidence/d1/report.md),
> [`evidence/d0/report.md`](evidence/d0/report.md),
> [`evidence/b1/report.md`](evidence/b1/report.md).

## Why a baseline phase

`NODE_PLAN` opens with a measured baseline instead of a feature. D0's report
and fixture remain a historical snapshot of the tree before a process existed;
they are regenerated from the live survey as later phases add public APIs.
The current `aurad` is no longer that D0 stub: it loads configuration, starts
the local transfer pipeline and serves the D4 JSON-RPC and D5 REST/gRPC APIs. Missing future capabilities
are still named explicitly by `aurad report` rather than silently implied.

## D1: the node skeleton

D1 established configuration, tracing, PID and shutdown ownership. The current
`aurad --chain-id <id>` loads CLI/environment/TOML configuration (CLI wins),
initializes tracing, opens the local S2-backed pipeline, starts the HTTP/WS
listener and an on-demand block producer, and removes its PID file after
SIGTERM or SIGINT. D4 replaced the initial D1 signal placeholder with safe
`signal-hook` handlers. Without `--chain-id`, startup refuses with exit 2.

## D2: native-transfer pipeline

`aura_node::pipeline` composes the D1 node with canonical C4 transactions,
C5 blocks, B3 one-validator finality, S2 WAL-backed storage, receipts and
commit events. The integration gate submits 1,000 signed transfers, checks
nonce/balance/fee accounting and roots, and proves that the three-chain rule
holds back storage/events until finalization. D2 only executes native transfers;
other transaction kinds are rejected. D4 exposes this pipeline through JSON-RPC;
later network phases supply multi-validator transport. The pipeline fails closed on restart when
committed receipts exist but no replayable finality checkpoint is available.

See [`spec/node-pipeline.md`](spec/node-pipeline.md) and
[`evidence/d2/report.md`](evidence/d2/report.md).

## D3: fee-priority mempool

`aura-mempool` now provides a bounded, deterministic pool with **16,384** entries.
Admission validates chain/signature/expiry and D2's transfer executor checks nonce,
balance, gas and fee before insertion. Selection ranks eligible sender-lane heads by
tip while only exposing contiguous nonces; C5 block encoding remains canonical by
`(sender, nonce)`. Same-nonce replacements require at least a 10% tip bump (rounded
up, minimum one unit), and a full pool evicts its deterministic lowest-tip entry only
for a strictly higher tip. Reverted-branch transactions can be reinserted after the
caller revalidates them against the new account view.

The D3 acceptance test submits **10,000** signed transfers, checks nonce and balance
conservation, unique durable receipts, gas and three-chain event ordering. The pool's
reorg hook is a reinsertion boundary, not a claim that D3 adds a multi-validator
consensus reorg engine.

See [`evidence/d3/report.md`](evidence/d3/report.md).

## D4: JSON-RPC 2.0 over HTTP and WebSocket

`aura-rpc` implements a strict JSON-RPC 2.0 envelope parser, bounded HTTP
transport (`POST /rpc`, also `/`), RFC 6455 WebSocket at `/ws`, batches,
notifications and durable-commit subscriptions. `aura-node::rpc` adapts the
API to the live pipeline; it releases the pipeline lock before serializing or
writing responses. The supported methods are `submit_tx`, `get_balance`,
`get_tx`, `get_block`, `estimate_gas`, `call` and WebSocket `subscribe`.

Only native transfers are executable at this stage: `call` and non-transfer
`estimate_gas` return explicit `-32004` errors. `get_block` retains a bounded
process-local window; persistent block history/checkpoint recovery is not
claimed. The daemon's default validator key is a development-only seed, and
its default genesis has no funded accounts. D4 provides the JSON-RPC surface,
not a P2P or multi-validator network.

Contract: [`spec/json-rpc.md`](spec/json-rpc.md). Evidence:
[`evidence/d4/report.md`](evidence/d4/report.md).

## D5: versioned REST gateway and protobuf gRPC

`aura-grpc-rest` serves REST versions `/api/v1` and `/api/v1.1` on the bounded
D4 HTTP listener and protobuf gRPC on a separate configurable listener. Both
adapters use the same D4 service/pipeline and do not bypass validation,
mempool admission, finality or storage. v1.1 adds `apiVersion` to REST responses
without changing the `result` payload; the protobuf response version field is
additive, so v1 decoders preserve existing fields. Closed REST request schemas
reject unknown fields and unsupported versions explicitly. Contract calls and
unsupported execution remain explicit errors; only native transfers execute.

Listener defaults are `127.0.0.1:8545` for REST/JSON-RPC and
`127.0.0.1:9090` for gRPC. `--grpc-addr`, `AURA_GRPC_ADDR` and TOML `grpc_addr`
configure the gRPC bind address. OpenAPI is available in YAML and JSON and is
also served at `/openapi.yaml` and `/openapi.json`.

Contract: [`spec/grpc-rest.md`](spec/grpc-rest.md) and
[`api/openapi-d5.yaml`](api/openapi-d5.yaml). Evidence:
[`evidence/d5/report.md`](evidence/d5/report.md).

## The survey

`aurad report` walks `crates/<layer>/src`, counts files, lines and public item
declarations, and classifies each layer:

| presence | meaning |
|---|---|
| `implemented` | the crate declares at least one public item |
| `placeholder` | sources exist but declare no public item — the C0 skeleton convention |
| `missing` | no sources at all |

"Placeholder" is measured, not declared, and the D0/D1 fixtures are refreshed by
`tools/gen_d0_vectors.py` and `tools/gen_d1_vectors.py` from the binary's own
`report --fixture` output. Landing a library implementation without updating the
measurement and plan in the same patch makes the gate fail rather than silently
changing the baseline.

After D6 the survey measures **13 implemented, 1 placeholder, 0 missing**.
Only `aura-p2p` (N1) remains a placeholder; `aura-mempool`, `aura-rpc` and
`aura-grpc-rest` expose the D3, D4 and D5 APIs, with D6 observability composed
into the shared node HTTP listener.

## Layers and owners

| layer | owner | responsibility |
|---|---|---|
| `aura-types` | C1 | addresses, hashes, amounts, chain id |
| `aura-codec` | C2 | canonical binary encoding |
| `aura-crypto` | C3 | BLAKE3 hashing, Ed25519 signatures |
| `aura-tx` | C4 | transaction envelope and signing domain |
| `aura-block` | C5 | block header and body commitment |
| `aura-state` | S1 | JMT-like versioned tree, overlay, history |
| `aura-storage` | S2 | RocksDB + WAL, snapshots, pruning |
| `aura-genesis` | C8 | genesis document, hash, `aura genesis` CLI |
| `aura-exec` | X1 | deterministic WASM execution, ABI v1–v3, verifier |
| `aura-consensus` | B3 | proposer, BLS/QC and three-chain finality; validator-set/VRF-leader management remain open |
| `aura-p2p` | N1 | gossip, peer management, sync — **placeholder** |
| `aura-mempool` | D3 | bounded fee-priority pool, replacement, eviction, reorg reinsertion |
| `aura-rpc` | D4 | bounded JSON-RPC 2.0 over HTTP/WebSocket |
| `aura-grpc-rest` | D5 | versioned REST gateway, protobuf gRPC, OpenAPI |

## What each later phase owes

| capability | phase | what it will do |
|---|---|---|
| `devnet` | D7 | `make devnet`: 4 validators, genesis, faucet key, logs |
| `cli` | D8 | `aura` node ops, query, tx, keys, snapshot, upgrade |

These two entries are `aura_node::baseline::MISSING_CAPABILITIES` after D6
(`config`/lifecycle D1, transfer pipeline D2, mempool D3, JSON-RPC D4,
versioned REST/gRPC D5, and Prometheus/health/structured logs/optional OTLP D6
are implemented). The gate asserts that the refusal text contains every
remaining capability, its phase and its description, so the message cannot rot
into a generic "not implemented".

## D6 observability

The shared HTTP listener exposes `/health` and Prometheus `/metrics`; the latter
includes committed height, finality lag, mempool size/capacity, peer count,
committed gas/transactions and rolling gas per second. JSON structured logs and
optional OTLP gRPC span export are configured through `AURA_LOG`/`RUST_LOG` and
`OTEL_EXPORTER_OTLP_ENDPOINT`. The Grafana dashboard and setup notes are in
[`observability/`](observability/). P2P remains unimplemented and peer metrics
stay explicitly zero; see the D6 docs for exact boundaries.

## Dependencies that must land first

* **N (network)** — `aura-p2p` is empty, so multiple validators cannot talk to each
  other yet. D7's multi-validator devnet depends on it.
* **B (consensus)** — B3 supplies local BFT finality; validator-set management and
  VRF leader selection remain open in B4/B5.

D2's local one-validator path is an integration gate, not a substitute for the
multi-validator network required by the later devnet milestone.

## D4 boundaries

* No P2P transport or multi-validator network; the periodic producer is the
  local one-validator development composition.
* No funded genesis allocation or faucet in the default daemon config; that is
  part of D7/devnet.
* No contract VM dispatch from `call`, and no non-transfer gas estimation;
  unsupported operations return explicit RPC errors.
* No persisted block index or finality checkpoint recovery; `get_block` serves
  the recent in-memory window and existing D2 recovery fails closed.

## D5 boundaries

* The gRPC listener is local API transport only; D5 does not add peer networking,
  multiple validators or contract execution.
* REST request objects reject unknown fields; protobuf wire-level unknown fields
  retain standard forward-compatible decoding behavior. New v1.1 response
  metadata does not remove or renumber v1 fields.
* Unsupported contract calls and transaction kinds remain explicit errors. The
  gateway does not claim success for operations D2/D4 do not implement.

## Reproducing the baseline

```bash
make gen-d1-vectors     # refresh the pinned survey from the binary's own output
make test-d1            # the gate: config, tracing, PID, health, refusal
make nc-d1              # 6 negative controls, each reddening a named test
make evidence-d1        # docs/evidence/d1/{gates.txt,report.md,inventory.txt,SHA256SUMS}
make test-d0            # the D0 gate is still green
make test-d3            # mempool + 10,000-transfer acceptance
make nc-d3              # six focused unit-test mutation controls
make evidence-d3        # docs/evidence/d3/{gates.txt,report.md,inventory.txt,SHA256SUMS}
make test-d4            # JSON-RPC vectors, transport and node-backed adapter tests
make nc-d4              # six exact-unit-test mutation controls
make evidence-d4        # aggregate completed gate receipts and NC inventory
make test-d5            # REST/gRPC contract + live transport tests
make nc-d5              # six exact-unit-test mutation controls
make evidence-d5        # aggregate completed D5 gate receipts and NC inventory
aurad report            # current measured capabilities
aurad --chain-id test   # starts HTTP/WS JSON-RPC, REST and gRPC; SIGTERM shuts down
```