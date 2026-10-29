# ARCHITECTURE — AuraLite-Crypto

This document fixes the **invariants**. Plans (`docs/plans/`) fix the order;
this file fixes what cannot change without a new protocol version.

## 0. Network identity (D0, 2026-10-25)

| Field | Value |
|---|---|
| Project | `AuraLite-Crypto` |
| Network / chain-id (Stage 1) | `AuraLite-Crypto Network` / `auralite-test-1` |
| chain-id (Stage 2, reserved) | `auralite-1` |
| Native asset | `ALCRY`, 18 decimals, smallest unit `aalcry` |
| Address | `bech32("aura", version || pubkey_hash)` → `aura1...` |
| Crates | `aura-*` is an internal namespace, **not** a ticker |
| Consensus | BFT-PoS (2f+1 QC, VRF, slashing) |
| VM | WASM (`wasmtime`) + versioned ABI |

The ticker was checked for collisions: [`TICKER_AUDIT.md`](TICKER_AUDIT.md).
Asset identity is always `(chain_id, denom/contract_address)`, never the ticker
(rule D5 in `ROADMAP.md`). Stage 1 issues no market token.

## 1. Formal network contract

```
Block N = Header || Body
Header  = { version, chain_id, height, parent_hash, tx_root, state_root,
            validator_set_hash, timestamp_ms, proposer, qc, gas_used, base_fee }
Body    = [Tx]  (canonically sorted by (signer, nonce))
State   = JMT(key -> value), root = BLAKE3(node)
Tx      = { chain_id, signer, nonce, kind, payload, gas_limit,
            max_fee, tip, expiry, signature }
```

- **`state_root` is the only truth.** Two nodes with the same tx history must
  produce a byte-identical root; a difference is a consensus bug (X7).
- **Deterministic order.** No iteration over `HashMap`, no "whatever order"
  sorting, no dependency on local time.
- **`timestamp_ms` is a consensus field**, not `SystemTime::now()`: median of
  the times in the QC, monotonically increasing, allowed window fixed in B3.
- **Versioning.** `version` in the header, `codec_version` in tx,
  `abi_version` in contracts. Any change = migration test (C10/B10).

**⚠️ AUDIT (S3, 2026-10-27).** The §3 graph earlier claimed `aura-state <- aura-block, aura-tx, aura-codec, aura-crypto, aura-storage`, which is false and would be a cycle: `aura-storage` depends on `aura-state`. The graph above now reflects the real `Cargo.toml` edges; this is a docs correction, not a code change.

**X0 determinism census.** `aura_exec::census` is a static, fail-closed WASM
classifier — the first real API of `aura-exec`, which was an honest empty
placeholder since C0. It parses magic/version, section framing and order,
imports, tables, memories, globals and every instruction of every function
body, and publishes the `CENSUS` table (source, feature, policy, enforcement
layer, why) plus `ModuleLimits::restrictive()`. Floats, imports, shared
memory, atomics, SIMD, bulk memory, indirect calls, the start section and
mutable globals are banned; an unlisted opcode is rejected (`UnknownOpcode`).
No engine is linked in X0, so the crate graph was unchanged by that phase.

**X1 sandbox.** `aura_exec::sandbox` is the runtime half, a pipeline whose every
step can only reject: size check → X0 census → engine compile → **zero-import**
check → instantiate with store limits → call `run` → closed trap classification.
`SandboxLimits::restrictive()` publishes fuel 1,000,000, memory 4096 pages,
module 512 KiB, stack 512 KiB and trace 4096 events; `validate()` refuses any
value looser than the X0 census ceiling, so the static gate and the runtime
cannot drift. The engine is behind the `Engine` trait: `wasmi` (default,
deterministic interpreter, `portable-dispatch`) and `wasmtime` (feature
`wasmtime-engine`, Cranelift). Both are configured from the same limits and
return the same `Execution { outcome, fuel_used, trace, exports }`, which is what
makes the X7 cross-engine determinism gate expressible. `Execution` carries a
bounded trace (call enter/exit) so a divergence is visible as an event stream
difference, not just a different number.

**X2 gas + ABI v1.** `aura_exec::abi` is the host interface a contract
compiles against: module `env`, version 1, **seven** functions
(`storage_read`, `storage_write`, `balance`, `transfer`, `emit`, `hash`,
`block_ctx`), all `(i32, …) -> i32`, with a closed status-code set (`0` success,
`-1` out-of-bounds, `-2` key too long, `-3` value too long, `-4` topic too long,
`-5` not owned, `-6` insufficient balance, `-7` overflow, `-8` out of gas,
`-11` too many events, plus the non-error statuses `-9` absent key and `-10`
unknown block field) and published limits (`MAX_KEY_LEN` 32, `MAX_VALUE_LEN`
4096, `MAX_TOPIC_LEN` 64, `MAX_ABI_BUFFER` 65 536). A storage key becomes a
32-byte slot by zero padding, which is documented (trailing NULs are not
significant) rather than accidental. `aura_exec::contract` adds `HostState`
(S3 overlay + call + gas), `Executor::call(wasm, base_tree, call, arg)` and
`ContractReceipt { status, gas_used, host_gas, events, writes, diff,
result_root }`. **Gas has exactly one counter**: 1 fuel = 1 gas, and the engine's
instruction meter is reset to `gas_left` at every host-call boundary, so
instruction gas and host charges cannot be counted twice or under-counted (the
X1 defect). Commit rule: a `Returned` outcome commits a canonical `StateDiff`
from the overlay; `OutOfGas`, any trap and any refusal roll the overlay back and
report the base root. **X5 ABI v2 + standards.** `aura_exec::abi_v2` is a **second** import module,
`env2`, with six functions (`calldata_len`, `calldata_read`, `return_write`,
`caller`, `self_address`, `abi_version`). It is a separate module name rather
than a new version of `env` because `env` is frozen by 23 golden vectors and by
every contract already deployed: a v1 module cannot even link `env2`, so its
meaning cannot move. Calldata is bounded at 1 KiB and the answer at 1 KiB
(`MAX_CALLDATA_LEN` / `MAX_RETURN_LEN`), both refused by the *session* before
the engine runs; `return_data` on the receipt is empty unless the call committed.
Three contracts under `standards/` — ARC-20, ARC-721 and a 3-signer multisig —
are written against `aura-sdk`, dispatch on a 4-byte little-endian selector, and
are built to wasm and pinned by size and hash. Because a negative `Returned`
status still commits under X2, a standard's refusal is a code **and zero
writes**, and every standards gate asserts the empty change set.

The ABI is linked only on the default `wasmi` backend —
the `wasmtime` backend returns `EngineUnavailable` for hosted calls instead of
claiming support.

**S7 recovery and backup.** `aura-storage::backup` builds a self-verifying
bundle — `u32le(manifest_len) || manifest || WAL blob` — where the manifest
carries a BLAKE3 checksum (domain `S7_MANIFEST_CONTEXT`), the chain-id, head
version/root, record count, blob length and `SHA-256(blob)`, while the blob is
the S2 framed, BLAKE3-checksummed WAL. `restore_backup` verifies all layers,
refuses a non-empty target, and then **replays every record through the real
commit path**, so a fully re-checksummed lie about a root is caught by
recomputation (`RootMismatch`) rather than trusted. The `s7-chaos` binary is
the `kill -9` harness used by the 50-cycle gate.

**S6 pruning + archive.** `aura-state::history` adds `HistoryTree`, an
incrementally maintained compressed sparse Merkle tree whose roots are
bit-for-bit identical to S1 `MerkleTree` (S1 rebuilds the whole arena per
commit: 16.4 ms at 1 024 leaves, measured; the incremental tree costs ~12
hashes per write). `aura-storage::prune` adds `RetentionStore`:
`RetentionPolicy::prune(keep_versions)` keeps a materialised base snapshot
plus the last `keep_versions` version records, `RetentionPolicy::archive()`
keeps all of them, `compact()` folds deltas into the base without moving the
root, and a pruned version returns `PruneError::Pruned` instead of a wrong
state. Version records and base snapshots are canonical, checksummed and
domain separated (`S6_RECORD_CONTEXT`, `S6_BASE_CONTEXT`). Crate edges are
unchanged: `aura-storage` already depends on `aura-state`.

**S5 state sync.** `aura-state::sync` adds a catch-up protocol: a fresh
node downloads one package (3-head quorum + S4 snapshot + compact delta)
instead of the full event history; all blobs are SHA-256 checked and the
recomputed S1 root must match the quorum. A substituted diff chunk is
rejected. Report: `docs/evidence/s5/report.md`.

**S4 snapshots.** `aura-state::snapshot` adds deterministic state snapshots:
SHA-256 chunk digests, a checksummed manifest with descriptors, a BLAKE3
`chunk_root` and an S1 `state_root`. Export/import hash chunks in parallel
with a bounded worker pool; 1,000,000 keys reimport byte-identically. A
substituted chunk is rejected. Report: `docs/evidence/s4/report.md`.

**S3 execution overlay.** `aura-state::overlay` adds `StateView` — a copy-on-write overlay over an immutable S1 tree — and `StateDiff`, a checksummed canonical diff with a strict decoder. `execute()` rolls back the whole view on `OutOfGas` or `Revert`, so a revert cannot change the state root. Report: `docs/evidence/s3/report.md`.

**S2 durable state.** `aura-storage` puts a checksummed WAL in front of a
pluggable `StateBackend`: the default is pure Rust (reproducible in CI), and
the optional `rocksdb` feature uses real RocksDB column families with one
atomic, `sync = true` `WriteBatch`. The WAL record is fsynced BEFORE head
publication, so `kill -9` between the two is recoverable by replay. Report:
`docs/evidence/s2/report.md`.

**S1 state tree.** `aura-state::tree` is a deterministic, versioned sparse
binary Merkle tree over `StateKey::digest` paths, with single-leaf subtree
compression, depth-dependent empty hashes and inclusion proofs bound to a
block height. `state_root = BLAKE3(S1_ROOT_CONTEXT ‖ version ‖ tree_root)`.
It is in memory only — S2 owes persistence. Report:
`docs/evidence/s1/report.md`.

**S0 state baseline.** `aura-state::baseline` is an intentionally naive
in-memory state whose only job is to measure what S1 must fix: no
`state_root`, no persistence, and write order changes balances. It is
explicitly not for consensus code. Report:
`docs/evidence/s0/baseline-report.md`. S1 replaces it with a deterministic
versioned Merkle tree keyed by `StateKey::digest`.

**C10 close-out.** `CORE_PLAN` C0–C10 is complete. Every shipped phase has a
numbered patch, a receipt, an NC log and checksummed evidence;
`tools/audit_closeout.py` runs inside `make test` and fails on any ✅ row
without those artifacts. Format specs are indexed in
[`docs/spec/FORMAT_INDEX.md`](spec/FORMAT_INDEX.md); the measured maturity
assessment is [`docs/CLOSEOUT_AUDIT.md`](CLOSEOUT_AUDIT.md). This closes the
format layer only — STATE S0–S7, EXEC X0–X9 and the consensus/network plans
are still open.

**C9 test rig.** `aura-testkit` (15th crate) owns 512 new golden vectors
(956 across all fixtures), `proptest` properties and a deterministic
portable fuzz harness over six decoders (`codec`, `tx`, `state-key`,
`genesis-json`, `block`, `merkle-proof`). `make fuzz-smoke` runs a measured
60 s campaign with zero tolerated panics; `make test-core` is the CI CORE
shard. `fuzz/` holds four `libfuzzer-sys` source targets sharing the same
`fuzz_one` entry point — H3 runs the 24-hour nightly campaign.
`tools/check_rig.py` fails if any gate is unwired.
Spec: [`docs/spec/test-rig.md`](spec/test-rig.md).

**C8 genesis.** `aura-genesis` (14th crate) parses a strict JSON config into
one canonical binary document. `genesis_hash` =
`hash_domain(chain_id, Domain::Genesis, GENESIS_CONTEXT ‖ canonical_bytes)`;
`validator_set_hash` is now computed (C5 left it caller-supplied);
`allocation_root` is a Merkle commitment over C7 account keys/values using
the C6 tree — **not** the JMT `state_root` (S1). Object key order and
whitespace never change any commitment; validator array order does.
`serde_json` is used only for this config file, never for consensus bytes.
Spec: [`docs/spec/genesis-format.md`](spec/genesis-format.md).

**C7 account model.** `aura-state` now has its first real API:
`Account { nonce, balance, code_hash, storage_root }` (88 canonical bytes) and
`StateKey` with five namespaces — `0x01` account, `0x02` code, `0x03` storage,
`0x04` staking, `0x05` receipt. `0x00` and `0x06`–`0xFF` are reserved and
cannot be decoded into a key; a contract may write only
`0x03 ‖ its_own_address ‖ slot`. `StateKey::digest` is the domain-separated
JMT path for S1. C7 fixes the layout only — no tree, root or persistence yet.
Spec: [`docs/spec/account-model.md`](spec/account-model.md).

**C6 Merkle proofs.** `aura-block::proof` adds compact inclusion proofs over
the unchanged C5 tree: `varint(index) || varint(total) || leaf[32] ||
varint(count) || sibling[32]*count`, with sibling direction derived from the
index (no direction bits). Event leaves use a separate `EVENT_LEAF_CONTEXT`
and a length prefix. `compute_tx_root` and `build_tx_tree` now share one tree
implementation, so roots and proofs cannot disagree. Spec:
[`docs/spec/merkle-proof-format.md`](spec/merkle-proof-format.md).

**C5 block format.** `aura-block` defines `Block = Header || Body` and fixes
the binary BLAKE3 `tx_root` (leaf/node domain tags, odd-node duplication,
empty tree = zero). The body is canonically sorted by `(signer, nonce)` and
`validate()` rejects an unsorted body and a stale `tx_root`. `block_hash` is
the C3 domain-separated hash of the **header only** with `Domain::Block`.
C5 **amends §1**: `chain_id` is added to `Header` (decision D-C7) so block
ids are chain-separated per rule D5; no migration is needed because the
network has produced 0 blocks. Full spec:
[`docs/spec/block-format.md`](spec/block-format.md).

**C4 transaction format.** `aura-tx` defines
`Tx = UnsignedTx || Signature`, where `UnsignedTx` is
`codec_version, chain_id, signer, nonce, kind, payload, gas_limit, max_fee,
tip, expiry` in that exact order, and `Signature` is an envelope
`{ algo, public_key[32], bytes[64] }`. `signing_bytes` =
`SIGNING_CONTEXT || canonical(UnsignedTx)`; the signature is Ed25519 over the
C3 domain-separated BLAKE3 digest. `signer` is bound to the public key via
`BLAKE3(ADDRESS_CONTEXT || public_key)[0..20]`. `validate(expected_chain,
height)` rejects replay across chain-ids. Full byte-level spec:
[`docs/spec/tx-format.md`](spec/tx-format.md).

**C2 wire format (one value → one byte string).** `aura-codec` uses fixed-width
little-endian integers (no `usize`/`isize`), minimal LEB128 length prefixes,
strict tags for `bool`/`Option`/`Domain`/`Denom`, ascending `BTreeMap` entries,
`CODEC_VERSION = 1`, and `decode_canonical()` which re-encodes and requires
byte equality. `serde` is not a dependency. Golden vectors:
`tools/gen_c2_vectors.py`.

## 2. Transaction flow (critical path)

```
1. wallet:  TxSign(canonical_bytes, domain="tx", chain_id) -> signature
2. rpc:     submit_tx(raw) -> size/decode/signature/nonce/fee checks
3. mempool: validate against state view, priority by effective_tip
4. consensus: proposer builds Block, validators vote -> QC (2f+1)
5. exec:    for each Tx: gas_start -> WASM/host ABI -> state overlay
6. state:   overlay commit -> new_state_root -> receipts_root -> events
7. storage: atomic write of block + state diff + WAL fsync
8. p2p:     broadcast block, vote, evidence
9. indexer: read canonical chain -> Postgres -> explorer
```

Invariant of step 5: **no host call has access to a nondeterministic source**.
Allowed context: `block.height`, `block.timestamp`, `tx.signer`, `tx.hash`,
`chain_id`, `contract.address`, `gas_remaining`. Forbidden: time, randomness,
files, network, env, float undefinedness.

## 3. Crate dependency graph (acyclic)

```
aura-codec  <- aura-types
aura-crypto <- aura-types
aura-tx     <- aura-types, aura-codec, aura-crypto   (C4, 12th crate)
aura-block  <- aura-tx, aura-codec, aura-crypto, aura-types  (C5, 13th crate)
aura-state  <- aura-types, aura-codec, aura-crypto  (C7 + S0/S1/S3/S5/S6)
aura-storage <- aura-types, aura-codec, aura-crypto, aura-state  (S2 + S6 + S7, RocksDB optional)
aura-genesis <- aura-state, aura-block, aura-crypto, aura-codec, aura-types  (C8, 14th crate)
aura-exec   <- aura-types, aura-crypto, aura-state, wasmi  (X1 engine; X2 state/ABI;
               wasmtime behind feature `wasmtime-engine`)
aura-sdk    <- aura-sdk-macros only  (X4, 16th crate: `no_std` CONTRACT side of
               ABI v1; never linked by the node)
aura-sdk-macros <- syn, quote, proc-macro2  (X4, 17th crate: the three attributes)
examples/{store,kv,events} <- aura-sdk  (X4 reference contracts: separate
               workspaces, built to wasm32-unknown-unknown, pinned by hash)
standards/{arc20,arc721,multisig} <- aura-sdk  (X5 reference STANDARDS: separate
               workspaces, dispatched by 4-byte selector over ABI v2, pinned by hash)
aura-testkit <- all of the above (C9, 15th crate, dev/test only)
aura-consensus <- aura-state, aura-crypto, aura-p2p
aura-node(aurad) <- all of the above + aura-rpc + aura-mempool
aura-wallet / indexer / explorer / bridge <- aura-types + aura-rpc client
```

`standards/` is the same shape one level up: a standard is a contract with a
published method table, so it lives beside the examples but is depended on by
nothing in the node — the node only ever sees its bytecode.

The SDK is the one edge that points *outward*: a contract depends on `aura-sdk`,
and `aura-sdk` depends on nothing in the node. That is why `examples/` is a
separate workspace and why `aura-exec` only ever sees the SDK's *output* (a wasm
module), never its code.

Rule: a lower layer **does not know** about a higher one. `aura-types` does not
depend on the VM, `aura-crypto` does not know about consensus. Violation =
review error.

## 4. Cryptography

| Purpose | Primitive | Note |
|---|---|---|
| Block/state hash | BLAKE3 (256 bit) | fast, deterministic |
| Tx hash / address | BLAKE3 + domain tag | domain separation (D5) |
| Tx signatures | Ed25519 (primary), secp256k1 (compat) | canonical checks, malleability banned |
| Vote aggregation | BLS12-381 (QC) | 2f+1 signatures → one |
| Leader election | VRF (ECVRF-compatible) | protects against stake grinding |
| P2P encryption | Noise + QUIC/TCP (libp2p) | identity = pubkey |
| PQ horizon | ML-DSA (FIPS 204) / SLH-DSA (FIPS 205) | versioned `sig_algo` in address |

**C3 status.** BLAKE3 (pure Rust), Ed25519, secp256k1/RFC6979, constant-time
comparison and zeroizing secrets are implemented in `crates/aura-crypto`.
Domain separation is
`HASH_CONTEXT || u64(len(prefix)) || prefix || u64(len(payload)) || payload`,
where `prefix = chain_id || 0x00 || domain_tag || 0x00` (C1). BLS12-381 QC
aggregation (B2) and the PQ migration (H2) are still planned, not implemented.

All signatures are verified with `domain = chain_id || purpose || version`.
Keys are zeroize-on-drop; comparisons are constant-time.

## 5. Consensus (BFT-PoS, briefly)

- Validators: set `V`, |V| = 3f+1 (target), stake `stake(v)`.
- Round: `proposer(VRF)` proposes a block → `PREVOTE` → `PRECOMMIT` →
  QC of 2f+1 BLS signatures → commit (HotStuff-like 3-chain pipeline).
- Finality: **instant** after QC; a reorg deeper than a finalized block is
  impossible without slashable evidence (B8).
- View change: timeout with exponential backoff, new proposer, QC on an empty
  (nil) block allowed.
- Slashing: double-sign, surround-vote, extended downtime → evidence in block,
  penalty in T5/B6.
- Weak subjectivity: long-range attack mitigated by checkpoints and the
  `unbonding` period (T4/B10).

## 6. Storage

```
RocksDB:
  "b/<height>"        -> Block (aura-codec)
  "h/<hash>"          -> height (index)
  "s/<version>/<key>" -> JMT node
  "snap/<root>"       -> snapshot manifest (chunks + sha256)
  "meta"              -> head, finalized, protocol_version
WAL: sequence of (height, state_diff, receipts), fsync before commit.
```

Recovery after `kill -9` at any step is the S7 gate.

## 7. Application layers

| Layer | Standard | Note |
|---|---|---|
| Fungible token | ARC-20 | reference contract, not the native asset |
| NFT / collections | ARC-721 / ARC-1155 | metadata + content hash |
| Names | `.aura` name service | on-chain resolver |
| AMM | constant product + concentrated liquidity (later) | E5 |
| Stablecoin | over-collateralized CDP | E6, depends on oracles |
| Lending | collateral + liquidation engine | E7 |
| Governance | on-chain proposal + timelock | T6/B9 |
| Bridge | light-client IBC-like | E9, highest-risk component |

## 8. Non-goals (what we deliberately do not build)

- Our own browser/OS as a mandatory part (AuraLite OS is the optional track E9).
- Zcash-level anonymity at launch (ZK is a separate plan after M4).
- PoW mining and "green" marketing instead of measurements.
- Promises of returns, listings or a token price.
- A universal "super-VM": first one deterministic WASM ABI, compatibility later
  (X9).
