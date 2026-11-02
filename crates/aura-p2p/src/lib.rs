//! `aura-p2p` — P2P transport, identity, handshake, ban list, gossipsub, discovery, block sync, state sync, DoS defence, light client, and chaos harness.
//!
//! N0 established the honest baseline measurement. N1 landed authenticated
//! peer identity (`peer_id = BLAKE3(ed25519_pubkey)`), the bounded Noise-inspired
//! handshake with protocol-version and `chain_id` binding, post-handshake
//! encrypted/MAC'd session framing, the [`ban::BanList`], and the TCP
//! [`transport::TransportSwarm`]. N2 landed [`gossipsub`] with separate
//! topics (`tx`, `proposal`, `vote`, `evidence`, `snapshot`), domain-separated
//! Ed25519 message signatures, deduplication (`SeenCache`), and peer scoring
//! with flood/invalid-message penalties and ban-list escalation. N3 landed
//! [`discovery`]: the 256-bucket Kademlia DHT (`XorDistance`, `KademliaTable`),
//! signed peer records (`SignedPeerRecord`), [`discovery::BootstrapList`],
//! NAT/UPnP port mapping (`UpnpMapper`), and Circuit Relay v2 (`RelayServer`).
//! N4 landed [`block_sync`]: header-first chain verification (`SyncedHeader`),
//! Quorum Certificate (`SyncQc`) verification against [`block_sync::SyncValidatorSet`],
//! and parallel body download (`SyncedBlockBody`, `BlockSyncEngine`).
//! N5 landed [`state_sync`]: network integration with S4 deterministic
//! snapshots (`SnapshotManifest`, chunk SHA-256 verification, corrupt-chunk
//! rollback + peer ban + re-request, and S1 Merkle `state_root` verification).
//! N6 landed [`dos_defence`]: pre-crypto frame size & message rate limits,
//! per-peer signature verification CPU budgets, and equivocation detection
//! (`EquivocationEvidence`, `DosDefenceEngine`).
//! N7 landed [`light_client`]: header+QC proof verification, finalized
//! checkpoint progression (`FinalizedCheckpoint`), simplified Data-Availability
//! Sampling (`DasChunkSample`), and strict proof byte-size and verification-gas
//! bounds (`LightClientVerifier`).
//! **N8** lands [`chaos`]: deterministic latency, packet-loss, network-partition,
//! eclipse-attack, and Byzantine-peer simulation over a 100-node stand
//! (`ChaosStand`, `canonical_20_scenarios`, `run_chaos_matrix`), completing
//! `NET_PLAN` (`N0`–`N8`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod ban;
pub mod baseline;
pub mod block_sync;
pub mod chaos;
pub mod discovery;
pub mod dos_defence;
pub mod error;
pub mod gossipsub;
pub mod identity;
pub mod light_client;
pub mod state_sync;
pub mod transport;
