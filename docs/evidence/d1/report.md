# D1 skeleton — aurad starts, traces, writes a PID file, shuts down (measured)

Generated (UTC): 2026-10-29T12:44:24+00:00. Every number below came out of `aurad` in this
tree; `gates.txt` holds the raw command output, and
`crates/aura-node/tests/fixtures/d1_skeleton.txt` pins the same survey so it
cannot drift silently.

## What this phase claims

> D1 lands the `aurad` process skeleton: configuration (TOML + env + CLI,
> `chain-id` mandatory), tracing, signal handling, PID file and graceful
> shutdown. The tx pipeline, mempool and RPC do not exist yet — those land
> in D2/D4.

## The binary's contract

| invocation | exit | what it does |
|---|---|---|
| `aurad` (no arguments, no --chain-id) | 2 | refuses: chain-id is mandatory, names the `config` capability (D1) |
| `aurad --chain-id <id> --data-dir <path>` | 0 | starts the skeleton, installs signal handlers, writes a PID file, waits for SIGTERM/SIGINT, shuts down gracefully |
| `aurad --help` / `--version` | 0 | documented commands, version names the phase |
| `aurad report [--workspace PATH]` | 0 | measures the tree and prints what exists |
| `aurad report --fixture` | 0 | the same measurement as `key=value` lines (the pin) |
| `aurad report --workspace <not a workspace>` | 3 | refuses: there is nothing to measure |
| `aurad <anything else>` | 64 | usage error, with the usage text |

## The survey (measured now)

| layer | owner | presence |
|---|---|---|
| `aura-types` | C1 | implemented |
| `aura-codec` | C2 | implemented |
| `aura-crypto` | C3 | implemented |
| `aura-tx` | C4 | implemented |
| `aura-block` | C5 | implemented |
| `aura-state` | S1 | implemented |
| `aura-storage` | S2 | implemented |
| `aura-genesis` | C8 | implemented |
| `aura-exec` | X1 | implemented |
| `aura-consensus` | B1 | placeholder |
| `aura-p2p` | N1 | placeholder |
| `aura-mempool` | D3 | placeholder |
| `aura-rpc` | D4 | placeholder |

**9 implemented, 4 placeholders, 0 missing.**

## The seven capabilities a node still needs after D1

| capability | phase | what it will do |
|---|---|---|
| `tx-pipeline` | D2 | rpc -> mempool -> exec -> block -> consensus -> storage -> event |
| `mempool` | D3 | nonce/balance/gas admission, `effective_tip` priority, eviction |
| `json-rpc` | D4 | `submit_tx`, `get_balance`, `get_tx`, `get_block`, `call`, `estimate_gas`, `subscribe` |
| `grpc-rest` | D5 | protobuf schema, gateway, OpenAPI, versioning |
| `metrics` | D6 | Prometheus, `/health`, `/metrics`, OpenTelemetry traces |
| `devnet` | D7 | `make devnet`: 4 validators, genesis, faucet key, logs |
| `cli` | D8 | `aura` node ops, query, tx, keys, snapshot, upgrade |

The `what` column lives in `MISSING_CAPABILITIES` and is printed by the binary;
the gate asserts that the refusal text contains every capability, its phase and
its description, so the message cannot rot into a generic "not implemented".

## What M0 still needs after D1

* **A pipeline.** No mempool, no block production, no receipts (D2, D3).
* **An API.** No RPC of any kind (D4, D5).
* **Observability.** No metrics, logs or health endpoint (D6).
* **A devnet.** `make devnet` with 4 validators is not yet possible (D7).
* **A CLI.** `aura` node ops, query, tx, keys, snapshot, upgrade (D8).
* **Consensus and network.** `aura-consensus` (B1) and `aura-p2p` (N1)
  are still empty crates, so a multi-validator devnet is not yet possible.

## What D1 deliberately does not do

* **It does not serve RPC.** JSON-RPC, gRPC and REST land in D4/D5.
* **It does not produce blocks.** The tx pipeline lands in D2.
* **It does not fake signal handling.** At D1 the OS default applies to
  SIGTERM/SIGINT; a dedicated signal crate lands when the node has real
  state to flush.

## Honest limits

* **The survey is a static count, not an audit.** A crate with one public item
  measures the same as a crate with two hundred and one.
* **`Cargo.toml` membership is trusted.** The survey looks for
  `crates/<dir>/src`; a crate outside the workspace would be measured all the
  same.
* **The baseline will go stale on purpose.** When D2 lands the tx pipeline,
  this gate fails until the plan and the fixture are updated in the same patch.

## Negative controls

Six tampers, each reddening a *named* test: the stub exits 0 without chain-id,
a TOML config with unknown fields is accepted, `report` measures a non-workspace,
an unknown command falls through to exit 0, the refusal stops naming the D1
phase, and `config::load` accepts a missing chain-id. A green baseline runs
first, so a red caused by a broken tree can never be mistaken for a
successful control.

6/6 PASS; total **171** across C0–C10 + S0–S7 + X0–X8 + D0 + D1.

Tests: **0** in the workspace, 0 of them in the D1 gate.
Validators / blocks / transactions remain **0 / 0 / 0**.
