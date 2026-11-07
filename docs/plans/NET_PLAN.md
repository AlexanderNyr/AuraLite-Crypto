# NET_PLAN — P2P, gossipsub, sync, light clients

**Status:** ✅ COMPLETE — N0–N8 ✅ (`patches/0040-N0-net-baseline.patch`..`patches/0042-N2-gossipsub.patch`, `patches/0047-N3-discovery.patch`..`patches/0052-N8-chaos-harness.patch`).
**Stage:** Stage 1: N0–N8 (all phases active and complete).
**Deliverable:** `patches/0040-N0-net-baseline.patch`..`patches/0042-N2-gossipsub.patch`, `patches/0047-N3-discovery.patch`..`patches/0052-N8-chaos-harness.patch` (landed).
**Dependencies:** CORE C3–C5, STATE S4, CONSENSUS B2 (votes).

## Problem

Without P2P the "network" is one process. We need discovery, delivery of
blocks/votes, spam protection, sync for lagging nodes and light clients. At the
same time the network is the main DoS surface: one malicious peer must not take
down a validator.

## Decisions

- **D-N1.** `rust-libp2p`: TCP + QUIC, Noise, identify, `peer_id = hash(pubkey)`.
- **D-N2.** gossipsub with separate topics: `tx`, `proposal`, `vote`, `evidence`, `snapshot`; peer scoring enabled from day one, not "later".
- **D-N3.** Everything arriving from the network passes the same validation as locally; "trusted peer" is a nonexistent concept.
- **D-N4.** Sync is header-first: headers/QCs first, then bodies/snapshots.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **N0** | Baseline | two nodes over RPC, measure "no P2P", report | NC: node B cannot see node A's tx → documented gap |
| ✅ **N1** | Transport | libp2p swarm, Noise/QUIC, identity, handshake, ban list | 10 nodes connect, identity substitution → reject |
| ✅ **N2** | Gossipsub | topics, message signatures, dedup, peer scoring | 1000 txs broadcast in < 1 s; flooding peer loses score and is banned |
| ✅ **N3** | Discovery | Kademlia DHT, bootstrap list, NAT/upnp, relay | 30 nodes behind NAT find each other with no central server |
| ✅ **N4** | Block sync | header-first, QC verification, parallel body download | lagging node catches up 10 000 blocks and reaches the same head |
| ✅ **N5** | State sync | integration with S4: chunks, manifest, root verification | new node syncs within measured time; corrupt chunk → rollback + re-request |
| ✅ **N6** | DoS defence | size/rate limits, signature rate limit, equivocation detection | 1 malicious peer sends 10k msg/s → validator keeps finalizing |
| ✅ **N7** | Light client | header+QC proof, finalized checkpoint check, simplified data-availability sampling | light client rejects a forged header; proof gas/size bounded |
| ✅ **N8** | Chaos harness | latency/loss/partition/eclipse simulation, 100-node stand | 20 scenarios × 3 runs: zero finalized-head divergence |

### N0 receipt

- **Deliverable:** `patches/0040-N0-net-baseline.patch`.
- **Code:** `crates/aura-p2p/src/baseline.rs`, `crates/aura-p2p/src/bin/main.rs`,
  `crates/aura-p2p/src/lib.rs`, `crates/aura-p2p/tests/fixtures/n0_baseline.txt`,
  and `crates/aura-node/tests/n0_two_nodes_no_p2p.rs`.
- **Measurement:** all eight `NET_PLAN` components (`transport` N1 through
  `chaos-harness` N8) measure as `placeholder` (0 implemented, 8 placeholders,
  0 missing). Running `aura-p2p` with no arguments exits `2` (`EXIT_STUB`) and
  names every missing N1–N8 capability.
- **Two-node RPC isolation:** two live `aurad` HTTP/JSON-RPC nodes (`node-a`
  and `node-b`) start on independent ports with `peerCount = 0` and
  `p2p = "not_implemented"`; a signed transfer submitted and committed on Node A
  is served by Node A over `get_tx`/`get_balance`, while Node B returns `null`
  for `get_tx`, keeps recipient balance `0`, and stays at height `0`.
- **Gates:** `make test-n0`, `make nc-n0` (**6/6** exact unit-test mutations),
  `make nc-all` (**231/231**), `make test` (**ALL GATES GREEN**). Evidence:
  `docs/evidence/n0/`.

### N1 receipt

- **Deliverable:** `patches/0041-N1-transport.patch`.
- **Code:** `crates/aura-p2p/src/identity.rs`, `crates/aura-p2p/src/ban.rs`,
  `crates/aura-p2p/src/transport.rs`, `crates/aura-p2p/src/error.rs`,
  `crates/aura-p2p/src/baseline.rs`, `crates/aura-p2p/tests/n1_transport.rs`,
  `crates/aura-p2p/tests/n1_spoofed_peer_ban.rs`,
  `crates/aura-p2p/tests/fixtures/n1_baseline.txt`, `tools/gen_n1_vectors.py`,
  `tools/nc_n1.py`, `tools/nc_p2p_review.py`, and `tools/write_n1_evidence.py`.
- **Identity & handshake (`D-N1`):** `PeerId = BLAKE3(ed25519_pubkey)` verified
  in constant time (`PeerId::verify_public_key`); domain-separated Ed25519
  transcript signatures (`AURA_P2P_HANDSHAKE_V1\0`) binding `protocol_version`,
  `chain_id`, initiator/responder role, challenges, and public key; directional
  BLAKE3-keyed session encryption + MAC (`SessionCipher`) with monotonic
  sequence numbers; deterministic `BanList` for `PeerId` and `IpAddr`.
- **Swarm & identity substitution gate:** live `TransportSwarm` nodes connect
  over TCP, authenticate each other, reject identity substitution
  (`P2pError::IdentitySubstitution`) without automatically banning another
  endpoint, reject self-connections, and enforce explicit peer/IP bans.
- **Historical gates:** the phase-close receipts in `docs/evidence/n1/`
  describe the earlier implementation and are intentionally preserved; see the
  security-review follow-up below for current behavior.

#### N1 security-review follow-up (2026-11-05; WIP)

The earlier transport automatically banned `HandshakeHello.claimed_peer_id`
for a claimed-ID/public-key mismatch, an expected-peer mismatch, or an invalid
Ed25519 transcript signature. Those values were attacker-controlled until a
valid signature proved possession of the corresponding key, so a remote peer
could permanently ban a victim by spoofing its public PeerId. The handshake no
longer creates identity bans on these unauthenticated failure paths; identity-
ban-list checks occur only after valid transcript-signature verification. An
expected-peer mismatch is rejected without an automatic ban, even after
authentication, because reaching a different honest endpoint is not by itself
evidence of malicious behavior. No automatic IP ban was added.

Live regressions cover inbound claimed-ID mismatch, inbound and outbound
invalid signatures, authenticated outbound identity substitution, and retention
of ban checks for authenticated peers. `make nc-p2p-review` has four mutation
controls that reintroduce the vulnerable ban behaviors
and require the named regression tests to go red. Historical N1 reports and
checksums remain unchanged. This correction is WIP and has not received an
independent review or production approval.

### N2 receipt

- **Deliverable:** `patches/0042-N2-gossipsub.patch`.
- **Code:** `crates/aura-p2p/src/gossipsub.rs`, `crates/aura-p2p/src/baseline.rs`,
  `crates/aura-p2p/src/error.rs`, `crates/aura-p2p/tests/n2_gossipsub.rs`,
  `crates/aura-p2p/tests/fixtures/n2_baseline.txt`, `tools/gen_n2_vectors.py`,
  `tools/nc_n2.py`, and `tools/write_n2_evidence.py`.
- **Topics, signatures, dedup, and peer scoring (`D-N2`, `D-N3`):**
  `GossipTopic` (`tx`, `proposal`, `vote`, `evidence`, `snapshot`);
  `GossipMessage` with domain-separated Ed25519 signatures
  (`AURA_P2P_GOSSIP_SIG_V1\0`) and `PeerId == BLAKE3(pubkey)` verification;
  bounded `SeenCache` deduplication over deterministic `MessageId`;
  `PeerScoreParams`, `PeerScoreState`, and `GossipEngine` with sliding-window
  flood rate limiting, invalid-message penalties, and automatic escalation into
  `BanList` at `ban_threshold`.
- **1000-tx 10-node mesh & flood ban gate:** 1,000 signed transactions broadcast
  across a 10-node mesh (`GossipMesh` ring + chords) in `< 1 s`, with every node
  receiving all 1,000 transactions and deduplicating redundant deliveries;
  flooding and signature-forging peers lose score and are banned.
- **Gates:** `make test-n2`, `make nc-n2` (**6/6** exact library unit-test
  mutations), `make nc-all` (**243/243**), `make test` (**ALL GATES GREEN**).
  Evidence: `docs/evidence/n2/`.

### N3 receipt

- **Deliverable:** `patches/0047-N3-discovery.patch`.
- **Code:** `crates/aura-p2p/src/discovery.rs`, `crates/aura-p2p/src/baseline.rs`,
  `crates/aura-p2p/src/error.rs`, `crates/aura-p2p/tests/n3_discovery.rs`,
  `crates/aura-p2p/tests/fixtures/n3_baseline.txt`, `tools/gen_n3_vectors.py`,
  `tools/nc_n3.py`, and `tools/write_n3_evidence.py`.
- **Kademlia DHT, bootstrap list, NAT/UPnP, and Circuit Relay v2 (`D-N1`, `D-N3`):**
  256-bit `XorDistance` (`between`, `bucket_index`) and 256-bucket `KademliaTable`
  (`k = 20`, `alpha = 3`, monotonic `seq` replay protection, `BanList` check,
  and `closest_peers`); domain-separated Ed25519 `SignedPeerRecord`
  (`DISCOVERY_RECORD_DOMAIN = "AuraLite-Crypto/p2p-discovery-record-v1\0"`) with
  constant-time `PeerId == BLAKE3(pubkey)` and `chain_id` verification;
  `BootstrapList`; `UpnpMapper` (`NatStatus::UpnpMapped`); and `RelayServer` /
  `RelayReservation` (`NatStatus::BehindSymmetricNat`) with TTL and frame caps.
- **30-node behind-NAT discovery gate:** 30 NATed nodes (5 cone-NAT UPnP-mapped
  relay-capable nodes + 25 symmetric-NAT nodes) seeded with at most 2 local
  neighbors and no central server converge in `<= 4` iterative Kademlia lookup
  rounds so every node discovers all 29 other verified peer records and routes
  traffic across Circuit Relay v2 and UPnP endpoints.
- **Gates:** `make test-n3`, `make nc-n3` (**6/6** exact library unit-test
  mutations), `make nc-all` (**273/273**), `make test` (**ALL GATES GREEN**).
  Evidence: `docs/evidence/n3/`.

### N4 receipt

- **Deliverable:** `patches/0048-N4-block-sync.patch`.
- **Code:** `crates/aura-p2p/src/block_sync.rs`, `crates/aura-p2p/src/baseline.rs`,
  `crates/aura-p2p/src/error.rs`, `crates/aura-p2p/tests/n4_block_sync.rs`,
  `crates/aura-p2p/tests/fixtures/n4_baseline.txt`, `tools/gen_n4_vectors.py`,
  `tools/nc_n4.py`, and `tools/write_n4_evidence.py`.
- **Header-first sync, QC verification, and parallel body download (`D-N3`, `D-N4`):**
  `SyncedHeader` (`BLOCK_SYNC_HEADER_DOMAIN = "AuraLite-Crypto/p2p-block-sync-header-v1\0"`,
  contiguous height, `parent_hash`, monotonic timestamp, and `block_hash`
  verification); `SyncQc` and `SyncValidatorSet` (`BLOCK_SYNC_QC_DOMAIN = "AuraLite-Crypto/p2p-block-sync-qc-v1\0"`,
  domain-separated Ed25519 signature verification, duplicate/unknown signer
  rejection, and `>= 2/3 + 1` quorum stake enforcement); `SyncedBlockBody`
  (`BLOCK_SYNC_BODY_DOMAIN = "AuraLite-Crypto/p2p-block-sync-body-v1\0"`,
  `tx_root` verification against verified header); and `BlockSyncEngine`
  (`ingest_header_batch`, `download_bodies_parallel` across `std::thread::scope`
  workers, out-of-order chunk assembly, and automatic `BanList` escalation on
  corrupt headers, QCs, or bodies).
- **10,000-block catch-up gate:** a lagging node starting at height `0` verifies
  10,000 headers + 3-of-4 QCs and downloads 10,000 block bodies in parallel
  across 4 peer workers (with out-of-order completion), reaching height `10,000`
  with the exact canonical `block_hash` and `state_root`; a peer serving a
  corrupted body is banned and the node recovers from an honest peer.
- **Gates:** `make test-n4`, `make nc-n4` (**6/6** exact library unit-test
  mutations), `make nc-all` (**279/279**), `make test` (**ALL GATES GREEN**).
  Evidence: `docs/evidence/n4/`.

### N5 receipt

- **Deliverable:** `patches/0049-N5-state-sync.patch`.
- **Code:** `crates/aura-p2p/src/state_sync.rs`, `crates/aura-p2p/src/baseline.rs`,
  `crates/aura-p2p/src/error.rs`, `crates/aura-p2p/Cargo.toml`,
  `crates/aura-p2p/tests/n5_state_sync.rs`,
  `crates/aura-p2p/tests/fixtures/n5_baseline.txt`, `tools/gen_n5_vectors.py`,
  `tools/nc_n5.py`, and `tools/write_n5_evidence.py`.
- **S4 chunks, manifest, root verification, and corrupt-chunk rollback (`D-N3`, `D-N4`):**
  `StateSyncSession::from_certified_header` anchors on a QC-certified
  `SyncedHeader` (`SyncQc` verified against `SyncValidatorSet`);
  `accept_manifest` verifies the canonical S4 `SnapshotManifest` (`ALS4` magic,
  SHA-256 checksum, `chunk_root`, `chain_id`, `height`, and `state_root`
  equality with the certified header); `ingest_chunk` and `sync_from_peers`
  verify every S4 chunk (`length`, `chunk_hash`, `decode_chunk`), roll back
  staged data on corruption, ban the offending peer (`BanList`), and re-request
  the chunk from an honest fallback peer; `finalize_and_verify_root` runs S4
  parallel `import_snapshot` and recomputes the S1 sparse Merkle
  `state_root(&entries)`.
- **New-node S4 state sync & corrupt-chunk failover gate:** a new node syncs a
  1,000-account S4 state snapshot (10 chunks) within `< 5 s`, rolls back and
  bans a peer serving a corrupted chunk `#1`, re-requests chunk `#1` from an
  honest peer, and commits the verified state with an identical S1 `state_root`.
- **Gates:** `make test-n5`, `make nc-n5` (**6/6** exact library unit-test
  mutations), `make nc-all` (**285/285**), `make test` (**ALL GATES GREEN**).
  Evidence: `docs/evidence/n5/`.

### N6 receipt

- **Deliverable:** `patches/0050-N6-dos-defence.patch`.
- **Code:** `crates/aura-p2p/src/dos_defence.rs`, `crates/aura-p2p/src/baseline.rs`,
  `crates/aura-p2p/src/error.rs`, `crates/aura-p2p/src/lib.rs`,
  `crates/aura-p2p/tests/n6_dos_defence.rs`,
  `crates/aura-p2p/tests/fixtures/n6_baseline.txt`, `tools/gen_n6_vectors.py`,
  `tools/nc_n6.py`, and `tools/write_n6_evidence.py`.
- **Size/rate limits, signature budget, and equivocation detection (`D-N3`):**
  `DosDefenceLimits`, `IngressKind`, `IngressEnvelope`
  (`DOS_ENVELOPE_DOMAIN = "AuraLite-Crypto/p2p-dos-envelope-v1\0"`),
  `EquivocationEvidence`, and `DosDefenceEngine` enforcing `O(1)` pre-crypto
  `BanList` rejection, `max_frame_bytes` (`256 KiB`), per-peer sliding-window
  message rate limits (`128 msg/s`), per-peer Ed25519 signature verification CPU
  budgets (`64 sig/s`), invalid-signature strike escalation, and consensus
  double-proposal / double-vote equivocation detection with verifiable
  cryptographic evidence.
- **10k msg/s flood isolation & validator finalization gate:** 1 malicious peer
  sends 10,000 msg/s (flood, corrupted signatures, and oversized frames) across
  a 1-second window while 4 honest validators exchange proposals and 2f+1 votes
  and finalize heights `1..=5` uninterrupted; the malicious peer is banned and
  `>= 9,936` flood messages are dropped pre-crypto.
- **Gates:** `make test-n6`, `make nc-n6` (**6/6** exact library unit-test
  mutations), `make nc-all` (**291/291**), `make test` (**ALL GATES GREEN**).
  Evidence: `docs/evidence/n6/`.

### N7 receipt

- **Deliverable:** `patches/0051-N7-light-client.patch`.
- **Code:** `crates/aura-p2p/src/light_client.rs`, `crates/aura-p2p/src/baseline.rs`,
  `crates/aura-p2p/src/error.rs`, `crates/aura-p2p/src/lib.rs`,
  `crates/aura-p2p/tests/n7_light_client.rs`,
  `crates/aura-p2p/tests/fixtures/n7_baseline.txt`, `tools/gen_n7_vectors.py`,
  `tools/nc_n7.py`, and `tools/write_n7_evidence.py`.
- **Header+QC proof, finalized checkpoint check, and simplified DAS (`D-N3`, `D-N4`):**
  `FinalizedCheckpoint`, `LightClientBounds` (`max_proof_bytes = 16 KiB`,
  `max_verification_gas = 150,000`, `min_das_samples = 4`), `DasChunkSample`
  (`LIGHT_CLIENT_LEAF_DOMAIN = "AuraLite-Crypto/p2p-light-leaf-v1\0"`,
  `LIGHT_CLIENT_BRANCH_DOMAIN = "AuraLite-Crypto/p2p-light-branch-v1\0"`,
  `build_das_tree`, `sample_das_chunks`), `LightClientProof`, and
  `LightClientVerifier::verify_and_advance` enforcing pre-crypto proof size and
  gas bounds, finalized checkpoint monotonicity, `SyncedHeader::verify_self` and
  `SyncQc::verify` (`2f+1` validator stake), and DAS chunk Merkle branch
  verification against `header.tx_root` with automatic `BanList` isolation of
  corrupt peers.
- **Forged-header rejection & bounded proof gas/size gate:** a light client
  verifies header+QC+DAS proofs without a full node, rejects a forged header
  (altered `state_root` or header hash not matching the 2f+1 QC) and corrupted
  DAS samples while banning the offending peers, and verifies both adjacent and
  multi-block checkpoint transitions (`1,000 -> 1,001 -> 1,500`) with bounded
  proof size (`< 4 KiB <= 16 KiB`) and verification gas (`< 50,000 <= 150,000`).
- **Gates:** `make test-n7`, `make nc-n7` (**6/6** exact library unit-test
  mutations), `make nc-all` (**297/297**), `make test` (**ALL GATES GREEN**).
  Evidence: `docs/evidence/n7/`.

### N8 receipt

- **Deliverable:** `patches/0052-N8-chaos-harness.patch`.
- **Code:** `crates/aura-p2p/src/chaos.rs`, `crates/aura-p2p/src/baseline.rs`,
  `crates/aura-p2p/src/error.rs`, `crates/aura-p2p/src/lib.rs`,
  `crates/aura-p2p/tests/n8_chaos.rs`,
  `crates/aura-p2p/tests/fixtures/n8_baseline.txt`, `tools/gen_n8_vectors.py`,
  `tools/nc_n8.py`, and `tools/write_n8_evidence.py`.
- **Latency/loss/partition/eclipse/Byzantine simulation & 100-node stand (`D-N3`, `D-N4`):**
  `ChaosScenarioConfig`, `ChaosNodeState`, `ChaosStand`, `ChaosRunReport`,
  `ChaosCampaignSummary`, `canonical_20_scenarios` (`CHAOS_PRNG_DOMAIN = "AuraLite-Crypto/p2p-chaos-prng-v1\0"`),
  `verify_cluster_convergence`, and `run_chaos_matrix` simulating 100 P2P nodes
  under variable link latency + jitter, 5%–40% packet loss, temporary network
  partitions with post-heal header-first catch-up sync, Sybil eclipse attacks
  defeated by `2f+1` QC verification, and Byzantine equivocation detected and
  banned by `DosDefenceEngine`.
- **20 scenarios × 3 runs & `NET_PLAN` closure-criteria gate:** all 60 runs
  (`20 scenarios × 3 runs`) on the 100-node stand complete with zero
  finalized-head (`block_hash` / `state_root`) divergence (`divergent_runs == 0`);
  in scenario `S01_100nodes_20pct_loss_1_byzantine`, 100 nodes with 20% packet
  loss and 1 Byzantine peer converge on the identical finalized `state_root`
  (with all 99 honest nodes banning the Byzantine equivocator), and an N7
  `LightClientVerifier` verifies the finalized head + QC + DAS proof without a
  full node.
- **Gates:** `make test-n8`, `make nc-n8` (**6/6** exact library unit-test
  mutations), `make nc-all` (**303/303**), `make test` (**ALL GATES GREEN**).
  Evidence: `docs/evidence/n8/`.

## Risks

- **Eclipse attack** — diversify the peer set + N3/N8.
- **Signature storm** — BLS aggregation (B2) + N6 limits.
- **Protocol version mismatch** — `protocol_version` in the handshake.

## Non-goals

HTTP API (NODE D4–D5), storage (STATE), economics (ECONOMICS).

## Closure criteria

One hundred nodes with 20% packet loss and one Byzantine peer reach the same
finalized `state_root`; a light client verifies this without a full node.
