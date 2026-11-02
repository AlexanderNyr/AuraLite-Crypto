# N0 — P2P network baseline evidence receipt

Generated (UTC): 2026-10-31T09:25:06+00:00.

## Delivered behavior

`aura-p2p` implements the N0 baseline survey (`aura_p2p::baseline`) and CLI
(`aura-p2p`). All eight `NET_PLAN` components (`transport` N1, `gossipsub` N2,
`discovery` N3, `block-sync` N4, `state-sync` N5, `dos-defence` N6,
`light-client` N7, `chaos-harness` N8) measure as `placeholder` (0 implemented,
8 placeholders, 0 missing). Invoking `aura-p2p` with no arguments exits `2`
(`EXIT_STUB`) and lists every missing capability with the phase that owes it.

Two-node RPC isolation is measured both in `aura_p2p::baseline` and in the live
two-node HTTP/JSON-RPC gate (`crates/aura-node/tests/n0_two_nodes_no_p2p.rs`):
when two independent `aurad` HTTP/JSON-RPC servers (`node-a` and `node-b`) run
with the same genesis state and a signed transfer is submitted and committed on
Node A, Node A serves the transaction and updated recipient balance over RPC,
while Node B returns `null` for `get_tx`, keeps recipient balance `0`, and stays
at height `0`. Both nodes report `peerCount = 0` and `p2p = "not_implemented"`.

N0 does not implement peer transport, gossipsub, discovery, sync, or a
multi-validator devnet.

## Gate results

- `make test-n0`: 10 `aura-p2p` library unit tests and 1 live two-node HTTP
  JSON-RPC isolation test passed.
- `make nc-n0`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact baseline::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **231/231** cumulative negative controls through N0; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean D6-baseline (`2cf0321a83d6b308950a196b476ab086bf50c42c`) apply and
  revalidation: [`clean-apply.txt`](clean-apply.txt).
