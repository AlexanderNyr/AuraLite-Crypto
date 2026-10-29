# D0 baseline — the libraries exist, the node does not (measured)

Generated (UTC): 2026-10-29T19:21:16+00:00. Every number below came out of `aurad report` in this
tree; `gates.txt` holds the raw command output, and
`crates/aura-node/tests/fixtures/d0_baseline.txt` pins the same survey so it
cannot drift silently.

## What this phase claims

> Twenty-eight phases of libraries exist (C0–C10, S0–S7, X0–X8). The process
> that composes them into a node
> does not exist, and `aurad` refuses to pretend otherwise.

That is the whole deliverable: a **measurement** of the gap D1–D8 has to close,
plus a binary that fails loudly instead of exiting 0. `CONTRIBUTING.md` §1
forbids stubs that silently return success, because a stub that exits 0 makes
every later script, CI job and demo lie.

## The binary's contract

| invocation | exit | what it does |
|---|---|---|
| `aurad` (no arguments) | 2 | prints the missing capabilities, one phase each, and refuses |
| `aurad --config <path>` / `--chain-id` / `--data-dir` / `--rpc-addr` / `--validator-key` | 2 | recognises the flag, names the capability and the phase that owes it, refuses |
| `aurad report [--workspace PATH]` | 0 | measures the tree and prints what exists |
| `aurad report --fixture` | 0 | the same measurement as `key=value` lines (the pin) |
| `aurad report --workspace <not a workspace>` | 3 | refuses: there is nothing to measure |
| `aurad <anything else>` | 64 | usage error, with the usage text |
| `aurad --help` / `--version` | 0 | documented commands, version names the phase |

`--version` prints `(D0 baseline — the node does not exist)`. A reader who pipes
`aurad --version` into a deployment script sees the truth.

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
| `aura-consensus` | B1 | implemented |
| `aura-p2p` | N1 | placeholder |
| `aura-mempool` | D3 | placeholder |
| `aura-rpc` | D4 | placeholder |

**10 implemented, 3 placeholders, 0 missing.**
"Placeholder" is not a claim, it is a measurement: the crate's sources declare
no public item at all, which is the C0 skeleton convention (an empty crate is
honest; a fake `Ok(())` is not). Four libraries are still in that state.

## The nine capabilities a node needs and this tree does not have

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

## What M0 still needs

* **A process.** Nothing here starts, serves or shuts down (D1).
* **A pipeline.** No mempool, no block production, no receipts (D2, D3).
* **An API.** No RPC of any kind (D4, D5).
* **Observability.** No metrics, logs or health endpoint (D6).
* **A network.** `aura-consensus` (B1) and `aura-p2p` (N1) are still empty
  crates, so a devnet of one validator is not yet possible (D7).

The M0 exit criterion is "genesis → 1000 tx → byte-identical state root on two
runs". The state and execution halves of that are already measured (S-phases
and X7); what D0 records is that the *node* half is entirely absent.

## What D0 deliberately does not do

* **It does not start a server.** A node that binds a port before D1 owns
  config, signals and shutdown would leak a socket on every failed start.
* **It does not fake a config file.** `--config` is refused with "D1", not
  parsed leniently: a config loader that accepts anything is worse than none.
* **It does not claim the libraries work end to end.** The survey counts files
  and public items; whether those libraries are correct is what the other 28
  phases' gates measure.

## Honest limits

* **The survey is a static count, not an audit.** A crate with one public item
  measures the same as a crate with two hundred and one.
* **`Cargo.toml` membership is trusted.** The survey looks for
  `crates/<dir>/src`; a crate outside the workspace would be measured all the
  same.
* **The baseline will go stale on purpose.** When B1 lands consensus, this gate
  fails until the plan and the fixture are updated in the same patch — that is
  the mechanism, not a defect.

## Negative controls

Six tampers, each reddening a *named* test: the stub exits 0, every layer is
classified `implemented`, `report` measures a non-workspace, an unknown command
falls through to exit 0, the refusal stops naming the capabilities, and a node
flag is no longer recognised. A green baseline runs first, so a red caused by a
broken tree can never be mistaken for a successful control.

6/6 PASS; total **165** across C0–C10 + S0–S7 + X0–X8 + D0.

Tests: **453** in the workspace, 38 of them in the D0 gate.
Validators / blocks / transactions remain **0 / 0 / 0**.
