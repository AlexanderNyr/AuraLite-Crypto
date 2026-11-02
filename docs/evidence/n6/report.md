# N6 — DoS defence: size/rate limits, signature rate limit, and equivocation detection evidence receipt

Generated (UTC): 2026-11-01T12:54:10+00:00.

## Delivered behavior

`aura-p2p` implements the **N6** DoS-defence layer (`aura_p2p::dos_defence`)
and updates `aura_p2p::baseline` to report 6 implemented network components
(`transport`, `gossipsub`, `discovery`, `block-sync`, `state-sync`,
`dos-defence`) and 2 remaining placeholders (`light-client` N7 and
`chaos-harness` N8):

- **Pre-crypto ban check (`D-N3`, `DosDefenceEngine::check_pre_crypto`)**:
  rejects banned peers in `O(1)` before spending any CPU on Ed25519 signature
  verification or payload deserialization.
- **Frame size & per-peer message rate limits (`DosDefenceLimits`)**: caps
  ingress frame size (`max_frame_bytes = 256 KiB`) and per-peer message rate
  (`max_messages_per_window = 128 msg/s`), escalating floods and repeated
  oversized frames to `BanList`.
- **Per-peer signature verification budget (`max_sig_verifications_per_window`)**:
  bounds Ed25519 verification CPU per peer (`64 sig/s` default) and bans peers
  exceeding the invalid-signature strike threshold.
- **Equivocation detection (`EquivocationEvidence`)**: detects conflicting
  consensus proposals or votes from the same peer at the same `(height, round)`,
  produces self-contained cryptographically verifiable `EquivocationEvidence`,
  and immediately bans the equivocator.

## Gate results

- `make test-n6`: 48 `aura-p2p` library unit tests and 1 `n6_dos_defence`
  integration test (1 malicious peer sending 10,000 msg/s while 4 validators
  keep finalizing heights 1..=5) passed.
- `make nc-n6`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact dos_defence::tests::<test>`); restored
  library tests passed.
- `make nc-all`: **291/291** cumulative negative controls through N6; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0049` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
