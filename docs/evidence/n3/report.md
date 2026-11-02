# N3 — Peer discovery: Kademlia DHT, bootstrap list, NAT/UPnP, and Circuit Relay v2 evidence receipt

Generated (UTC): 2026-11-01T07:49:38+00:00.

## Delivered behavior

`aura-p2p` implements the **N3** discovery layer (`aura_p2p::discovery`) and
updates `aura_p2p::baseline` to report 3 implemented network components
(`transport`, `gossipsub`, `discovery`) and 5 remaining placeholders
(`block-sync` N4 .. `chaos-harness` N8):

- **Kademlia DHT (`XorDistance`, `KademliaTable`, `KADEMLIA_K = 20`, `KADEMLIA_ALPHA = 3`)**:
  256-bit XOR metric, 256 k-buckets, closest-peer ordering (`closest_peers`),
  stale sequence replay rejection (`StalePeerRecord`), and `BanList` enforcement.
- **Cryptographically signed peer records (`D-N1`, `D-N3`)**: `SignedPeerRecord`
  signs and verifies `(DISCOVERY_RECORD_DOMAIN, chain_id, peer_id, public_key, seq, direct_addrs, nat_status)`
  with Ed25519 and checks `PeerId == BLAKE3(public_key)` in constant time.
- **Bootstrap list, NAT/UPnP, and Circuit Relay v2**: `BootstrapList`,
  `UpnpMapper` (`NatStatus::UpnpMapped`), and `RelayServer` / `RelayReservation`
  (`NatStatus::BehindSymmetricNat`) with TTL expiration and frame size bounds.
- **30-node behind-NAT discovery gate (`crates/aura-p2p/tests/n3_discovery.rs`)**:
  30 NATed nodes (5 cone-NAT UPnP-mapped relay-capable peers + 25 symmetric-NAT
  peers) seeded with at most 2 local neighbors and no central server converge in
  `<= 4` iterative Kademlia lookup rounds so every node discovers all 29 other
  verified peer records and exchanges traffic over relay and UPnP endpoints.

## Gate results

- `make test-n3`: 30 `aura-p2p` library unit tests and 2 `n3_discovery`
  integration tests passed.
- `make nc-n3`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact discovery::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **273/273** cumulative negative controls through N3; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0046` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
