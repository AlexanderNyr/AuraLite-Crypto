# D7 — Four-validator devnet (M1) report

Generated (UTC): `2026-10-31T19:37:36Z`
Deliverable: `patches/0045-D7-devnet.patch`

## Scope delivered

1. **Four-validator devnet composition (`crates/aura-node/src/devnet.rs`)**:
   - Deterministic devnet configuration (`DEVNET_CHAIN_ID = "auralite-devnet-1"`, `DEVNET_VALIDATOR_COUNT = 4`, `DEVNET_QUORUM_THRESHOLD = 3`, `DEVNET_GENESIS_TIME_MS = 1_700_000_000_000`).
   - Development-only faucet account (`DEVNET_FAUCET_SEED`, `faucet_address`, `build_faucet_transfer`) credited with `1,000,000,000,000` units at genesis across all four validators.
   - Peer identity verification (`verify_devnet_peer_identity`) rejecting substituted Ed25519 keys where `PeerId != BLAKE3(public_key)`.
   - Signed and deduplicated N2 wire frame ingestion (`ingest_devnet_gossip`) rejecting malformed/unsigned/forged frames and deduplicating repeated messages via `SeenCache`.
   - BFT `2f + 1` quorum threshold (`devnet_quorum_threshold(4) == 3`) and BLS vote verification/QC formation (`try_commit_round_votes`) ensuring 2 votes of 4 never commit.
   - In-process `DevnetCluster` and multi-process `aurad` worker (`run_devnet_worker`, `trigger_and_wait_devnet_round`) gating every local `Pipeline::produce_block()` on a verified 3-of-4 BLS `QuorumCertificate`.
2. **Multi-process M1 acceptance gate (`crates/aura-node/tests/d7_devnet.rs`, `tools/run_devnet.py`, `docker-compose.yml`)**:
   - Spawns 4 independent `aurad` validator processes with separate `--data-dir`, `--rpc-addr`, `--grpc-addr`, and `--validator-key`.
   - Verifies HTTP 200 on `/health` and Prometheus `/metrics` across all 4 validators.
   - Submits a signed faucet transfer via JSON-RPC (`submit_tx`), gossips it over N2 `GossipTopic::Tx`, and finalizes `>= 100` blocks (`height = 101`) with all 4 validators reporting identical `height`, `hash` (`block_hash`), `stateRoot` (`state_root`), and account balances (`get_block`, `get_balance`).
   - Stops `validator-3` (`SIGTERM`) and verifies the remaining 3 validators preserve the `f = 1` quorum of 3 and finalize additional blocks (`height = 113`) and transfers with identical `state_root`.
   - Records per-validator JSON structured logs in `docs/evidence/d7/validator-{0..3}.log` showing the common finalized `state_root`.

## Verification gates

- `make test-d7`: 6 `devnet::tests` library unit tests + 1 live 4-process `d7_devnet` M1 acceptance test PASS.
- `make devnet`: 4-validator `aurad` run finalizes `height=101` (`4/4`) and `height=113` (`3/4` after 1-validator outage) with identical `state_root` across online validators.
- `make nc-d7`: **6/6** library unit-test mutation controls PASS.
- `make nc-all`: **261/261** cumulative negative controls PASS across all 45 phases (`C0..C10`, `S0..S7`, `X0..X8`, `D0..D7`, `B0..B5`, `N0..N2`).
- `make test`: `ALL GATES GREEN`.

## Explicit non-claims

- Catch-up block/header sync for a restarted validator rejoining after an outage belongs to `NET_PLAN` **N4** (`Block/header sync`).
- Fast state sync over P2P belongs to `NET_PLAN` **N5** (`Fast sync hookup`).
- Full CLI operator subcommands (`aura` query/tx/keys/snapshot/upgrade) belong to `NODE_PLAN` **D8**.
