# NET_PLAN — P2P, gossipsub, sync, light clients

**Status:** 📋 PLAN (N0–N8)  
**Stage:** Stage 1: N0–N8 (all phases active).  
**Deliverable:** `patches/0041-N0-...` … `patches/0049-N8-...`
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
| **N0** | Baseline | two nodes over RPC, measure "no P2P", report | NC: node B cannot see node A's tx → documented gap |
| **N1** | Transport | libp2p swarm, Noise/QUIC, identity, handshake, ban list | 10 nodes connect, identity substitution → reject |
| **N2** | Gossipsub | topics, message signatures, dedup, peer scoring | 1000 txs broadcast in < 1 s; flooding peer loses score and is banned |
| **N3** | Discovery | Kademlia DHT, bootstrap list, NAT/upnp, relay | 30 nodes behind NAT find each other with no central server |
| **N4** | Block sync | header-first, QC verification, parallel body download | lagging node catches up 10 000 blocks and reaches the same head |
| **N5** | State sync | integration with S4: chunks, manifest, root verification | new node syncs within measured time; corrupt chunk → rollback + re-request |
| **N6** | DoS defence | size/rate limits, signature rate limit, equivocation detection | 1 malicious peer sends 10k msg/s → validator keeps finalizing |
| **N7** | Light client | header+QC proof, finalized checkpoint check, simplified data-availability sampling | light client rejects a forged header; proof gas/size bounded |
| **N8** | Chaos harness | latency/loss/partition/eclipse simulation, 100-node stand | 20 scenarios × 3 runs: zero finalized-head divergence |

## Risks

- **Eclipse attack** — diversify the peer set + N3/N8.
- **Signature storm** — BLS aggregation (B2) + N6 limits.
- **Protocol version mismatch** — `protocol_version` in the handshake.

## Non-goals

HTTP API (NODE D4–D5), storage (STATE), economics (ECONOMICS).

## Closure criteria

One hundred nodes with 20% packet loss and one Byzantine peer reach the same
finalized `state_root`; a light client verifies this without a full node.
