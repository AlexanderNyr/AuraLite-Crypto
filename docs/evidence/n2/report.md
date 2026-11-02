# N2 — Gossipsub topics, message signatures, dedup, and peer scoring evidence receipt

Generated (UTC): 2026-10-31T11:31:27+00:00.

## Delivered behavior

`aura-p2p` implements the **N2** gossipsub layer (`aura_p2p::gossipsub`) and
updates `aura_p2p::baseline` to report 2 implemented network components
(`transport`, `gossipsub`) and 6 remaining placeholders (`discovery` N3 ..
`chaos-harness` N8):

- **Canonical topics (`D-N2`)**: `GossipTopic` with the 5 protocol topics (`tx`,
  `proposal`, `vote`, `evidence`, `snapshot`), wire discriminant encoding/decoding,
  and per-node / per-peer topic subscription filtering.
- **Domain-separated Ed25519 message signatures (`D-N3`)**: `GossipMessage` signs
  and verifies `(topic, chain_id, sender_peer_id, seq_no, BLAKE3(payload))` under
  `AURA_P2P_GOSSIP_SIG_V1\0`, verifying `sender_peer_id == BLAKE3(sender_public_key)`
  in constant time and enforcing payload bounds (`1..=MAX_GOSSIP_PAYLOAD_BYTES`).
- **Deduplication (`SeenCache`)**: bounded FIFO + `BTreeSet<MessageId>` cache over
  deterministic `MessageId = BLAKE3(AURA_P2P_GOSSIP_ID_V1\0 || ...)` that drops
  already-seen messages without re-forwarding.
- **Peer scoring & flood/invalid-message penalties (`D-N2`)**: `PeerScoreParams`,
  `PeerScoreState`, and `GossipEngine` track per-peer score and sliding-window
  message rate (`rate_window_ms`, `max_messages_per_window`), penalizing floods
  and invalid messages and escalating peers at or below `ban_threshold` into
  `BanList`.
- **1000-tx 10-node mesh broadcast gate (`crates/aura-p2p/tests/n2_gossipsub.rs`)**:
  1000 signed transactions broadcast across a 10-node mesh (ring + chords) in
  `< 1 s`, with all 10 nodes receiving all 1000 transactions and deduplicating
  redundant mesh links; flooding and signature-forging peers lose score and are
  banned.

## Gate results

- `make test-n2`: 24 `aura-p2p` library unit tests and 2 `n2_gossipsub`
  integration tests (1000-tx 10-node mesh `< 1 s` + flood/forgery ban) passed.
- `make nc-n2`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact gossipsub::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **243/243** cumulative negative controls through N2; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean N1-baseline (`2cf0321a83d6b308950a196b476ab086bf50c42c` + `0040-N0` +
  `0041-N1`) apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
