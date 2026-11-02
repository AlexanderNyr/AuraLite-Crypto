# N8 — Chaos harness: latency/loss/partition/eclipse simulation and 100-node stand evidence receipt

Generated (UTC): 2026-11-01T13:59:55+00:00.

## Delivered behavior

`aura-p2p` implements the **N8** chaos-harness layer (`aura_p2p::chaos`) and
updates `aura_p2p::baseline` to report all 8 `NET_PLAN` components
(`transport`, `gossipsub`, `discovery`, `block-sync`, `state-sync`,
`dos-defence`, `light-client`, `chaos-harness`) as implemented (`8 implemented,
0 placeholders`), closing `NET_PLAN` (`N0`–`N8`):

- **100-node stand with latency, packet loss, partition, eclipse, and Byzantine faults (`ChaosStand`)**:
  simulates 100 P2P nodes with configurable base latency + deterministic jitter,
  packet loss up to 40%, temporary network partitions with post-heal
  header-first catch-up sync, Sybil eclipse attacks defeated by `2f+1` QC
  verification (`D-N3`, `D-N4`), and Byzantine equivocation detected and banned
  by `DosDefenceEngine`.
- **20 scenarios × 3 runs matrix (`canonical_20_scenarios`, `run_chaos_matrix`)**:
  executes 60 deterministic runs across 20 fault scenarios on the 100-node stand
  with zero finalized-head (`block_hash` / `state_root`) divergence
  (`divergent_runs == 0`).
- **`NET_PLAN` closure criteria verified (`S01_100nodes_20pct_loss_1_byzantine`)**:
  100 nodes with 20% packet loss and 1 Byzantine peer converge on the exact
  same finalized `state_root`, all 99 honest nodes ban the Byzantine equivocator,
  and an N7 `LightClientVerifier` verifies the finalized head + QC + DAS proof
  without a full node.

## Gate results

- `make test-n8`: 60 `aura-p2p` library unit tests and 1 `n8_chaos` integration
  test (20 scenarios × 3 runs on the 100-node stand + closure-criteria check)
  passed.
- `make nc-n8`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact chaos::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **303/303** cumulative negative controls through N8; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0051` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
