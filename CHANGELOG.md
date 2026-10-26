# Changelog — AuraLite-Crypto

All notable changes. Dates are ISO 8601, Europe/Moscow. Historical entries are
never rewritten: corrections are made with a new entry or an explicit note.

## [C10 — close-out] 2026-10-26

**Eleventh code phase and closure of `CORE_PLAN`.** Deliverable:
`patches/0011-C10-close-out.patch`. No new crate, no new external dependency.

- **Plan↔tree audit:** `tools/audit_closeout.py` runs inside `make test` and
  CI. For **every** ✅ phase row across all 12 plans it requires a numbered
  patch, a `### Cx receipt` in `CORE_PLAN.md`, a `## [Cx` CHANGELOG entry,
  `tools/nc_cX.py`, a valid `docs/evidence/cX/nc.log` and evidence receipts;
  it verifies all evidence SHA256SUMS, the **56-NC** total, that every
  `docs/spec/` file is indexed, and that no deferred (non-CORE) phase is
  marked done. The C10 patch itself is cut at commit time, so the audit
  validates the ten shipped patches and reports C10 once present
  (`--strict` requires it).
- **Format index:** `docs/spec/FORMAT_INDEX.md` indexes the six byte-level
  specs (tx, block, proof, account, genesis, test rig), the genesis config,
  the frozen C3 dependency baseline and the C8/C9 addenda, plus a
  phase→patch→NC→evidence ledger.
- **Maturity audit:** `docs/CLOSEOUT_AUDIT.md` — what is real (table with
  evidence), measured numbers, what is explicitly not real (no network, no
  state tree, no VM, no audit, no mainnet), six known weaknesses and the
  verdict **L1 — core libraries + test rig, no network**. M0 is not closed:
  it still needs `STATE_PLAN` S0–S2 and `EXEC_PLAN` X0–X2.
- **NC inventory:** `docs/evidence/c10/closeout.txt` is generated from the
  11 committed NC logs and lists all **56** controls with their verdicts.
- **Gate:** `make test` → **109 passed / 0 failed**, fmt/clippy clean, drift
  guard 12 plans / 112 phase rows, dependency gate 71 external crates,
  vector inventory 956, `make audit-closeout` green.
- **Negative controls:** `tools/nc_c10.py` → `make nc-c10` →
  `docs/evidence/c10/nc.log`, **6/6 PASS**: deleted C7 receipt, `S1` marked
  ✅ in a deferred plan, forged C9 NC result, unindexed format spec, forged
  patch subject, corrupted evidence checksum. All restore historical
  evidence byte-for-byte. Total C0–C10: **56 NC**.
- **Tooling / CI:** `make nc-all` covers C0–C10 (56 NC) without rewriting
  historical logs; CI adds a `close-out audit` step and regenerates only
  `make evidence-c10`. New targets: `make audit-closeout`, `make nc-c10`,
  `make evidence-c10`.
- **Evidence:** `docs/evidence/c10/gates.txt`, `docs/evidence/c10/nc.log`,
  `docs/evidence/c10/closeout.txt`, `docs/evidence/c10/SHA256SUMS` (also
  pins `FORMAT_INDEX.md` and `CLOSEOUT_AUDIT.md`).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md` (maturity L1),
  `docs/ARCHITECTURE.md` (C10 block), `CHANGELOG.md`, `ROADMAP.md` (ledger
  CORE ✅ COMPLETE, §4, §8 → next S0), `CONTRIBUTING.md`, `CORE_PLAN.md`
  (status ✅ COMPLETE, C10 ✅, decision D-C12 + receipt, corrected closure
  criteria), plus `docs/CLOSEOUT_AUDIT.md` and `docs/spec/FORMAT_INDEX.md`.
- **Not claimed:** state tree / `state_root` (S1), VM (X1), consensus/P2P/
  node/RPC, Miri/ASan/UBSan or 24 h fuzzing (H3), state-root determinism
  (X7), external audit / white paper / mainnet (⏸ Stage 2), `cargo deny`
  (H1). Validators/blocks/tx remain **0/0/0**.

## [C9 — test rig] 2026-10-26

**Tenth code phase.** Deliverable: `patches/0010-C9-test-rig.patch`.
Adds the **15th crate**, `aura-testkit`, and `proptest =1.12.0` as a **dev
dependency** (`default-features = false, features = ["std"]`); +13
transitive dev-only crates recorded in a reviewed C9 addendum.

- **Golden vectors:** `tools/gen_c9_vectors.py` — independent fixed-seed
  Python LCG with a C2 minimal-LEB128 transcription. 512 new vectors in
  `c9_vectors.txt`: 128 valid varints (boundaries + pseudo-random), 128
  non-minimal/overflow varints, 128 trailing-byte encodings, 128 distinct
  truncated encodings. Generator self-checks every case before writing;
  offline and byte-stable.
- **Vector inventory:** `tools/count_vectors.py` counts vectors across all 9
  fixtures with an explicit constant/vector classification → **956 vectors**
  (>= 200 required by the C9 DoD). It runs inside `make test`.
- **Wiring check:** `tools/check_rig.py` runs inside `make test` and fails if
  `test-core`, `fuzz-smoke`, the vector inventory, `proptest`, `nc-all` or a
  libFuzzer target disappears from the Makefile/CI. A rig that is never run
  protects nothing.
- **Property tests:** 5 `proptest` properties (256 cases each) — varint
  minimality/round-trip, trailing-byte rejection, `CanonicalRecord`
  round-trip, decoder totality on arbitrary bytes, C7 key-digest purity.
  Plus 3 harness tests and 5 vector tests (13 new tests total).
- **Portable fuzz harness:** `fuzz_one(target, bytes)` feeds arbitrary input
  to six decoders (`codec`, `tx`, `state-key`, `genesis-json`, `block`,
  `merkle-proof`) under `catch_unwind`; `run_portable_fuzz` uses a fixed
  xorshift64* PRNG with separate `work`/`iterations` counters. `aura-fuzz`
  CLI: `--seconds` is the TOTAL budget split across targets. `make
  fuzz-smoke` runs the measured **60 s campaign with 0 crashes**;
  `make test-core` uses a short iteration-bounded pass.
- **libFuzzer targets:** `fuzz/` (separate workspace, `exclude = ["fuzz"]`)
  with `fuzz_codec`, `fuzz_tx`, `fuzz_state_key`, `fuzz_genesis_json`
  calling the same `fuzz_one`. `libfuzzer-sys` never enters the production
  `Cargo.lock`. These targets were **not executed** in this sandbox (stable
  toolchain, no cargo-fuzz/nightly); H3 runs the 24-hour campaign. Stated,
  not claimed.
- **Real gates replace the C0 stubs:** `make test-core` (vector inventory +
  rig wiring + all crate tests + short fuzz + drift/deps guards) and
  `make fuzz-smoke` (60 s). CI has explicit `vector-inventory`, `CORE shard`
  and `fuzz smoke (60 s)` steps.
- **Spec:** `docs/spec/test-rig.md`.
- **Gate:** `make test` → **109 passed / 0 failed** (96 C0–C8 + 13 C9),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 26 markdown files,
  dependency gate 71 external crates allowlisted, 956 vectors, 60 s fuzz
  with zero panics.
- **Negative controls:** `tools/nc_c9.py` → `make nc-c9` →
  `docs/evidence/c9/nc.log`, **6/6 PASS**: corrupted golden vector,
  impossible `MIN_VECTORS`, injected non-minimal-varint bug, injected
  trailing-byte-guard bug, zeroed fuzz iteration counter, unwired Makefile.
  The first four inject **real production bugs** and prove the properties
  catch them. Total C0–C9: **50 NC**.
- **Fixes found by the gate itself:** clippy rejected the PRNG method name
  `next` (renamed `next_u64`); a `prop_assert_eq!` moved its value (cloned);
  the fuzz CLI initially spent 6×60 s instead of 60 s total; the truncated
  vector generator produced duplicate lengths (payload lengthened to 256
  bytes). All fixed in code, not suppressed.
- **NC hygiene:** because C9 NCs deliberately inject real bugs, `proptest`
  was writing `properties.proptest-regressions` seeds for failures that do
  not exist in correct code. `failure_persistence: None` is now set in the
  proptest config and `*.proptest-regressions` is gitignored, so artificial
  NC failures can never leak into the repository.
- **Tooling / CI:** `make nc-all` covers C0–C9 (50 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c9`. New targets:
  `make gen-c9-vectors`, `make nc-c9`, `make evidence-c9`,
  `make vector-inventory`, `make record-c9-deps`.
- **Evidence:** `docs/evidence/c9/gates.txt` (pre-C9 counts from the C8
  commit, inventory, wiring, 60 s fuzz, raw gates), `docs/evidence/c9/nc.log`,
  `SHA256SUMS` (also pins the fixture, spec and C9 addendum).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C9 block + crate graph), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C9 ✅,
  decision D-C11 + receipt), `SECURITY_PLAN.md` (C9 note), and the new
  `docs/spec/test-rig.md`.
- **Not claimed:** Miri/ASan/UBSan, 24-hour libFuzzer campaign, coverage
  thresholds or crash triage (H3/O0); state-root determinism (X7);
  genesis block / execution; `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [C8 — genesis] 2026-10-26

**Ninth code phase.** Deliverable: `patches/0009-C8-genesis.patch`.
Adds the **14th crate**, `aura-genesis`, and the **first new external
dependency since C3**: `serde_json =1.0.154` (config parsing only).

- **Genesis document:** `Genesis { chain_id, protocol_version,
  genesis_time_ms, validators, allocations }` with `Validator`
  (`address`, `power`, `public_key[32]`) and `Allocation` (`address`,
  `balance`). A strict JSON schema with exactly five top-level keys; unknown
  or missing keys, wrong types, invalid bech32/hex, float balances, zero
  power, zero allocations, duplicate addresses, empty validator sets and
  `u128` supply overflow are all rejected. Balances are **decimal strings** —
  `u128` amounts never round-trip through `f64`.
- **Canonical binary:** C2 encoding (`varint(len) ‖ chain_id`, `u32`, `u64`,
  validator and allocation vectors). `from_canonical_bytes` uses
  `decode_canonical()` and then re-runs every structural check. JSON is
  never hashed directly — `serde_json` is used only to read the config,
  exactly the exception allowed by `ARCHITECTURE.md`.
- **Commitments:** `genesis_hash` = C3 `hash_domain(chain_id,
  Domain::Genesis, GENESIS_CONTEXT ‖ canonical_bytes)`; `validator_set_hash`
  is now actually computed (C5 left the header field caller-supplied);
  `allocation_root` is a C6 Merkle root over C7 `StateKey::account` digests
  and `Account` values, sorted by address. It is an allocation commitment,
  **not** the JMT `state_root` (S1).
- **Determinism:** object key order and whitespace do not change any
  commitment; validator array order **does** (C8 NC); allocation array order
  does not change `allocation_root` because state is a set.
  `genesis_time_ms` is explicit — no local clock.
- **CLI:** the `aura` binary gains `genesis hash --file`, `genesis verify
  --file --expect` and `genesis show --file`. `verify` exits 1 on mismatch,
  bad arguments exit 2. `make genesis-hash` is a convenience wrapper.
  D8 owns the rest of the CLI.
- **Spec / sample:** `docs/spec/genesis-format.md` and
  `docs/spec/genesis-testnet.json` (the sample's hash is pinned by a test).
- **Independent golden vectors:** `tools/gen_c8_vectors.py` — pure-Python
  BLAKE3 (7 official vectors), an independent BIP-173 bech32 encoder, C3
  domain framing, C5/C6 tree rules, C7 key/account encodings. 44 fixture
  lines: **19 JSON documents** (3 equivalent, 2 order variants, 13 invalid)
  and **2 full commitment sets**. The generator self-checks every
  determinism property. Offline and byte-stable.
- **DoD gates:** three independent JSON representations of the same config
  give one hash and one canonical byte string; reordering validators changes
  both `genesis_hash` and `validator_set_hash`. Four CLI integration tests
  pin `hash`, `verify` (accept + reject), `show` and usage.
- **Dependency governance:** the C3 allowlist is **frozen** historical
  evidence (pinned by `docs/evidence/c3/SHA256SUMS`). C8 records its +4
  delta (`serde_json`, `itoa`, `memchr`, `zmij`) in
  `docs/spec/c8-dependency-addendum.txt`; `tools/check_deps.py` compares
  `Cargo.lock` against the union of baseline + addenda.
  `tools/record_c8_deps.py` / `make record-c8-deps` is the reviewed action.
- **Gate:** `make test` → **96 passed / 0 failed** (82 C0–C7 + 14 C8),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 24 markdown files,
  dependency gate 58 external crates allowlisted.
- **Negative controls:** `tools/nc_c8.py` → `make nc-c8` →
  `docs/evidence/c8/nc.log`, **6/6 PASS**: genesis context, validator-set
  context, golden hash, validator-order sorting, unknown-field rejection,
  Stage 1 chain-id check. Total C0–C8: **44 NC**.
- **Fixes found by the gate itself:** a CLI test used a path relative to the
  test file instead of `CARGO_MANIFEST_DIR`; a JSON string helper needed an
  explicit lifetime; an unused import was removed. All fixed in code.
- **Tooling / CI:** `make nc-all` covers C0–C8 (44 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c8`. New targets:
  `make gen-c8-vectors`, `make nc-c8`, `make evidence-c8`,
  `make record-c8-deps`, `make genesis-hash`; `make test-core` covers all
  eight implemented crates.
- **Evidence:** `docs/evidence/c8/gates.txt` (pre-C8 counts from the C7
  commit, dependency delta, CLI hash, raw gates), `docs/evidence/c8/nc.log`,
  `SHA256SUMS` (also pins fixture, spec, sample config, addendum).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C8 block + crate graph), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C8 ✅,
  decision D-C10 + receipt), `NODE_PLAN.md` (C8 note), and the new
  `docs/spec/genesis-format.md`.
- **Not claimed:** genesis **block** / execution (B/D phases), JMT
  `state_root` (S1), validator key ceremony (W7/O5), BLS aggregation (B2),
  Stage 2 mainnet genesis, `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [C7 — account model] 2026-10-26

**Eighth code phase.** Deliverable: `patches/0008-C7-account-model.patch`.
**No new crate and no new external dependency** — `aura-state`, an honest
empty placeholder since C0, gets its first real API.

- **Account value:** `Account { nonce, balance, code_hash, storage_root }`,
  88 canonical bytes (`u64 LE ‖ u128 LE ‖ 32 ‖ 32`), `decode_canonical()`
  round-trip. `NO_CODE = EMPTY_STORAGE_ROOT = Hash::ZERO` sentinels.
  `credit`/`debit`/`bump_nonce` use checked arithmetic — no silent wrap; a
  failed debit leaves the balance unchanged.
- **State key layout:** five protocol namespaces — `0x01` account (21 bytes),
  `0x02` code (21), `0x03` storage (53 = tag + contract[20] + slot[32]),
  `0x04` staking (21), `0x05` receipt (33). `0x00` and `0x06`–`0xFF` are
  **reserved**: `StateKey::from_raw` rejects them, so contract code cannot
  even construct such a key. Known prefix + exact namespace length only.
- **Contract isolation:** `assert_contract_may_write(contract, key)` allows
  only `0x03 || its_own_address || slot` — other namespaces give
  `ForbiddenContractWrite`, another contract's storage gives
  `WrongContractStorage`. The VM host ABI (X phases) must call it per write.
- **`StateKey::digest`:** `BLAKE3(STATE_KEY_CONTEXT || u64(len) || raw)` — a
  domain-separated, length-prefixed JMT path ready for S1. C7 does **not**
  compute a state root and has no storage backend.
- **Spec:** `docs/spec/account-model.md` (namespaces, reserved space,
  isolation, digest, account wire format, arithmetic, non-goals).
  `STATE_PLAN.md` carries a C7 note clarifying that S1 still owes the tree.
- **Independent golden vectors:** `tools/gen_c7_vectors.py` — pure-Python
  BLAKE3 (self-checked against 7 official vectors) plus a transcription of
  the key layout, digests and account encoding. 107 fixture lines: raw keys,
  encoded keys and digests for 5 namespaces × 4 indices (generator asserts
  all 20 digests differ), 3 account encodings, contract-isolation operands
  and **12 rejection vectors** (5 reserved prefixes, wrong key lengths, empty
  key, non-minimal varint, trailing byte, truncation). Offline, byte-stable.
- **DoD gates:** 1000 distinct addresses → 5000 keys/digests with zero
  collisions; all 256 prefixes iterated with exactly five reachable;
  contracts cannot write outside their own storage namespace.
- **Gate:** `make test` → **82 passed / 0 failed** (71 C0–C6 + 11 C7),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 23 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c7.py` → `make nc-c7` →
  `docs/evidence/c7/nc.log`, **6/6 PASS**: state-key context, golden digest,
  reserved-prefix rejection, namespace length, contract namespace, contract
  address. Total C0–C7: **38 NC**.
- **Defects found by the gate itself:** the first deterministic address
  pattern had a period of 256 and collided at index 256 — replaced by a
  pattern whose first 8 bytes encode the index. Two wrong test assumptions
  (account length 76 vs 88, storage-key length 21 vs 53) were corrected.
  All fixed in code, not suppressed.
- **Tooling / CI:** `make nc-all` now covers C0–C7 (38 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c7`. New targets:
  `make gen-c7-vectors`, `make nc-c7`, `make evidence-c7`; `make test-core`
  now covers all six implemented crates.
- **Evidence:** `docs/evidence/c7/gates.txt` (pre-C7 counts from the C6
  commit, DoD gates, raw output), `docs/evidence/c7/nc.log`, `SHA256SUMS`
  (also pins the golden fixture and the spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C7 block), `CHANGELOG.md`, `ROADMAP.md` (ledger,
  §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C7 ✅, decision D-C9 +
  receipt), `STATE_PLAN.md` (C7 note), and the new
  `docs/spec/account-model.md`.
- **Not claimed:** versioned state tree / `state_root` (S1), persistence
  (S2), execution overlays (S3), genesis allocation (C8), test rig/fuzzing
  (C9), VM host ABI (X phases), `cargo deny` (H1). Validators/blocks/tx
  remain **0/0/0**.

## [C6 — Merkle proofs] 2026-10-25

**Seventh code phase.** Deliverable:
`patches/0007-C6-merkle-proofs.patch`. **No new crate and no new external
dependency** — the proof module lives inside `aura-block`.

- **Compact inclusion proofs:** `MerkleProof = varint(index) ||
  varint(total) || leaf[32] || varint(sibling_count) || sibling[32]*count`.
  Sibling direction is derived from `index >> level`, so no direction bits are
  stored. `MAX_PROOF_DEPTH = 64`; minimal LEB128 varints and
  `decode_canonical()` reject non-canonical encodings. For 1000 leaves the
  depth is 10 and a proof is ~358 bytes.
- **Two leaf kinds, one tree shape.** Transaction leaves are the C5
  `leaf_hash(txid)`; event leaves are
  `BLAKE3(EVENT_LEAF_CONTEXT || u64(len) || event)`, so a tx leaf cannot be
  replayed as an event leaf and ambiguous concatenations cannot collide.
  `verify_tx_inclusion` / `verify_event_inclusion` bind the supplied value to
  `proof.leaf` before checking the path — without that, a proof would only
  show that *some* hash is in the tree.
- **C5 tree unchanged.** `compute_tx_root` now delegates to `build_tx_tree`;
  root and proof share one padded-level implementation, so they cannot
  disagree. A regression test compares the C5 root of four real C4
  transactions with the C6 tree root. The C5 and C6 generators agree on
  `LEAF_CONTEXT`/`NODE_CONTEXT` — pinned by a test.
- **Spec:** `docs/spec/merkle-proof-format.md` (tree construction, wire
  format, verification order, vectors, non-goals).
- **Independent golden vectors:** `tools/gen_c6_vectors.py` — pure-Python
  BLAKE3 (self-checked against 7 official BLAKE3 vectors) plus a
  transcription of the C5 tree and C6 encoding. 69 fixture lines: roots and
  depths for **1/2/3/5/8/1000** tx leaves, roots for 1/3/5 event leaves,
  **34 canonical proofs** and **11 rejection vectors**, each pinned to a
  specific error. Generator is offline and byte-stable.
- **C6 DoD gate:** 1000 leaves, all **1000** proofs generated and verified,
  then one substituted leaf rejected — both by leaf binding (`LeafMismatch`)
  and by the reconstructed root (`RootMismatch`).
- **Gate:** `make test` → **71 passed / 0 failed** (60 C0–C5 + 11 C6),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 22 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c6.py` → `make nc-c6` →
  `docs/evidence/c6/nc.log`, **6/6 PASS**: event-leaf context, golden root,
  leaf/value binding, sibling-count check, sibling direction, root
  comparison. Total C0–C6: **32 NC**.
- **Gate-found fixes:** clippy required `u64::div_ceil`,
  `is_multiple_of(2)` and an iterator instead of an index loop. All fixed in
  code, not suppressed.
- **Tooling / CI:** `make nc-all` now covers C0–C6 (32 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c6`. New targets:
  `make gen-c6-vectors`, `make nc-c6`, `make evidence-c6`.
- **Evidence:** `docs/evidence/c6/gates.txt` (pre-C6 counts from the C5
  commit, the 1000-leaf gate, raw output), `docs/evidence/c6/nc.log`,
  `SHA256SUMS` (also pins the golden fixture and the spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C6 block), `CHANGELOG.md`, `ROADMAP.md` (ledger,
  §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C6 ✅, decision D-C8 +
  receipt), and the new `docs/spec/merkle-proof-format.md`.
- **Not claimed:** accounts and state key layout (C7), genesis (C8), test rig
  and fuzzing (C9), batch/multiproofs, sparse state trees (S1), proof serving
  over RPC (N/D phases), `cargo deny` (H1). Validators/blocks/tx remain
  **0/0/0**.

## [C5 — block and header] 2026-10-25

**Sixth code phase.** Deliverable: `patches/0006-C5-block-header.patch`.
Adds the **13th crate**, `aura-block`, with **no new external dependencies**.

- **Format:** `Block = Header || Body`. `Header` has 12 fields: `version,
  chain_id, height, parent_hash, tx_root, state_root, validator_set_hash,
  timestamp_ms, proposer, qc, gas_used, base_fee`. `HeaderParams` groups the
  11 caller-supplied fields; `tx_root` is deliberately absent and always
  derived, so it cannot be forged at construction. `Body::new` sorts by
  `(signer, nonce)` (both derive `Ord`) and rejects duplicate keys.
- **C5 amends `ARCHITECTURE.md` §1 (decision D-C7):** `chain_id` is added to
  the header because rule D5 requires chain separation for block ids. No
  migration is needed — the network has produced **0 blocks**.
  `state_root`, `validator_set_hash` and `qc` are caller-supplied values:
  state (S1), validator sets and QCs (B phases) are not implemented. C5 fixes
  their wire representation and validation, not their computation.
- **`tx_root`:** binary BLAKE3 Merkle tree over C4 txids —
  `leaf = BLAKE3(LEAF_CONTEXT || txid)`,
  `node = BLAKE3(NODE_CONTEXT || left || right)`, odd final node duplicated,
  empty body → `Hash::ZERO`. Order-sensitive: the DoD vector proves
  `tx_root_forward != tx_root_backward`. The shape is fixed now so C6 can add
  proofs without changing the root scheme.
- **`block_hash`:** C3 `hash_domain(chain_id, Domain::Block,
  BLOCK_HASH_PAYLOAD_CONTEXT || canonical(Header))`. Header only; `tx_root`
  commits to the body. Any header field or chain change alters the id.
- **Validation:** `validate(expected_chain)` checks version, chain, body/QC
  size limits, strict `(signer, nonce)` ordering, duplicates and then a
  recomputed `tx_root`. `validate_transactions()` is separate and checks each
  tx signature/expiry at `header.height`. `timestamp_ms` is caller supplied —
  no local clock anywhere.
- **Spec:** `docs/spec/block-format.md` (field table, tree, hash, validation
  order, error catalogue, non-goals).
- **Independent golden vectors:** `tools/gen_c5_vectors.py` — pure-Python
  BLAKE3 (self-checked against 7 official vectors), C2/C3 wire transcription
  and the committed C4 tx vectors as bodies. 95 fixture lines: **4 canonical
  blocks** (2/3/0/1 transactions covering even/odd tree levels, empty tree,
  a real `b1 → b2` parent link, `Some(qc)`, u64/u128 maxima), an
  order-sensitivity pair and **9 rejection vectors** (trailing byte,
  truncation, bad version, bad root, reordered body, reordered body **with a
  recomputed root**, duplicate key, bad QC tag, non-minimal count). Each bad
  vector is pinned to a specific error. Generator is offline and byte-stable.
- **Gate:** `make test` → **60 passed / 0 failed** (49 C0–C4 + 11 C5),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 21 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c5.py` → `make nc-c5` →
  `docs/evidence/c5/nc.log`, **6/6 PASS**: leaf context, node context,
  block-hash context, golden `tx_root`, `tx_root` check, canonical-order
  check. Total C0–C5: **26 NC**.
- **Gate-found fixes:** clippy caught an unused import, a collapsible `if`,
  `chunks_exact(2)` and a cloned single-ref slice. Fixed in code
  (`as_chunks::<2>()`, `std::slice::from_ref`), not suppressed.
- **Tooling / CI:** `make nc-all` now covers C0–C5 (26 NC) without rewriting
  historical logs; CI regenerates only `make evidence-c5`. New targets:
  `make gen-c5-vectors`, `make nc-c5`, `make evidence-c5`; `make test-core`
  now covers all five implemented crates.
- **Evidence:** `docs/evidence/c5/gates.txt` (pre-C5 counts from the C4
  commit, 12→13 crates, raw gate output), `docs/evidence/c5/nc.log`,
  `SHA256SUMS` (also pins the golden fixture and the format spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (§1 header + §3 graph + C5 block), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C5 ✅,
  decision D-C7 + receipt), and the new `docs/spec/block-format.md`.
- **Not claimed:** Merkle proofs (C6), accounts (C7), genesis (C8), fuzzing
  (C9), QC/validator-set/state computations (B/S phases), BLS (B2), PQ (H2),
  `cargo deny` (H1). Validators/blocks/tx remain **0/0/0**.

## [C4 — transaction format] 2026-10-25

**Fifth code phase.** Deliverable: `patches/0005-C4-tx-format.patch`.
Adds the **12th crate**, `aura-tx`, with **no new external dependencies**.

- **Transaction format:** `Tx = UnsignedTx || Signature`. `UnsignedTx` has
  the ten C4 fields (`chain_id, signer, nonce, kind, payload, gas_limit,
  max_fee, tip, expiry`) plus `codec_version` from `ARCHITECTURE.md` §1.
  `Signature` is an envelope `{ algo, public_key[32], bytes[64] }` —
  Ed25519 cannot recover a public key, so it travels with the tx and is
  bound to `signer` by `BLAKE3(ADDRESS_CONTEXT || public_key)[0..20]`.
  `TxKind` has five strict tags; `SignatureAlgo` accepts Ed25519 only
  (secp256k1 tag 1 reserved, PQ tags for H2).
- **Signing:** `signing_bytes = SIGNING_CONTEXT || canonical(UnsignedTx)`,
  signed with C3's domain-separated `aura_crypto::sign(Domain::Tx,
  chain_id)` over the BLAKE3 digest. `txid = BLAKE3(TXID_CONTEXT ||
  canonical(Tx))` includes the signature. `TxParams` groups the nine
  constructor fields so clippy's `too_many_arguments` is fixed, not silenced.
- **Replay protection (DoD gate):** `validate(expected_chain, height)`
  rejects a tx minted for `auralite-1` on `auralite-test-1`; rewriting the
  chain-id after signing breaks the signature because the chain-id is inside
  both `signing_bytes` and the C3 domain prefix. Validation also checks
  version, payload ≤ 65536, gas ≠ 0, tip ≤ max_fee and `height ≤ expiry`.
  `height`/`expiry` are consensus heights — no local clock.
- **Spec:** `docs/spec/tx-format.md` — field/wire tables, tags, signing,
  txid, validation order, error catalogue and non-goals.
- **Independent golden vectors:** `tools/gen_c4_vectors.py` contains
  pure-Python BLAKE3 (self-checked against 7 official BLAKE3 vectors),
  pure-Python Ed25519 (self-checked against RFC 8032 §7.1 TEST 1/2/3) and a
  C2 wire encoder. 104 fixture lines: **5 canonical transactions** (including
  u64/u128 maxima and a mainnet-chain tx) and **9 rejection vectors**
  (trailing byte, truncation, bad version, unknown kind, unknown algorithm,
  tampered signer/signature, non-minimal chain and payload length). Each bad
  vector is pinned to a specific error. Generator is offline and byte-stable.
- **Additive C2 change:** `CodecError::InvalidEnumTag(&'static str, u8)` for
  caller-defined enums. No C2 wire-format change; C2 vectors, evidence and
  checksums untouched.
- **Gate:** `make test` → **49 passed / 0 failed** (39 C0–C3 + 10 C4),
  fmt/clippy clean, drift guard 12 plans / 112 rows / 20 markdown files,
  dependency gate 54 external crates allowlisted.
- **Negative controls:** `tools/nc_c4.py` → `make nc-c4` →
  `docs/evidence/c4/nc.log`, **6/6 PASS**: signing context, txid context,
  golden signature, replay protection, signer↔key binding, Ed25519
  verification. Total C0–C4: **20 NC**.
- **Tooling / CI:** `make nc-all` runs all 20 NCs **without rewriting**
  historical C0–C3 logs, so their committed `SHA256SUMS` stay valid; CI now
  regenerates only the current phase's evidence (`make evidence-c4`). New
  targets: `make gen-c4-vectors`, `make nc-c4`, `make evidence-c4`.
  `make test-core` now covers all four implemented crates.
- **Evidence:** `docs/evidence/c4/gates.txt` (pre-C4 counts from the C3
  commit, 11→12 crates, raw gate output), `docs/evidence/c4/nc.log`,
  `SHA256SUMS` (also pins the golden fixture and the format spec).
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (crate graph + C4 block), `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C4 ✅,
  decision D-C6 + receipt), and the new `docs/spec/tx-format.md`.
- **Not claimed:** block/header and `tx_root` (C5), Merkle proofs (C6),
  accounts (C7), genesis (C8), fuzzing (C9), fee charging/gas metering,
  secp256k1 in tx signatures, hardware keys (W5), PQ signatures (H2),
  `cargo deny` (H1). Validators/blocks/tx remain **0/0/0**.

## [C3 — cryptography] 2026-10-25

**Fourth code phase. First phase with external dependencies.**
Deliverable: `patches/0004-C3-crypto.patch`.

- **`aura-crypto` crate:** BLAKE3 hashing (upstream `pure` feature — portable
  Rust, no C/assembly in the hash path), Ed25519 signatures (RFC 8032),
  secp256k1 deterministic ECDSA (RFC 6979, low-S, compatibility only),
  domain-separated hashing and signing, constant-time byte comparison
  (`subtle`) and zeroizing secret storage (`zeroize`).
- **Domain separation:** `HASH_CONTEXT || u64(len(prefix)) || prefix ||
  u64(len(payload)) || payload`, where `prefix = chain_id || 0x00 ||
  domain_tag || 0x00` comes from C1's `Domain::prefix`. A testnet signature
  is invalid on mainnet; a vote cannot be replayed as a transaction. Changing
  `HASH_CONTEXT` is a protocol change and is pinned by an NC.
- **Independent golden vectors** (`tools/gen_c3_vectors.py`, 66 fixture
  lines, byte-stable, offline): 15 official BLAKE3 vectors from
  `BLAKE3-team/BLAKE3/test_vectors.json`; a self-checking pure-Python BLAKE3
  producing 5 domain-hash vectors; RFC 8032 §7.1 Ed25519 TEST 1/2/3; and a
  pure-Python secp256k1 + RFC 6979 implementation producing 2 signatures,
  additionally verified with OpenSSL `pkeyutl -verify`. `k256` matched the
  independent signatures byte for byte.
- **Dependencies (reviewed, not silent):** exact pins in
  `[workspace.dependencies]` — `blake3 =1.8.7`, `ed25519-dalek =2.2.0`,
  `k256 =0.13.4`, `zeroize =1.8.2`, `subtle =2.6.1`. `Cargo.lock` resolves
  54 external crates, all crates.io-sourced with sha256 checksums.
  `docs/spec/c3-dependency-allowlist.txt` records the exact resolved set;
  `tools/check_deps.py` is now part of `make test` and rejects unpinned/git
  specs, missing checksums, direct `serde` in consensus crates and allowlist
  drift. `tools/record_c3_deps.py` is the deliberate re-record action.
  **Full `cargo deny` (licences + advisories) is still owed to H1.**
- **Gate:** `make test` → **39 passed / 0 failed** (28 C0–C2 + 11 C3),
  fmt/clippy clean, drift guard 12 plans / 112 rows, dependency gate OK.
- **Negative controls:** `tools/nc_c3.py` → `make nc-c3` →
  `docs/evidence/c3/nc.log`, **5/5 PASS**: changed `HASH_CONTEXT`, corrupted
  domain-hash vector, corrupted RFC 8032 vector, Ed25519 verification
  bypass, dependency allowlist drift (this one runs `make test`, proving the
  new gate is actually wired in). Total across C0–C3: **14 NC**.
- **Evidence:** `docs/evidence/c3/gates.txt` (pre-C3 counts from the C2
  commit, dependency audit, raw gate output), `docs/evidence/c3/nc.log`,
  `SHA256SUMS`.
- **Tooling:** `make gen-c3-vectors`, `make nc-c3`, `make evidence-c3`,
  `make check-deps`, `make record-c3-deps`; CI runs all four phases' NCs and
  uploads C0–C3 evidence.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C3 status block), `CHANGELOG.md`, `ROADMAP.md`
  (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C3 ✅ + receipt).
- **Not claimed:** BLS12-381 QC aggregation (B2), PQ migration to
  ML-DSA/SLH-DSA (H2), hardware-backed keys (W5), tx/block formats (C4/C5),
  `cargo deny` (H1). `cc` remains an upstream build-dependency of `blake3`
  but no native hash code is compiled with the `pure` feature.

## [C2 — canonical codec] 2026-10-25

**Third code phase.** Deliverable: `patches/0003-C2-codec.patch`. Still zero
external dependencies (one internal path dependency
`aura-codec -> aura-types`).

- **`aura-codec` crate:** `Encode`/`Decode` traits, `Writer`/`Reader`,
  `write_varint`/`read_varint` (minimal unsigned LEB128; non-minimal,
  overflowing and >10-byte encodings rejected), fixed-width LE integers for
  `u8..u128` (deliberately **no** `usize`/`isize`), strict tags for `bool`,
  `Option`, `Domain`, `Denom`, length-prefixed `String`/`Vec<T>`/`ChainId`,
  fixed 32-byte `Hash` and 20-byte `Address`, checked `Amount`/`Nonce`/
  `Height`/`Gas`, and `BTreeMap` decoding that requires strictly ascending
  keys (rejecting disorder and duplicates).
- **Two independent canonicality layers:** strict decoders plus
  `decode_canonical()`, which re-encodes the decoded value and requires exact
  byte equality with the input (`NonCanonicalEncoding` otherwise).
  `CODEC_VERSION = 1`, `MAX_LENGTH = 1 MiB`, `MAX_ITEMS = 65536`.
- **Reference record:** `CanonicalRecord` with 11 fields in a fixed wire
  order, covering every primitive kind. `serde` remains absent.
- **Independent golden vectors:** `tools/gen_c2_vectors.py` re-implements the
  wire format in Python; `crates/aura-codec/tests/fixtures/c2_golden.txt`
  holds 21 lines — 3 canonical records (incl. `u64`/`u128` maxima and
  multi-byte UTF-8), 5 constants and 12 non-canonical vectors (bad version,
  domain, bool, option tag; trailing byte; non-minimal chain length and map
  count; oversized length; invalid UTF-8; unordered and duplicate map keys;
  varint overflow). Each bad vector is pinned to a specific `CodecError`.
- **Gate:** `make test` → **28 passed / 0 failed** (17 C0/C1 + 11 C2),
  fmt/clippy clean, drift guard 12 plans / 112 rows.
- **Negative controls:** `tools/nc_c2.py` → `make nc-c2` →
  `docs/evidence/c2/nc.log`, **4/4 PASS**: `CODEC_VERSION` 1→2, corrupted
  golden fixture, disabled `Reader::finish()` trailing-byte guard, disabled
  non-minimal-varint predicate. All restore, with a mandatory green check.
- **Evidence:** `docs/evidence/c2/gates.txt` (raw output incl. pre-C2 counts
  from the C1 commit and a dependency audit showing 0 external crates),
  `docs/evidence/c2/nc.log`, `SHA256SUMS`.
- **Tooling:** `make gen-c2-vectors`, `make nc-c2`, `make evidence-c2`; CI now
  runs all three phases' NCs (9 total) and uploads C0/C1/C2 evidence.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`,
  `docs/ARCHITECTURE.md` (C2 wire format), `CHANGELOG.md`, `ROADMAP.md`
  (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C2 ✅ + receipt).
- **Not claimed:** hashing/signatures (C3), tx/block formats (C4/C5),
  `cargo deny` (H1). Any wire format change needs a `CODEC_VERSION` bump and a
  migration test.

## [C1 — canonical types] 2026-10-25

**Second code phase.** Deliverable: `patches/0002-C1-types.patch`. Still zero
external dependencies.

- **New types in `aura-types`:** `Hash` (32 bytes, hex/slice decode, `ZERO`),
  `Address` (20 bytes, in-tree classic BIP-173 bech32 encode/decode),
  `Amount`/`Denom` (checked `u128` in `aalcry`, `from_coins`),
  `ChainId` (validated `[a-z0-9-]`, 3..64), `Nonce`/`Height`/`Gas`
  (checked `u64`), `Domain` (5 tags, `chain-id || 0 || tag || 0` prefix for
  D5), and `CanonicalMap = BTreeMap` (rule D3).
- **Independent golden vectors:** `tools/gen_c1_vectors.py` implements BIP-173
  separately in Python; `crates/aura-types/tests/fixtures/c1_golden.txt` holds
  23 lines — 4 hash vectors, 4 address vectors, 2 corrupted checksums, wrong
  prefix, uppercase, amount bounds. `make gen-c1-vectors` is byte-stable and
  its sha256 is checked in evidence.
- **Gate:** `make test` → **17 passed / 0 failed** (5 unit + 11 C1 + 1
  identity), fmt/clippy clean, drift guard still 12 plans / 112 rows.
- **Negative controls:** `tools/nc_c1.py` → `make nc-c1` →
  `docs/evidence/c1/nc.log`, **3/3 PASS**: checksum constant 1→0, address
  version `0x00`→`0x01`, corrupted golden vector. Each requires red, then
  restores; a final green restore check is mandatory.
- **Evidence:** `docs/evidence/c1/gates.txt` (raw output incl. pre-C1 counts
  from the C0 commit), `docs/evidence/c1/nc.log`, `SHA256SUMS` (gates, NC log,
  golden fixture).
- **Tooling:** `make gen-c1-vectors`, `make nc-c1`, `make evidence-c1`; CI now
  runs both phases' NCs and uploads both evidence directories. C0 evidence is
  explicitly marked historical and is **not** regenerated in CI (it would
  relabel the wrong tree).
- **Defects caught by the existing C0 gate while writing C1** (fixed in this
  patch, not hidden): a compile error `&String + &str` in a vector test and
  rustfmt drift in four files. The gate did its job.
- **Docs synced (D9):** `README.md`, `docs/STATUS.md`, `CHANGELOG.md`,
  `ROADMAP.md` (ledger, §4, §8), `CONTRIBUTING.md`, `CORE_PLAN.md` (C1 ✅
  + receipt).
- **Not claimed:** bech32m, serialization (C2), hashing or signatures (C3),
  tx/block formats (C4/C5). bech32m is a versioned decision, not a C1 tweak.

## [C0 — baseline workspace] 2026-10-25

**First code.** Deliverable: `patches/0001-C0-baseline-workspace.patch`.
No business logic: C0 changes only the measurable foundation, exactly as the
CORE_PLAN DoD requires.

- **Workspace:** `Cargo.toml` with 11 member crates and **zero external
  dependencies**; `Cargo.lock` committed; `rust-toolchain.toml` pinned to
  rustc 1.99.1 with `rustfmt` + `clippy`.
- **Identity in code:** `crates/aura-types` exports `NETWORK_NAME`,
  `CHAIN_ID="auralite-test-1"`, `MAINNET_CHAIN_ID="auralite-1"` (reserved),
  `DENOM="ALCRY"`, `SMALLEST_UNIT="aalcry"`, `DECIMALS=18`,
  `BECH32_PREFIX="aura"`, `CONSENSUS="bft-pos"`, `VM="wasm"`,
  `PROTOCOL_VERSION=1`, plus `is_our_chain()` and `units_per_coin()`.
  4 unit tests + 1 integration test lock these values; the testnet/mainnet
  chain-id separation is asserted.
- **Honest placeholders:** 10 crates (`aura-codec`, `aura-crypto`,
  `aura-state`, `aura-storage`, `aura-exec`, `aura-consensus`, `aura-p2p`,
  `aura-mempool`, `aura-rpc`, `aura-node`) compile with no public API —
  CONTRIBUTING.md forbids stubs that silently return success.
- **Gates:** `Makefile` (`deps-check`, `test`, `test-core`, `nc-c0`, plus
  explicitly labelled SKIP targets for sanitizers/determinism/fuzzing), and
  `.github/workflows/ci.yml` running the same commands (one honest C0 shard;
  the 10 shards arrive with O0).
- **Docs drift guard:** `tools/check_claims.py` is part of `make test`. It
  validates every local markdown link, the exact 112-row phase ledger, the
  4-column phase-row shape, Status/Stage lines and CHANGELOG sync — the
  `MATURITY_AUDIT.md` lesson turned into a gate.
- **Negative controls:** `tools/nc_c0.py` (→ `make nc-c0` →
  `docs/evidence/c0/nc.log`). NC-1 tampers `DENOM` and requires
  `cargo test -p aura-types` to go red; NC-2 deletes the C0 phase row in a
  temp doc tree and requires `check_claims.py` to go red. Both then restore.
- **Defect found by the new gate (measured, not assumed):** the first run of
  `make test` went red because `check_claims.py` compiled its phase-row regex
  without `re.MULTILINE` and therefore reported all 112 phase rows missing.
  Fixed in this same patch; NC-2 now pins that removing a single row really
  does turn the guard red. This is the `MATURITY_AUDIT.md` pattern applied to
  our own tooling: the guard is itself guarded.
- **Evidence:** `docs/evidence/c0-baseline.txt` (pre-C0 zeros measured from
  the baseline git commit, post-C0 numbers with raw command output),
  `docs/evidence/c0/nc.log`, `docs/evidence/SHA256SUMS`. Regenerate with
  `make evidence-c0` (run before the C0 commit so HEAD is the baseline).
- **Honest refusals:** `make devnet` exits 1 with "D7 not implemented";
  `make test-sanitize` / `test-determinism` / `fuzz-smoke` print SKIP lines
  naming H3/C9/X7 instead of pretending to pass.
- **Docs synced in this same patch (D9):** `README.md`, `docs/STATUS.md`,
  `CHANGELOG.md`, `ROADMAP.md` (ledger + §8), `CONTRIBUTING.md`,
  `docs/plans/CORE_PLAN.md` (C0 row ✅).
- **Not claimed:** sanitizers, determinism, fuzzing, devnet, `cargo deny`,
  10 CI shards. Those are named SKIPs with their owning phases.

## [English translation / A0.2] 2026-10-25

Owner request: "make everything in English and continue". All 19 markdown files
(`README.md`, `ROADMAP.md`, `CHANGELOG.md`, `CONTRIBUTING.md`,
`docs/ARCHITECTURE.md`, `docs/STATUS.md`, `docs/TICKER_AUDIT.md` and the 12
`docs/plans/*_PLAN.md`) were translated to English to match the language of
AuraLite-OS and AuraLite-Emulator. Technical identifiers, phase codes, file
paths, decisions D1–D12, Q1–Q7, Stage 1/Stage 2 marks and all measured numbers
are unchanged. This is a documentation-only change: still **0 lines of Rust**.

## [D0 locked / A0.1] 2026-10-25

**Owner decisions locked** (D0 questionnaire). Still no code.

- **Name:** project and repository renamed to `AuraLite-Crypto`;
  network `AuraLite-Crypto Network`; chain-id `auralite-test-1`
  (`auralite-1` reserved for mainnet).
- **Ticker:** **`ALCRY`** chosen (18 decimals, `aalcry`). Added
  [`docs/TICKER_AUDIT.md`](docs/TICKER_AUDIT.md): the CoinGecko search API and
  Yahoo Finance Symbol Lookup returned 0 matches for `ALCRY`; `AURA`, `AUR`,
  `AURS`, `ARA`, `AURC`, `ALC`, `ALCX`, `ALTC`, `ALCR`, `AURY` were rejected
  as taken. Caveat: there is no single global ticker registry, so a re-check is
  mandatory in T1 and before Stage 2.
- **Consensus (Q2):** BFT-PoS — HotStuff-like pipeline, 2f+1 QC, VRF.
- **VM (Q3):** WASM (`wasmtime`) + our ABI + Rust SDK; EVM (`revm`) remains
  the optional phase X9.
- **Scope (Q5):** Stage 1 = M0–M2 (core, devnet, public testnet) with no public
  market token. Mainnet, the MiCA white paper and the external audit moved to
  ⏸ Stage 2 and start by a separate decision.
- Q4 (accounts + JMT), Q6 (documentation language), Q7 (AuraLite OS integration
  — optional E9) accepted as defaults.
- Updated: `README.md`, `ROADMAP.md` (§0, north star, M0–M5 milestones, queue),
  `docs/STATUS.md`, `docs/ARCHITECTURE.md`, all 12 plans (`**Stage:**` lines
  with ⏸ Stage 2 marks on deferred phases).
- Workspace directory: `/home/user/AuraLite-Crypto/`.

## [Plan baseline / A0] 2026-10-25

**What landed:** the plan only. No code, no tests, no network.

- `README.md` — north star, ecosystem composition, architecture at a glance,
  honest Reality check.
- `ROADMAP.md` — house style: ledger of 12 plans, decisions D1–D12,
  open questions Q1–Q7, standing rules, state-of-tree audit, milestones M0–M5,
  critical path.
- `docs/STATUS.md` — measured baseline (0 crates, 0 lines of Rust, 0 tests)
  and maturity scale L0–L5.
- `docs/ARCHITECTURE.md` — invariants, formats, transaction flow, crate
  dependency graph.
- `CONTRIBUTING.md` — phase rules, patch naming, gates, NCs, secret ban.
- `docs/plans/*_PLAN.md` — 12 plans, each with phases carrying DoD, test gate,
  deliverable patch and a list of non-goals:
  `CORE` C0–C10, `STATE` S0–S7, `EXEC` X0–X9, `CONSENSUS` B0–B10,
  `NET` N0–N8, `NODE` D0–D8, `WALLET` W0–W7, `ECOSYSTEM` E0–E9,
  `ECONOMICS` T0–T8, `SECURITY` H0–H9, `PERF` P0–P6, `OPS` O0–O9.

**At the time of this entry** Q1–Q7 were open; they were locked by the next
entry `[D0 locked / A0.1]` (we do not rewrite history).

**Deliberately not done:** no `Cargo.toml`, `Makefile`, CI, `patches/`,
`crates/`. All of that is the deliverable of phase C0, not a "small thing" to
add silently. Just like C0 in AuraLite-Emulator, the first patch changes
*only* the measurable foundation and documentation.

**External sources used in the plan:** Regulation (EU) 2023/1114 (MiCA) on
public offers of crypto-assets
[2](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32023R1114),
the white paper notification guidance
[3](https://www.amf-france.org/en/news-publications/depth/mica),
the final NIST FIPS 203/204/205 standards
[7](https://csrc.nist.gov/news/2024/postquantum-cryptography-fips-approved).
These are planning references, not legal advice.
