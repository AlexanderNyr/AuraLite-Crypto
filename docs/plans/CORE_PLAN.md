# CORE_PLAN — primitives, codec, cryptography, formats

**Status:** ✅ COMPLETE (C0–C10 done ✅ 2026-10-26)  
**Stage:** Stage 1: C0–C10 (all phases active).  
**Deliverable:** `patches/0001-C0-...` … `patches/0011-C10-...`
**Dependencies:** none. This is the root of the whole tree.

## Problem, stated once

Without canonical types and a deterministic codec you cannot compute a
`state_root`, verify a signature or agree on a block. The typical mistake —
"write the node first" — changes the tx format ten times and makes every test
unreproducible. C0–C10 fix a **small, verifiable foundation** on which the
other 11 plans rest.

## Decisions

- **D-C1.** Address = `bech32("aura", version || pubkey_hash)` with checksum;
  the high version byte is reserved for the PQ algorithm (see D7 in ROADMAP).
- **D-C2.** `Amount` is `u128` in smallest units (18 decimals), checked
  arithmetic only; `as` casts and unsigned overflow banned.
- **D-C3.** `aura-codec` is our own format with golden vectors; it rejects
  non-canonical encodings. `serde` is for RPC/configs only.
- **D-C4.** Hash is BLAKE3 with domain tags; `Hash` is a newtype over `[u8; 32]`.
- **D-C5.** All signatures: `sign(domain || chain_id || canonical_bytes)`.
- **D-C6 (C4).** The transaction format lives in a dedicated 12th crate,
  `aura-tx`, because it needs **both** `aura-codec` and `aura-crypto` without
  inverting the layering (a lower layer must not know a higher one). C4
  updates the crate graph in `ARCHITECTURE.md` §3 and all crate counts.
- **D-C7 (C5).** Block/header types live in a dedicated 13th crate,
  `aura-block`, below both state and consensus. `chain_id` is added to the
  `Header` (amending `ARCHITECTURE.md` §1): rule D5 requires chain separation
  for block ids, and with 0 blocks produced no migration is needed.
  `state_root`, `validator_set_hash` and `qc` are caller-supplied until
  S1/B phases exist; C5 fixes their bytes, not their computation.
- **D-C8 (C6).** Merkle proofs live in `aura-block::proof`, not a new crate,
  and reuse the C5 tree **without changing `tx_root`**. Event leaves get a
  separate context and a length prefix. `compute_tx_root` and `build_tx_tree`
  share one implementation so a root and a proof cannot disagree about
  padding. Proof direction is derived from the index — no direction bits.
- **D-C9 (C7).** The account model and state key layout live in the existing
  `aura-state` crate (no new crate at C7). Exactly five key prefixes are valid;
  `0x00` and `0x06`–`0xFF` stay reserved so contract code can never reach
  protocol space. Each namespace has one fixed key length. C7 is layout
  only — the versioned tree, `state_root` and persistence remain S1–S7.
- **D-C10 (C8).** Genesis is a **config**, not consensus bytes: `serde_json`
  parses a strict JSON schema, while every hash is computed over C2 binary.
  Validator array order is consensus-significant (NC); allocation order is
  not, because state is a set. `serde_json` is the first new external
  dependency since C3 and is recorded in a reviewed C8 addendum — the frozen
  C3 baseline allowlist stays untouched. Stage 1 rejects `auralite-1`.
- **D-C11 (C9).** The test rig is **wired, not decorative**:
  `tools/check_rig.py` and `tools/count_vectors.py` run inside `make test`,
  `make test-core` is the CI CORE shard, and `make fuzz-smoke` is a measured
  60 s deterministic campaign. NCs inject real production bugs rather than
  deleting files. `proptest` is a dev dependency only; `fuzz/` is a separate
  workspace so `libfuzzer-sys` never enters the production `Cargo.lock`.
  H3 still owes sanitizers and the 24-hour libFuzzer campaign.
- **D-C12 (C10).** A phase is done only if a machine can prove it.
  `tools/audit_closeout.py` runs inside `make test` and requires, for every
  ✅ row, a numbered patch, a `### Cx receipt`, a CHANGELOG entry, an NC
  driver/log and evidence with verified SHA256 checksums. It also checks the
  NC total, indexes all `docs/spec/` files and rejects ✅ marks in deferred
  plans. `docs/CLOSEOUT_AUDIT.md` records what is real vs planned.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **C0** | Baseline + workspace + CI | `Cargo.toml` workspace, `Makefile`, `rust-toolchain.toml`, `.github/workflows/ci.yml`, `docs/evidence/c0-baseline.txt` with measured zeros; constants `CHAIN_ID="auralite-test-1"`, `DENOM="ALCRY"`, `DECIMALS=18` fixed in code and checked by a test | `make test` green (0 tests so far, but the pipeline is real); NC: broken CI → red |
| ✅ **C1** | Canonical types | `Hash`, `Address` (bech32 `aura1...`), `Amount`/`Denom` (`ALCRY` / `aalcry`), `ChainId`, `Nonce`, `Height`, `Gas`, `Domain`; `BTreeMap` instead of `HashMap` | unit vectors: bech32 round-trip, bad checksum rejected, checked `Amount` overflow |
| ✅ **C2** | `aura-codec` | fixed field order, lengths, `Option` tags, non-canonical encodings banned, `codec_version` | golden vectors (hex fixtures), property: `decode(encode(x)) == x`, NC: one mutated byte → `Err` |
| ✅ **C3** | `aura-crypto` | BLAKE3 + domain tags, Ed25519, secp256k1, constant-time compare, zeroize | RFC/test-vector signatures, malleable `s` rejected, NC: swapped domain → `Err` |
| ✅ **C4** | Transaction format | `Tx { chain_id, signer, nonce, kind, payload, gas_limit, max_fee, tip, expiry, sig }`, canonical `signing_bytes` | sign/verify, replay test: a tx from `auralite-1` rejected in `auralite-test-1` |
| ✅ **C5** | Block and header | `Header`/`Body`, `block_hash`, `tx_root`, `state_root`, `validator_set_hash`, protocol version | vector: changing tx order changes `tx_root`; NC: reorder without recompute → red |
| ✅ **C6** | Merkle proofs | build/verify proofs for txs and events, compact format | 1000 leaves, all proofs valid, one substituted — invalid |
| ✅ **C7** | Account model | `Account { nonce, balance, code_hash, storage_root }`, state key layout, reserved prefixes | gate: keys of two accounts do not collide, reserved space unreachable from contracts |
| ✅ **C8** | Genesis | genesis JSON → binary genesis hash, validator set, allocation, chain-id, `aura genesis` tool | same genesis → same hash on 3 hosts; NC: validator order change → different hash |
| ✅ **C9** | Test rig | `proptest`, golden fixtures, `cargo-fuzz` targets for codec/tx, `make test-core`, CI shard `core` | >= 200 vectors, 60 s fuzzing with no crashes, `clippy -D warnings` |
| ✅ **C10** | Close-out | all NCs collected, docs + STATUS + CHANGELOG, format specs in `docs/spec/` | full `make test`; plan↔tree audit (not a single ✅ line without a receipt) |


### C0 receipt (2026-10-25)

- Deliverable: `patches/0001-C0-baseline-workspace.patch`.
- Landed: 11-crate workspace with zero dependencies, `Cargo.lock`,
  `rust-toolchain.toml` (1.99.1), `Makefile`, `.github/workflows/ci.yml`,
  `aura-types` identity constants, `tools/check_claims.py`, `tools/nc_c0.py`.
- Gate: `make test` = `cargo fmt --check` + `cargo clippy --workspace
  --all-targets -- -D warnings` + `cargo test --workspace` (**5 passed,
  0 failed**) + docs drift guard (**12 plans / 112 phase rows / all links OK**).
- NC: `make nc-c0` → **2/2 PASS** (`docs/evidence/c0/nc.log`).
- Defect found by the gate itself: `check_claims.py` initially lacked
  `re.MULTILINE` and reported all 112 rows missing; fixed in this patch, and
  NC-2 now pins that a single removed row turns the guard red.
- Raw numbers: `docs/evidence/c0-baseline.txt`, `docs/evidence/SHA256SUMS`.
- `make devnet` refuses with rc=1 (D7 not implemented); sanitizer,
  determinism and fuzz gates are explicit SKIPs until H3/C9/X7.
- Toolchain note: verified with `RUSTUP_TOOLCHAIN=stable` (rustc 1.99.1) because
  the sandbox had that toolchain installed; CI installs the pinned 1.99.1
  channel from `rust-toolchain.toml` with rustfmt/clippy.


### C1 receipt (2026-10-25)

- Deliverable: `patches/0002-C1-types.patch`.
- Landed: `Hash` (32-byte newtype, hex/slice decode), `Address` (20 bytes,
  in-tree classic BIP-173 bech32), `Amount`/`Denom` (checked `u128`),
  `ChainId` (validated), `Nonce`/`Height`/`Gas` (checked `u64`), `Domain`
  (5 tags + `chain-id || 0 || tag || 0` prefix), `CanonicalMap` (`BTreeMap`).
  Still zero external dependencies.
- Golden vectors: `tools/gen_c1_vectors.py` is an **independent** Python
  BIP-173 encoder; `crates/aura-types/tests/fixtures/c1_golden.txt` has 23
  lines (4 hashes, 4 addresses, 2 bad checksums, wrong prefix, uppercase,
  amount bounds). Re-running the generator is byte-identical (sha256 gate).
- Gate: `make test` → **17 passed / 0 failed** (5 unit + 11 C1 vector + 1
  identity), fmt + clippy clean, drift guard 12 plans / 112 rows.
- NC: `make nc-c1` → **3/3 PASS**: checksum constant 1→0, address version
  0x00→0x01, corrupted golden vector. All three require red, then restore.
- Evidence: `docs/evidence/c1/gates.txt`, `docs/evidence/c1/nc.log`,
  `docs/evidence/c1/SHA256SUMS`.
- Defects found while building C1 (fixed in the same patch): a real compile
  error in a vector test (`&String + &str`), and rustfmt drift across four
  files — both caught by the C0 gate before the patch was cut.
- Deliberately deferred: bech32m (a versioned decision, not a C1 tweak),
  serialization (C2), hashing/signatures (C3).


### C2 receipt (2026-10-25)

- Deliverable: `patches/0003-C2-codec.patch`.
- Landed: `aura-codec` with `Encode`/`Decode`, `Writer`/`Reader`, minimal
  LEB128 varints, fixed-width LE integers (no `usize`/`isize`), strict tags
  for `bool`/`Option`/`Domain`/`Denom`, length caps `MAX_LENGTH = 1 MiB` and
  `MAX_ITEMS = 65536`, ascending `BTreeMap` decoding, `CODEC_VERSION = 1`,
  and `decode_canonical()` (re-encode must equal input).
- Golden vectors: `tools/gen_c2_vectors.py` is an independent Python
  implementation; `crates/aura-codec/tests/fixtures/c2_golden.txt` has 21
  lines — 3 canonical records, 5 constants, 12 non-canonical vectors.
- Gate: `make test` → **28 passed / 0 failed** (17 C0/C1 + 11 C2), fmt/clippy
  clean, drift guard 12 plans / 112 rows.
- NC: `make nc-c2` → **4/4 PASS**: codec version 1→2, corrupted golden
  fixture, disabled trailing-byte guard, disabled non-minimal-varint guard.
- Evidence: `docs/evidence/c2/gates.txt`, `docs/evidence/c2/nc.log`,
  `docs/evidence/c2/SHA256SUMS`.
- Still 0 external dependencies; one internal path dependency
  `aura-codec -> aura-types`.
- Deferred: hashing/signatures (C3), tx/block formats (C4/C5). Any wire
  format change requires a `CODEC_VERSION` bump + migration test.


### C3 receipt (2026-10-25)

- Deliverable: `patches/0004-C3-crypto.patch`.
- Landed: `aura-crypto` with BLAKE3 (`pure`), Ed25519 (RFC 8032), secp256k1
  deterministic ECDSA (RFC 6979, low-S), `HASH_CONTEXT`-framed domain
  separation (`chain-id || 0 || tag || 0`, explicit u64 lengths),
  constant-time equality and `SecretBytes` (zeroize-on-drop, redacted Debug).
  No randomness in the API; all signing is deterministic from a seed.
- Golden vectors: `tools/gen_c3_vectors.py` — official BLAKE3 vectors (15),
  a self-checking pure-Python BLAKE3 for 5 domain hashes, RFC 8032 §7.1
  TEST 1/2/3, and pure-Python secp256k1/RFC6979 (2 cases, verified with
  OpenSSL `pkeyutl -verify`). 66 fixture lines, generator byte-stable.
- Gate: `make test` → **39 passed / 0 failed** (28 C0–C2 + 11 C3), fmt/clippy
  clean, drift guard 12 plans / 112 rows, dependency gate 54 external crates.
- NC: `make nc-c3` → **5/5 PASS**: HASH_CONTEXT change, two corrupted golden
  vectors, Ed25519 verification bypass, dependency allowlist drift (proves
  `make test` really runs `check_deps.py`).
- Evidence: `docs/evidence/c3/gates.txt`, `docs/evidence/c3/nc.log`,
  `docs/evidence/c3/SHA256SUMS`; `docs/spec/c3-dependency-allowlist.txt`.
- Dependencies: exact pins `blake3 =1.8.7`, `ed25519-dalek =2.2.0`,
  `k256 =0.13.4`, `zeroize =1.8.2`, `subtle =2.6.1`. `blake3` uses `pure`
  (portable Rust); `cc` remains an upstream build-dependency but compiles no
  native hash code. `cargo deny` (licences/advisories) is owed to H1.
- Deferred: BLS12-381 QC aggregation (B2), PQ migration to ML-DSA/SLH-DSA
  (H2), tx/block formats (C4/C5).


### C4 receipt (2026-10-25)

- Deliverable: `patches/0005-C4-tx-format.patch`.
- Landed: the 12th crate `aura-tx` (no new external dependencies). `Tx =
  UnsignedTx || Signature`; `UnsignedTx` has the ten C4 fields plus
  `codec_version`; `Signature` is an envelope `{ algo, public_key[32],
  bytes[64] }` because Ed25519 cannot recover a public key. `TxParams`
  groups the nine constructor fields (fixes `clippy::too_many_arguments`
  without suppressing the lint).
- Canonical `signing_bytes` = `SIGNING_CONTEXT || canonical(UnsignedTx)`,
  signed via C3's domain-separated `aura_crypto::sign(Domain::Tx, chain_id)`.
  `signer` is bound to the key by
  `BLAKE3(ADDRESS_CONTEXT || public_key)[0..20]`. `txid` =
  `BLAKE3(TXID_CONTEXT || canonical(Tx))`. Replay protection:
  `validate(expected_chain, height)` rejects a tx minted for `auralite-1`
  on `auralite-test-1` (DoD gate), and rewriting the chain-id after signing
  breaks the signature.
- Validation order: version, chain, payload ≤ 65536, gas ≠ 0, tip ≤ max_fee,
  `height ≤ expiry`, signature. `height`/`expiry` are consensus heights, not
  a local clock. Nonce/state checks remain C7/mempool.
- Spec: `docs/spec/tx-format.md` (field table, wire layout, tags, signing,
  txid, validation, errors, non-goals).
- Golden vectors: `tools/gen_c4_vectors.py` — independent Python containing
  pure-Python BLAKE3 (self-checked against 7 official vectors), pure-Python
  Ed25519 (self-checked against RFC 8032 TEST 1/2/3) and a C2 wire encoder.
  104 fixture lines: 5 canonical transactions (incl. u64/u128 maxima, a
  mainnet-chain tx) and 9 rejection vectors, each pinned to a specific error.
  Generator is offline and byte-stable.
- Additive C2 change: `CodecError::InvalidEnumTag(&'static str, u8)` for
  caller-defined enums (`TxKind`, `SignatureAlgo`). No C2 wire-format change;
  C2 golden vectors and checksums are untouched.
- Gate: `make test` → **49 passed / 0 failed** (39 C0–C3 + 10 C4), fmt/clippy
  clean, drift guard 12 plans / 112 rows / 20 markdown files, dependency
  gate 54 external crates allowlisted.
- NC: `make nc-c4` → **6/6 PASS**: signing context, txid context, golden
  signature, replay protection, signer↔key binding, Ed25519 verification.
  Total across C0–C4: **20 NC** via `make nc-all`.
- Evidence: `docs/evidence/c4/gates.txt`, `docs/evidence/c4/nc.log`,
  `docs/evidence/c4/SHA256SUMS` (also pins the golden fixture and the spec).
- CI: `make nc-all` runs every NC without rewriting historical C0–C3 logs;
  only the current phase's evidence is regenerated. This keeps the committed
  `SHA256SUMS` of past phases valid.
- Deferred: block/header and `tx_root` (C5), Merkle proofs (C6), accounts
  (C7), genesis (C8), fuzzing (C9), secp256k1 in tx signatures, PQ (H2).


### C5 receipt (2026-10-25)

- Deliverable: `patches/0006-C5-block-header.patch`.
- Landed: the 13th crate `aura-block` (no new external dependencies).
  `Block = Header || Body`; `Header` has 12 fields including the new
  `chain_id` (D-C7); `HeaderParams` groups the 11 caller-supplied fields and
  deliberately omits `tx_root`, which is always derived from the body.
  `Body::new` sorts by `(signer, nonce)` and rejects duplicate keys.
- `tx_root`: binary BLAKE3 Merkle tree over C4 txids with separate
  `LEAF_CONTEXT`/`NODE_CONTEXT`, odd-node duplication, empty tree = zero.
  Order-sensitive — the C5 DoD vector `tx_root_forward != tx_root_backward`.
  Tree shape is fixed now so C6 can add proofs without changing the root.
- `block_hash` = C3 `hash_domain(chain_id, Domain::Block,
  BLOCK_HASH_PAYLOAD_CONTEXT || canonical(Header))`. Header only; `tx_root`
  commits to the body.
- `validate(expected_chain)` checks version, chain, sizes, canonical order,
  duplicates and then `tx_root`; `validate_transactions()` separately checks
  each tx signature/expiry at `header.height`. `timestamp_ms` is caller
  supplied — no `SystemTime::now()`.
- Spec: `docs/spec/block-format.md` (field table, tree, hash, validation,
  non-goals). `ARCHITECTURE.md` §1/§3 amended in the same patch.
- Golden vectors: `tools/gen_c5_vectors.py` — independent Python with
  pure-Python BLAKE3 (self-checked against 7 official vectors) reusing the
  committed C4 tx vectors as bodies. 95 fixture lines: **4 canonical blocks**
  (2/3/0/1 tx, real `b1 → b2` parent link, `Some(qc)`, u64/u128 maxima), an
  order-sensitivity pair and **9 rejection vectors**, each pinned to a
  specific error. Generator is offline and byte-stable.
- Gate: `make test` → **60 passed / 0 failed** (49 C0–C4 + 11 C5),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 21 markdown files,
  dependency gate 54 external crates allowlisted.
- NC: `make nc-c5` → **6/6 PASS**: leaf context, node context, block-hash
  context, golden `tx_root`, `tx_root` check, canonical-order check. Total
  across C0–C5: **26 NC** via `make nc-all`.
- Evidence: `docs/evidence/c5/gates.txt`, `docs/evidence/c5/nc.log`,
  `docs/evidence/c5/SHA256SUMS` (also pins the golden fixture and the spec).
- Fixes found by the gate itself: clippy flagged an unused import, a
  collapsible `if`, `chunks_exact(2)` and a cloned single-ref slice. All
  fixed in code (`as_chunks::<2>()`, `std::slice::from_ref`), not suppressed.
- Deferred: Merkle proofs (C6), accounts (C7), genesis (C8), fuzzing (C9),
  QC/validator-set/state computations (B/S phases), BLS (B2), PQ (H2).


### C6 receipt (2026-10-25)

- Deliverable: `patches/0007-C6-merkle-proofs.patch`. No new crate and no new
  external dependency: `crates/aura-block/src/proof.rs` + `tests/c6_merkle.rs`.
- Landed: `MerkleTree` (padded levels), `MerkleProof` (compact wire format),
  `build_tx_tree`/`build_event_tree`/`build_tree_from_leaf_hashes`, `prove`,
  `tree_depth`, `verify_proof`, `verify_tx_inclusion`,
  `verify_event_inclusion`, `event_leaf_hash`, and `MerkleError`.
- **Compact proof:** `varint(index) || varint(total) || leaf[32] ||
  varint(sibling_count) || sibling[32]*count`. Sibling direction is derived
  from `index >> level` (no direction byte). `MAX_PROOF_DEPTH = 64`; minimal
  varints and `decode_canonical()` enforce canonicality. For 1000 leaves a
  proof is ~358 bytes (depth 10).
- **C5 tree unchanged.** `compute_tx_root` now delegates to `build_tx_tree`,
  so root and proof share one shape. A regression test compares the C5 root
  of four real C4 transactions against the C6 tree root. Event leaves use
  `EVENT_LEAF_CONTEXT` plus `u64(len)` so a tx leaf cannot be replayed as an
  event leaf.
- Spec: `docs/spec/merkle-proof-format.md` (tree, wire format, verification
  order, vectors, non-goals).
- Golden vectors: `tools/gen_c6_vectors.py` — independent Python with
  pure-Python BLAKE3 (self-checked against 7 official vectors). 69 fixture
  lines: roots/depths for **1/2/3/5/8/1000** tx leaves, roots for 1/3/5 event
  leaves, **34 canonical proofs** and **11 rejection vectors** (substituted
  leaf, wrong sibling/index/total, truncated siblings, empty total, OOB
  index, depth overflow, trailing byte, truncation, non-minimal varint).
  Generator is offline and byte-stable. The C5 and C6 generators agree on the
  shared LEAF/NODE contexts — pinned by a test.
- **DoD gate:** 1000 leaves, all 1000 proofs generated and verified, one
  substituted leaf rejected (both by leaf binding and by root mismatch).
- Gate: `make test` → **71 passed / 0 failed** (60 C0–C5 + 11 C6),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 22 markdown files,
  dependency gate 54 external crates allowlisted.
- NC: `make nc-c6` → **6/6 PASS**: event-leaf context, golden root, leaf
  binding, sibling-count check, sibling direction, root comparison. Total
  across C0–C6: **32 NC** via `make nc-all`.
- Evidence: `docs/evidence/c6/gates.txt`, `docs/evidence/c6/nc.log`,
  `docs/evidence/c6/SHA256SUMS` (also pins the golden fixture and the spec).
- Fixes found by the gate itself: clippy required `div_ceil`,
  `is_multiple_of` and an iterator instead of an index loop — all fixed in
  code, not suppressed.
- Deferred: accounts and state key layout (C7), genesis (C8), test rig and
  fuzzing (C9), batch/multiproofs, sparse state trees (S1), RPC proof serving
  (N/D phases).


### C7 receipt (2026-10-26)

- Deliverable: `patches/0008-C7-account-model.patch`. No new crate and no new
  external dependency — `aura-state` gets its **first real API** (it was an
  honest empty placeholder since C0).
- Landed: `Account { nonce, balance, code_hash, storage_root }` (88 canonical
  bytes, checked `u128`/`u64` arithmetic, `NO_CODE`/`EMPTY_STORAGE_ROOT`
  sentinels), `KeySpace` (5 tags), `StateKey` (validated raw + varint wire
  format), `StateKey::digest` (domain-separated JMT path for S1),
  `assert_contract_may_write` and `StateError`.
- **Key layout:** `0x01` account (21), `0x02` code (21), `0x03` storage (53),
  `0x04` staking (21), `0x05` receipt (33). `0x00` and `0x06`–`0xFF` are
  **reserved** and rejected by `from_raw`/`from_bytes`. Known prefix + exact
  namespace length only — this is what makes account keys collision-free and
  stops cross-namespace aliasing.
- **Contract isolation:** a contract may write only
  `0x03 || its_own_address || slot[32]`; other namespaces give
  `ForbiddenContractWrite`, another contract's storage gives
  `WrongContractStorage`. Reserved prefixes cannot even be constructed.
- Spec: `docs/spec/account-model.md` (namespaces, reserved space, isolation,
  digest, account wire format, arithmetic, non-goals).
- Golden vectors: `tools/gen_c7_vectors.py` — independent Python with
  pure-Python BLAKE3 (self-checked against 7 official vectors). 107 fixture
  lines: raw/encoded keys and digests for 5 namespaces × 4 indices (generator
  asserts all 20 digests differ), 3 account encodings, contract-isolation
  operands and **12 rejection vectors**. Generator is offline and byte-stable.
- **DoD gates:** 1000 distinct addresses → 5000 keys/digests, zero collisions;
  all 256 prefixes iterated with exactly 5 reachable; contract write
  isolation checked both ways.
- Gate: `make test` → **82 passed / 0 failed** (71 C0–C6 + 11 C7),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 23 markdown files,
  dependency gate 54 external crates allowlisted.
- NC: `make nc-c7` → **6/6 PASS**: state-key context, golden digest, reserved
  prefix rejection, namespace length, contract namespace, contract address.
  Total across C0–C7: **38 NC** via `make nc-all`.
- Evidence: `docs/evidence/c7/gates.txt`, `docs/evidence/c7/nc.log`,
  `docs/evidence/c7/SHA256SUMS` (also pins the golden fixture and the spec).
- Defects found by the gate itself: the first deterministic address pattern
  had a period of 256 and **collided at index 256** — replaced with a pattern
  whose first 8 bytes are the index. Two wrong assumptions in the test
  (account length 76 vs 88, storage-key length 21 vs 53) were also corrected.
  Fixed in code, not papered over.
- Deferred: versioned state tree and `state_root` (S1), persistence (S2),
  overlays (S3), genesis allocation (C8), test rig (C9), VM host ABI (X
  phases).


### C8 receipt (2026-10-26)

- Deliverable: `patches/0009-C8-genesis.patch`. Adds the **14th crate**,
  `aura-genesis`, and the `aura` binary with the `genesis` subcommand
  (`hash` / `verify` / `show`). First new external dependency since C3:
  `serde_json =1.0.154` (config parsing only, no serde derive, no JSON in
  the consensus path).
- Landed: `Genesis { chain_id, protocol_version, genesis_time_ms,
  validators, allocations }`, `Validator`, `Allocation`, `GenesisError`, a
  strict JSON parser and C2 `Encode`/`Decode`. Rejects unknown/missing keys,
  wrong types, float balances, invalid bech32/hex, zero power, zero
  allocation, duplicate addresses, empty validator set and supply overflow.
- Commitments: `genesis_hash` =
  `hash_domain(chain_id, Domain::Genesis, GENESIS_CONTEXT ‖ canonical_bytes)`;
  `validator_set_hash` = `BLAKE3(VALIDATOR_SET_CONTEXT ‖ validator_vector)`
  (now actually computed — C5 left it caller-supplied); `allocation_root` =
  C6 Merkle root over C7 `StateKey::account` digests and `Account` values,
  sorted by address. Not the JMT `state_root` (S1).
- Determinism: object key order and whitespace are irrelevant; validator
  array order **is** consensus-significant; allocation array order does not
  change `allocation_root`. `genesis_time_ms` is explicit, never a clock.
- Spec: `docs/spec/genesis-format.md`; sample config
  `docs/spec/genesis-testnet.json` (its hash is pinned by a test).
- Golden vectors: `tools/gen_c8_vectors.py` — independent Python with
  pure-Python BLAKE3 (self-checked against 7 official vectors), an
  independent BIP-173 bech32 encoder, C3 domain framing, C5/C6 tree rules
  and C7 key/account encodings. 44 fixture lines: 19 JSON documents
  (3 equivalent, 2 order variants, 13 invalid) plus 2 full commitment sets.
  Generator self-checks all determinism properties; offline, byte-stable.
- DoD gates: three independent JSON representations produce one hash and one
  canonical byte string; reordering validators changes both `genesis_hash`
  and `validator_set_hash`. Four CLI integration tests pin `hash`, `verify`
  (accept + reject), `show` and usage.
- Gate: `make test` → **96 passed / 0 failed** (82 C0–C7 + 14 C8),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 24 markdown files,
  dependency gate 58 external crates allowlisted.
- Dependency governance: the C3 baseline allowlist is frozen historical
  evidence (pinned by `docs/evidence/c3/SHA256SUMS`). C8 records its +4
  delta (`serde_json`, `itoa`, `memchr`, `zmij`) in
  `docs/spec/c8-dependency-addendum.txt`; `check_deps.py` compares the lock
  against baseline + addenda. `tools/record_c8_deps.py` is the reviewed
  re-record action.
- NC: `make nc-c8` → **6/6 PASS**: genesis context, validator-set context,
  golden hash, validator-order sorting, unknown-field rejection, Stage 1
  chain-id check. Total across C0–C8: **44 NC** via `make nc-all`.
- Evidence: `docs/evidence/c8/gates.txt`, `docs/evidence/c8/nc.log`,
  `docs/evidence/c8/SHA256SUMS` (also pins the fixture, spec, sample config
  and dependency addendum).
- Fixes found by the gate itself: a CLI test used the wrong relative path
  (`CARGO_MANIFEST_DIR` is the crate dir, not the test file dir); a JSON
  string helper needed an explicit lifetime; an unused import was removed.
  All fixed in code, not suppressed.
- Deferred: genesis **block** and execution (B/D phases), JMT `state_root`
  (S1), validator key ceremony (W7/O5), BLS aggregation (B2), Stage 2
  mainnet genesis.


### C9 receipt (2026-10-26)

- Deliverable: `patches/0010-C9-test-rig.patch`. Adds the **15th crate**,
  `aura-testkit`, and `proptest =1.12.0` as a **dev dependency**
  (`default-features = false, features = ["std"]`, +13 transitive crates,
  recorded in `docs/spec/c9-dependency-addendum.txt`).
- Landed: 512 new golden vectors (`c9_vectors.txt`) — 128 valid varints,
  128 non-minimal/overflow varints, 128 trailing-byte vectors, 128 distinct
  truncated vectors — from `tools/gen_c9_vectors.py` (fixed-seed Python LCG,
  self-checked). **956 vectors** across 9 fixtures via
  `tools/count_vectors.py` (`MIN_VECTORS = 200`).
- Landed: 5 `proptest` properties (256 cases each) covering varint
  minimality, trailing bytes, `CanonicalRecord` round-trips, decoder
  totality and C7 key-digest purity; 3 harness tests; 5 vector tests.
- Landed: portable deterministic fuzz harness `fuzz_one` over six decoders
  (`codec`, `tx`, `state-key`, `genesis-json`, `block`, `merkle-proof`) with
  `catch_unwind`, a fixed xorshift64* PRNG and separate `work`/`iterations`
  counters. `aura-fuzz --seconds 60 --target all` is the DoD gate (60 s is
  the TOTAL budget, split across targets); `make test-core` uses a short
  iteration-bounded pass.
- Landed: four C9 `libfuzzer-sys` source targets in `fuzz/` (its own workspace,
  `exclude = ["fuzz"]` at the root) calling the same `fuzz_one`. A separate
  `fuzz_rpc` target was added with D4 for JSON-RPC HTTP/WebSocket parser
  coverage. The C9 targets were **not executed** at that time (stable toolchain,
  no cargo-fuzz/nightly); H3 runs the 24-hour campaign. This is stated
  explicitly rather than claimed.
- `make test-core` and `make fuzz-smoke` are now real (the C0 stubs are
  replaced); `make test` additionally runs the vector inventory and
  `tools/check_rig.py`, which fails if any gate is unwired.
- Spec: `docs/spec/test-rig.md`.
- Gate: `make test` → **109 passed / 0 failed** (96 C0–C8 + 13 C9),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 26 markdown files,
  dependency gate 71 external crates allowlisted, **60 s fuzz with 0
  crashes**.
- NC: `make nc-c9` → **6/6 PASS**: corrupted golden vector, impossible
  `MIN_VECTORS`, injected non-minimal-varint bug, injected trailing-byte
  bug, zeroed fuzz iteration counter, unwired Makefile. The first four show
  the rig detects **real** regressions. Total across C0–C9: **50 NC**.
- Evidence: `docs/evidence/c9/gates.txt`, `docs/evidence/c9/nc.log`,
  `docs/evidence/c9/SHA256SUMS` (also pins the fixture, spec and addendum).
- Fixes found by the gate itself: clippy rejected the PRNG method name
  `next` (renamed `next_u64`); a `prop_assert_eq!` moved its value (cloned);
  the fuzz CLI initially spent 6×60 s instead of 60 s total (budget split
  added); the first truncated-vector generator produced duplicate cut
  lengths (payload lengthened to 256 bytes). All fixed in code.
- NC hygiene: `failure_persistence: None` in the proptest config and a
  `*.proptest-regressions` ignore rule ensure the artificial NC failures
  never leave bogus regression seeds in the tree.
- Deferred: Miri/ASan/UBSan and 24 h libFuzzer coverage (H3), state-root
  determinism (X7), C10 close-out, `cargo deny` (H1).


### C10 receipt (2026-10-26)

- Deliverable: `patches/0011-C10-close-out.patch`. No new crate, no new
  external dependency — C10 is an audit/consolidation phase.
- Landed: `tools/audit_closeout.py`, wired into `make test` and CI. It parses
  all 12 plans, finds every ✅ row and requires for it a numbered patch, a
  `### Cx receipt`, a `## [Cx` CHANGELOG entry, `tools/nc_cX.py`, a valid
  `docs/evidence/cX/nc.log`, and evidence receipts; verifies **all**
  evidence SHA256SUMS, the 56-NC total, that all `docs/spec/` files are
  indexed and that no deferred (non-CORE) phase is marked done.
- Landed: `docs/spec/FORMAT_INDEX.md` — index of all six format specs plus
  dependency governance files and the phase→artifact ledger.
- Landed: `docs/CLOSEOUT_AUDIT.md` — honest maturity assessment: L1 (core
  libraries + test rig, no network), explicit non-goals and known weaknesses
  (serde_json config-only, opaque QC, non-coverage-guided fuzz, no
  `cargo deny`, sandbox toolchain note).
- Landed: `docs/evidence/c10/closeout.txt` — consolidated inventory of all
  **56** negative controls read back from the 11 committed NC logs.
- Gate: `make test` → **109 passed / 0 failed**, fmt/clippy clean, drift guard
  12 plans / 112 rows, dependency gate 71 external crates, vector inventory
  956, `make audit-closeout` green.
- NC: `make nc-c10` → **6/6 PASS**: deleted receipt, deferred phase marked
  ✅, forged NC result, unindexed spec, forged patch subject, corrupted
  evidence checksum. Each restores historical evidence byte-for-byte.
  Total across C0–C10: **56 NC**.
- Evidence: `docs/evidence/c10/gates.txt`, `docs/evidence/c10/nc.log`,
  `docs/evidence/c10/closeout.txt`, `docs/evidence/c10/SHA256SUMS` (also
  pins `FORMAT_INDEX.md` and `CLOSEOUT_AUDIT.md`).
- CORE_PLAN is now **closed**. Next in dependency order is `STATE_PLAN` S0;
  M0 additionally needs S0–S2 and X0–X2. Stage 2 remains ⏸ by separate
  decision.

## Key risks

1. **Non-canonical serialization** → two nodes get different hashes for the same
   object. Fixed by C2 + banning `serde` in consensus.
2. **Replay/domain substitution** → a testnet signature passes on mainnet. C3/C4 + D5.
3. **Floating point / local clocks in types** → caught by a clippy ban and review.

## Non-goals

Consensus, VM, P2P, RPC, wallet, UI. CORE is only the "bricks".

## Closure criteria

Part of M0: a compiling workspace, all formats fixed in `docs/spec/`, and
not one format change without a version bump.

**C10 closure (measured, 2026-10-26):** the original "11 stub crates,
200+ tests" wording is superseded by the actual tree — 15 crates (8
implemented, 7 honest placeholders), 109 named Rust tests, 5 proptest
properties × 256 cases, 956 golden vectors, 56 negative controls and 11
receipts. `make test` and `make audit-closeout` are green. M0 as a milestone
is **not** closed: it also requires `STATE_PLAN` S0–S2 and `EXEC_PLAN` X0–X2.
