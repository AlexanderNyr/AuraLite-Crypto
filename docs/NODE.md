# The node — `aurad`

> Status: **D0 ✅ (baseline), D1 ✅ (skeleton), B1 ✅ (PoA proposer + block signature)**.
> The libraries exist, the node starts, and consensus has its first real code.
> Numbers: [`evidence/d1/report.md`](evidence/d1/report.md),
> [`evidence/d0/report.md`](evidence/d0/report.md),
> [`evidence/b1/report.md`](evidence/b1/report.md).

## Why a baseline phase

`NODE_PLAN` opens with a measurement instead of a feature, for the reason
`CONTRIBUTING.md` §1 gives: **a stub that exits 0 makes every later script, CI job and
demo lie.** Twenty-eight phases of libraries (C0–C10, S0–S7, X0–X8) are in the tree; none of
them is a process. D0 therefore shipped the opposite of a fake node:

* `aurad` with no arguments prints the missing capabilities, one phase each, and exits **2**;
* a node flag (`--config`, `--chain-id`, `--data-dir`, `--rpc-addr`, `--validator-key`)
  is recognised, named and refused with the phase that owes it;
* anything else is a usage error (**64**);
* `aurad report` is the one command that succeeds, because it produces a measurement —
  and it exits **3** when asked to measure a directory that is not a workspace.

## D1: the node skeleton

D1 lands the process itself. `aurad` with `--chain-id <id>` now:

1. loads configuration from CLI flags, environment variables (`AURA_*`) and `aurad.toml`
   (CLI overrides env overrides TOML);
2. initialises structured tracing to stderr;
3. writes a PID file into the data directory;
4. installs signal handlers (OS default at D1 — no `unsafe` FFI);
5. waits for SIGTERM/SIGINT, then shuts down gracefully, removing the PID file.

Without `--chain-id`, `aurad` still refuses with exit 2 and names the `config` capability (D1).
The tx pipeline, mempool and RPC do not exist yet — those land in D2/D4.

## The survey

`aurad report` walks `crates/<layer>/src`, counts files, lines and public item
declarations, and classifies each layer:

| presence | meaning |
|---|---|
| `implemented` | the crate declares at least one public item |
| `placeholder` | sources exist but declare no public item — the C0 skeleton convention |
| `missing` | no sources at all |

"Placeholder" is measured, not declared, and the result is pinned in
`crates/aura-node/tests/fixtures/d1_skeleton.txt` (written by
`tools/gen_d1_vectors.py` from the binary's own `report --fixture` output).
When B1 lands consensus, that gate fails until the plan and the fixture change in the
same patch. That is the mechanism working, not a defect.

After D1: **11 implemented, 2 placeholders, 0 missing** — only `aura-consensus` (B1)
and `aura-p2p` (N1) remain placeholders. After B1, `aura-consensus` measures **2
implemented, 4 placeholders, 0 missing** — proposer and block-signature are
implemented; qc, finality, validator-set, vrf-leader remain placeholders.

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
| `aura-consensus` | B1 | PoA proposer + block signature landed; QC/finality/validator-set/VRF-leader missing |
| `aura-p2p` | N1 | gossip, peer management, sync — **placeholder** |
| `aura-mempool` | D3 | nonce/balance/gas admission, prioritisation — **placeholder** |
| `aura-rpc` | D4 | JSON-RPC 2.0, gRPC, REST — **placeholder** |

## What each later phase owes

| capability | phase | what it will do |
|---|---|---|
| `tx-pipeline` | D2 | rpc → mempool → exec → block → consensus → storage → event |
| `mempool` | D3 | nonce/balance/gas admission, `effective_tip` priority, eviction |
| `json-rpc` | D4 | `submit_tx`, `get_balance`, `get_tx`, `get_block`, `call`, `estimate_gas`, `subscribe` |
| `grpc-rest` | D5 | protobuf schema, gateway, OpenAPI, versioning |
| `metrics` | D6 | Prometheus, `/health`, `/metrics`, OpenTelemetry traces |
| `devnet` | D7 | `make devnet`: 4 validators, genesis, faucet key, logs |
| `cli` | D8 | `aura` node ops, query, tx, keys, snapshot, upgrade |

These seven entries are `aura_node::baseline::MISSING_CAPABILITIES` after D1
(`config` and `lifecycle` landed with D1). The gate asserts that the refusal text
contains every one of them, with its phase and its description, so the message cannot
rot into a generic "not implemented".

## Dependencies that must land first

* **B (consensus)** — `aura-consensus` is empty, so there is no block production and
  no finality to serve. D2/D7 depend on it.
* **N (network)** — `aura-p2p` is empty, so "4 validators" (D7) cannot talk to each
  other yet.

D1 records the gap honestly rather than pretending the ordering away: the node phase
cannot finish before B and N, and `NODE_PLAN`'s own dependency line says so.

## What D1 does not do, on purpose

* It does not bind a port. A node that opens a socket before D1 owns config, signals
  and shutdown leaks a socket on every failed start.
* It does not serve RPC. JSON-RPC, gRPC and REST land in D4/D5.
* It does not produce blocks. The tx pipeline lands in D2.
* It does not claim the libraries work end to end — the survey counts files and public
  items, and correctness belongs to the gates of the phases that shipped them.

## Reproducing the baseline

```bash
make gen-d1-vectors     # refresh the pinned survey from the binary's own output
make test-d1            # the gate: config, tracing, PID, health, refusal
make nc-d1              # 6 negative controls, each reddening a named test
make evidence-d1        # docs/evidence/d1/{gates.txt,report.md,inventory.txt,SHA256SUMS}
make test-d0            # the D0 gate is still green
aurad report            # what this tree contains, right now
aurad --chain-id test   # starts the skeleton; SIGTERM shuts it down cleanly
```