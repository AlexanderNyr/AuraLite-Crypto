# Format & evidence index — C10 close-out

This is the single entry point for every byte-level specification and every
governance artifact produced by `CORE_PLAN` C0–C10. `tools/audit_closeout.py`
fails if a file in `docs/spec/` is not listed here.

## 1. Protocol format specifications

| Phase | Document | Fixes |
|---|---|---|
| C4 | [`tx-format.md`](tx-format.md) | `UnsignedTx`, signature envelope, `signing_bytes`, txid, replay rules |
| C5 | [`block-format.md`](block-format.md) | `Header`/`Body`, `tx_root`, `block_hash`, canonical ordering |
| C6 | [`merkle-proof-format.md`](merkle-proof-format.md) | compact inclusion proofs for tx and event leaves |
| C7 | [`account-model.md`](account-model.md) | `Account`, `StateKey`, five namespaces, reserved prefixes |
| C8 | [`genesis-format.md`](genesis-format.md) | genesis JSON schema, canonical bytes, genesis commitments |
| C9 | [`test-rig.md`](test-rig.md) | golden vectors, proptest, portable fuzz harness, libFuzzer targets |
| D2 | [`node-pipeline.md`](node-pipeline.md) | native transfer payload, execution fees, block/finality/storage/event order |
| D3 | [`mempool.md`](mempool.md) | nonce lanes, tip priority, replacement bump, bounded eviction, reorg reinsertion |
| D4 | [`json-rpc.md`](json-rpc.md) | JSON-RPC 2.0 methods, HTTP/WS transports, errors, resource bounds, commit subscriptions |
| D5 | [`grpc-rest.md`](grpc-rest.md) | protobuf gRPC service, REST gateway, OpenAPI, additive v1.1 compatibility |

C0–C3 have no standalone format spec: the canonical types are defined by
`crates/aura-types` and pinned by `crates/aura-types/tests/fixtures/c1_golden.txt`,
the wire format by `crates/aura-codec` and `c2_golden.txt`, and the crypto by
`crates/aura-crypto` and `c3_golden.txt`. All three are covered by receipts in
`CORE_PLAN.md`.

## 2. Configuration and governance

| File | Purpose |
|---|---|
| [`genesis-testnet.json`](genesis-testnet.json) | human-readable Stage 1 genesis config; its hash is pinned by a test |
| [`c3-dependency-allowlist.txt`](c3-dependency-allowlist.txt) | **frozen** C3 baseline of 54 external crates (pinned by C3 evidence checksums) |
| [`c8-dependency-addendum.txt`](c8-dependency-addendum.txt) | reviewed +4 delta (serde_json config parsing) |
| [`c9-dependency-addendum.txt`](c9-dependency-addendum.txt) | reviewed +13 dev-only delta (proptest tree) |
| [`d4-dependency-addendum.txt`](../evidence/d4-dependency-addendum.txt) | pinned SHA-1/WebSocket handshake and safe signal-hook dependencies |
| [`d5-dependency-addendum.txt`](../evidence/d5-dependency-addendum.txt) | pinned tonic/prost/Tokio stack and vendored protoc for gRPC |
| [`FORMAT_INDEX.md`](FORMAT_INDEX.md) | this file |

`tools/check_deps.py` compares `Cargo.lock` against the union of the baseline
and all addenda. `cargo deny` (licences/advisories) is still owed to H1.

## 3. Phase → artifact ledger (C0–C10)

Each ✅ row in `docs/plans/CORE_PLAN.md` has all of: a numbered patch in
`patches/`, a `### Cx receipt` section in `CORE_PLAN.md`, a `## [Cx ...]`
section in `CHANGELOG.md`, an NC driver `tools/nc_cX.py`, an NC log
`docs/evidence/cX/nc.log` and evidence receipts under `docs/evidence/cX/`.

| Phase | Patch | NC | Evidence |
|---|---|---|---|
| C0 | `0001-C0-baseline-workspace.patch` | 2/2 | `c0-baseline.txt`, `c0/nc.log`, root `SHA256SUMS` |
| C1 | `0002-C1-types.patch` | 3/3 | `c1/gates.txt`, `c1/nc.log`, `c1/SHA256SUMS` |
| C2 | `0003-C2-codec.patch` | 4/4 | `c2/…` |
| C3 | `0004-C3-crypto.patch` | 5/5 | `c3/…` |
| C4 | `0005-C4-tx-format.patch` | 6/6 | `c4/…` |
| C5 | `0006-C5-block-header.patch` | 6/6 | `c5/…` |
| C6 | `0007-C6-merkle-proofs.patch` | 6/6 | `c6/…` |
| C7 | `0008-C7-account-model.patch` | 6/6 | `c7/…` |
| C8 | `0009-C8-genesis.patch` | 6/6 | `c8/…` |
| C9 | `0010-C9-test-rig.patch` | 6/6 | `c9/…` |
| C10 | `0011-C10-close-out.patch` | 6/6 | `c10/…` |

**Total: 56 negative controls.** The C10 patch is produced by the phase that
runs the audit, so the audit validates the ten already-shipped patches and
reports the C10 patch as present once the artifact commit lands.
`docs/evidence/c10/closeout.txt` is the consolidated NC inventory.

## 4. Honest scope

These documents fix **formats and receipts**, not a running network. No
validator set, no blocks, no transactions, no state tree, no VM, no P2P, no
RPC, no wallet. See [`docs/CLOSEOUT_AUDIT.md`](../CLOSEOUT_AUDIT.md) for the
measured maturity assessment.
