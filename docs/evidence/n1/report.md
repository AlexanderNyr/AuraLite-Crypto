# N1 — P2P transport, identity, handshake, and ban list evidence receipt

Generated (UTC): 2026-10-31T10:25:36+00:00.

## Delivered behavior

`aura-p2p` implements the **N1** transport layer (`aura_p2p::identity`,
`aura_p2p::ban`, `aura_p2p::transport`, `aura_p2p::error`) and updates
`aura_p2p::baseline` to report 1 implemented network component (`transport`)
and 7 remaining placeholders (`gossipsub` N2 .. `chaos-harness` N8):

- **Cryptographic peer identity (`D-N1`)**: `PeerId = BLAKE3(ed25519_public_key)`
  with constant-time verification (`PeerId::verify_public_key`) and
  domain-separated Ed25519 handshake transcript signing/verification
  (`AURA_P2P_HANDSHAKE_V1\0` binding `protocol_version`, `chain_id`,
  initiator/responder role, local & remote challenges, and public key).
- **Ban list**: deterministic `BanList` tracking banned `PeerId`s and `IpAddr`s
  with permanent or expiring bans (`BanEntry`, `BanReason`).
- **Noise-inspired authenticated transport & TCP swarm**:
  - `HandshakeHello` challenge-response exchange verifying `protocol_version`,
    `chain_id`, `PeerId == BLAKE3(public_key)`, expected remote `PeerId` (when
    dialing a known peer), and mutual Ed25519 transcript signatures.
  - `SessionCipher` directional BLAKE3-keyed stream + MAC session framing with
    monotonic sequence numbers and `MAX_FRAME_BYTES` cap.
  - `TransportSwarm` live TCP listener + dialer connecting 10 independent nodes,
    rejecting and auto-banning identity substitution (`P2pError::IdentitySubstitution`),
    rejecting self-connections, and enforcing peer/IP bans.

N1 does not implement gossipsub (N2), Kademlia discovery (N3), block/state sync
(N4–N5), or multi-validator devnet (D7).

## Gate results

- `make test-n1`: 18 `aura-p2p` library unit tests and 2 live 10-node TCP
  `TransportSwarm` integration tests passed.
- `make nc-n1`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact ...`); restored library tests passed.
- `make nc-all`: **237/237** cumulative negative controls through N1; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD (`2cf0321a83d6b308950a196b476ab086bf50c42c` + `0040-N0`)
  apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
