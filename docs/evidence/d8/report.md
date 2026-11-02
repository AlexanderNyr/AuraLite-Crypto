# D8 — `aura` CLI report (`NODE_PLAN` complete)

Generated (UTC): `2026-10-31T20:21:35Z`
Deliverable: `patches/0046-D8-cli-aura.patch`

## Scope delivered

1. **`aura` operator and client CLI (`crates/aura-node/src/cli.rs`, `crates/aura-genesis/src/bin/aura.rs`)**:
   - Complete `--help` / `-h` / `help` and `--version` / `-V` documentation.
   - `genesis` (`hash`, `verify`, `show`) — C8 deterministic genesis inspection and verification.
   - `keys` (`derive`, `inspect`, `sign`, `verify`) — Ed25519 key derivation, Bech32 address inspection, and domain-separated `Domain::Tx` signing and verification.
   - `tx` (`transfer`, `decode`, `hash`) — canonical C4 transfer construction with gas-floor (`21,000`) and fee-cap (`max_fee >= base_fee + tip`) enforcement, offline canonical decoding/verification, and optional live JSON-RPC submission (`--rpc`).
   - `query` (`balance`, `block`, `tx`, `health`) — JSON-RPC 2.0 and HTTP `/health` queries against a live `aurad` node.
   - `node` (`config-check`, `status`, `health`) — strict TOML node configuration validation (rejecting unknown keys and missing `chain_id`) and live daemon status inspection.
   - `snapshot` (`create`, `verify`, `info`) — domain-separated deterministic snapshot manifest digest creation and constant-time verification (`SNAPSHOT_CLI_DOMAIN`).
   - `upgrade` (`plan`, `verify`) — domain-separated deterministic protocol upgrade plan generation and verification (`UPGRADE_CLI_DOMAIN`), enforcing strictly increasing protocol versions (`to_version > from_version`).
2. **30 Golden CLI vectors and live devnet integration (`crates/aura-node/tests/fixtures/d8_golden.txt`, `crates/aura-node/tests/d8_cli.rs`, `tools/gen_d8_vectors.py`)**:
   - 30 golden vectors (`d8_cli_00` .. `d8_cli_29`) covering `keys`, `tx`, `genesis`, `snapshot`, and `upgrade`.
   - End-to-end live `aurad` devnet test exercising `aura tx transfer --rpc`, `aura query balance`, `aura query tx`, `aura query block`, `aura node health`, and `aura node status`.

## Verification gates

- `make test-d8`: 6 `cli::tests` library unit tests + 2 `d8_cli` integration tests PASS.
- `make nc-d8`: **6/6** library unit-test mutation controls PASS.
- `make nc-all`: **267/267** cumulative negative controls PASS across all 46 phases (`C0..C10`, `S0..S7`, `X0..X8`, `D0..D8`, `B0..B5`, `N0..N2`).
- `make test`: `ALL GATES GREEN` (5,129 golden vectors across 39 fixtures).
