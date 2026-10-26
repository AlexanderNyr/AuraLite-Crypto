# Implementation status — AuraLite-Crypto

> Updated **in the same patch** as the phase's code (rule D9).
> Snapshot date: **2026-10-26**, tree: "C10 landed — CORE_PLAN closed,
> Stage 1 in progress". See [`CLOSEOUT_AUDIT.md`](CLOSEOUT_AUDIT.md).

## Locked decisions (2026-10-25)

| Parameter | Value |
|---|---|
| Project / network | `AuraLite-Crypto` / `AuraLite-Crypto Network` |
| Ticker | `ALCRY` (18 decimals, `aalcry`) — see [`TICKER_AUDIT.md`](TICKER_AUDIT.md) |
| chain-id Stage 1 | `auralite-test-1` (mainnet `auralite-1` reserved) |
| Consensus | BFT-PoS, 2f+1 QC, VRF |
| VM | WASM (`wasmtime`) + our ABI; EVM optional in X9 |
| Current stage | **Stage 1 = M0–M2** (core → devnet → public testnet) |
| ⏸ Stage 2 | mainnet, white paper, external audit, DeFi, bridge — separate decision |
| Docs language | English (owner request, 2026-10-25) |

**Phase ledger:** C0 ✅ (`patches/0001-C0-baseline-workspace.patch`),
C1 ✅ (`patches/0002-C1-types.patch`), C2 ✅
(`patches/0003-C2-codec.patch`) and C3 ✅
(`patches/0004-C3-crypto.patch`) and C4 ✅
(`patches/0005-C4-tx-format.patch`) and C5 ✅
(`patches/0006-C5-block-header.patch`) and C6 ✅
(`patches/0007-C6-merkle-proofs.patch`) and C7 ✅
(`patches/0008-C7-account-model.patch`) and C8 ✅
(`patches/0009-C8-genesis.patch`) and C9 ✅
(`patches/0010-C9-test-rig.patch`) and C10 ✅
(`patches/0011-C10-close-out.patch`) done — **CORE_PLAN is complete**. Next
in dependency order: `STATE_PLAN` S0. No other phase has started.

## Maturity level

| Level | Definition | Now |
|---|---|---|
| L0 | Docs/plan, no code | ✅ done at C0 |
| L1 | Core libraries + test rig, no network | ✅ **we are here** (CORE C0–C10; S/X phases still open) |
| L2 | Devnet: 4 validators in one process/host | ❌ |
| L3 | Public testnet with external validators | ❌ (Stage 1 target) |
| L4 | Mainnet candidate: audit, freeze, white paper | ⏸ Stage 2 |
| L5 | Mainnet + live application ecosystem | ⏸ Stage 2 |

L3 is the ceiling of **Stage 1**. L4–L5 belong to ⏸ Stage 2 (decision Q5).

## Working now

- `make test` is green: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (**109 tests**),
  the docs drift guard, the dependency gate, the vector inventory, the
  test-rig wiring check and the plan↔tree close-out audit. Receipts:
  `docs/evidence/c0-baseline.txt`, `docs/evidence/c0/nc.log`,
  `docs/evidence/c1/gates.txt`, `docs/evidence/c1/nc.log`,
  `docs/evidence/c2/gates.txt`, `docs/evidence/c2/nc.log`,
  `docs/evidence/c3/gates.txt`, `docs/evidence/c3/nc.log`,
  `docs/evidence/c4/gates.txt`, `docs/evidence/c4/nc.log`,
  `docs/evidence/c5/gates.txt`, `docs/evidence/c5/nc.log`,
  `docs/evidence/c6/gates.txt`, `docs/evidence/c6/nc.log`,
  `docs/evidence/c7/gates.txt`, `docs/evidence/c7/nc.log`,
  `docs/evidence/c8/gates.txt`, `docs/evidence/c8/nc.log`,
  `docs/evidence/c9/gates.txt`, `docs/evidence/c9/nc.log`,
  `docs/evidence/c10/gates.txt`, `docs/evidence/c10/nc.log`,
  `docs/evidence/c10/closeout.txt`.
- `aura-types` exports the locked D0 identity (`auralite-test-1`, `ALCRY`,
  18 decimals, `aura`, `bft-pos`, `wasm`, v1) **and** the C1 canonical types:
  `Hash`, bech32 `Address`, `Amount`/`Denom`, `ChainId`, `Nonce`, `Height`,
  `Gas`, `Domain`, `CanonicalMap`. Still **zero external dependencies**.
- C1 addresses are verified against `tools/gen_c1_vectors.py`, an independent
  Python BIP-173 encoder; 3 C1 negative controls pin the checksum constant,
  the version byte and the golden fixture itself.
- C2 `aura-codec` is verified against `tools/gen_c2_vectors.py`, an independent
  Python implementation of the wire format (3 canonical records + 12
  non-canonical vectors). `decode_canonical()` re-encodes and requires byte
  equality. 4 C2 negative controls pin the version, the golden fixture, the
  trailing-byte guard and the non-minimal-varint guard.
- C3 `aura-crypto` implements BLAKE3 (`pure` feature), Ed25519 (RFC 8032),
  secp256k1 deterministic ECDSA (RFC 6979), domain-separated hashing/signing,
  constant-time equality and zeroizing secret storage. Verified against
  official BLAKE3 vectors, RFC 8032 §7.1 and a pure-Python secp256k1
  reference (OpenSSL-verified). 5 C3 negative controls pin the context, two
  fixtures, the verifier and the dependency allowlist.
- C10 closes `CORE_PLAN`: `tools/audit_closeout.py` runs inside
  `make test` and requires, for every ✅ phase row, a numbered patch, a
  `### Cx receipt`, a CHANGELOG entry, an NC driver/log and verified
  evidence; it also checks the 56-NC total, the spec index and that no
  deferred phase is marked done. `docs/spec/FORMAT_INDEX.md` indexes all
  format specs and governance files; `docs/CLOSEOUT_AUDIT.md` is the honest
  maturity assessment; `docs/evidence/c10/closeout.txt` lists all 56 NCs.
  6 C10 negative controls remove receipts/specs/checksums and require red.
- C9 adds the **15th crate**, `aura-testkit`: 512 new golden vectors
  (**956 total** across 9 fixtures), 5 `proptest` properties (256 cases
  each), a deterministic portable fuzz harness over six decoders and four
  `libfuzzer-sys` source targets in `fuzz/` (excluded from the workspace).
  `make test-core` and `make fuzz-smoke` are now real gates; the 60 s
  campaign must end with zero panics. `tools/count_vectors.py` and
  `tools/check_rig.py` run inside `make test`. 6 C9 NCs inject two **real**
  production bugs and prove the properties catch them. Spec:
  `docs/spec/test-rig.md`. 13 new dev-only external crates (proptest tree).
- C8 adds the **14th crate**, `aura-genesis`, plus the first new external
  dependency since C3 (`serde_json =1.0.154`, config parsing only).
  Strict JSON schema → canonical binary document → `genesis_hash`,
  `validator_set_hash` and `allocation_root` (C6 tree over C7 accounts).
  `make genesis-hash` and the `aura genesis` CLI (`hash`/`verify`/`show`)
  agree with independent Python vectors. Spec:
  `docs/spec/genesis-format.md`; sample: `docs/spec/genesis-testnet.json`.
  DoD: 3 JSON representations → one hash; NC: validator order changes it.
  6 C8 negative controls. Still no genesis **block**, no JMT `state_root`.
- C7 gives `aura-state` its **first real API** (no new crate, no new
  dependency): `Account` (88 canonical bytes, checked arithmetic) and
  `StateKey`/`KeySpace` with 5 fixed-length namespaces. `0x00` and
  `0x06`–`0xFF` are reserved; `assert_contract_may_write` confines a contract
  to its own `0x03 || address || slot` prefix. `StateKey::digest` is the
  domain-separated JMT path for S1. Spec: `docs/spec/account-model.md`;
  107 independent fixture lines; DoD gates: 1000 addresses → 5000 distinct
  keys/digests, all 256 prefixes checked. 6 C7 negative controls. Still no
  state tree, root or persistence — those are S1–S7.
- C6 added compact Merkle proofs inside `aura-block` (no new crate, no new
  dependency): tx and event inclusion proofs, `EVENT_LEAF_CONTEXT` domain
  separation, `verify_proof`/`verify_tx_inclusion`/`verify_event_inclusion`,
  and a C5↔C6 shared tree implementation. Spec:
  `docs/spec/merkle-proof-format.md`; 34 independent proofs + 11 rejection
  vectors; the 1000-leaf DoD gate verifies all proofs and rejects one
  substituted leaf. 6 C6 negative controls.
- C5 added the **13th crate**, `aura-block`, with no new external
  dependencies: `Header`/`Body`, binary BLAKE3 `tx_root`, `block_hash`,
  canonical `(signer, nonce)` ordering and replay-separated chain-id in the
  header (C5 amends `ARCHITECTURE.md` §1). `state_root`,
  `validator_set_hash` and `qc` are caller-supplied — not computed state.
  Spec: `docs/spec/block-format.md`; 4 canonical blocks + 9 rejection
  vectors from `tools/gen_c5_vectors.py`; 6 C5 negative controls.
- C4 added the **12th crate**, `aura-tx`, with no new external dependencies.
  It fixes `Tx = UnsignedTx || Signature`, canonical `signing_bytes`,
  signer↔public-key binding, txid and replay protection. Spec:
  `docs/spec/tx-format.md`; 5 canonical + 9 rejection golden vectors from
  `tools/gen_c4_vectors.py`; 6 C4 negative controls (contexts, fixture,
  replay, binding, verification).
- **Dependencies:** 71 external crates, all with exact `=x.y.z` pins,
  crates.io sources and sha256 checksums. The C3 baseline
  (`docs/spec/c3-dependency-allowlist.txt`) is **frozen** historical evidence;
  C8 records its +4 delta in `docs/spec/c8-dependency-addendum.txt` and C9
  its +13 dev-only delta in `docs/spec/c9-dependency-addendum.txt`.
  `make check-deps` compares `Cargo.lock` against baseline + addenda.
  `cargo deny` (licences/advisories) is still owed to H1.
  7 placeholder crates compile but expose **no API** (an honest empty crate,
  not a fake `Ok(())`). CI runs the same commands as `make test`.
- Still absent: consensus, VM, P2P, node, RPC, wallet, explorer, contracts.
  Present and agreed: north star, 12 plans, 112 phase rows, contribution rules,
  D0 decisions, ticker audit, architecture, and the plan for a threat model
  (`SECURITY_PLAN` H0 — not yet written as a document).

## Measured numbers (2026-10-26)

```
find crates -name Cargo.toml       -> 15 crates (+1 workspace root)
find . -name '*.rs' -not -path './target/*' -> 63 files (incl. fuzz/)
ls docs/plans/*_PLAN.md           -> 12 plans / 112 phase rows
cargo test --workspace            -> 109 passed; 0 failed
make test                         -> ALL GATES GREEN (incl. close-out audit)
make vector-inventory             -> 956 vectors >= 200
make audit-closeout               -> every ✅ has a receipt
make fuzz-smoke                   -> 60 s, 0 crashes
make nc-all                       -> 2/2+3/3+4/4+5/5+6/6*7 = 56 NC PASS
make check-deps                   -> 71 external crates allowlisted
validators / blocks / tx          -> 0 / 0 / 0
```

Pre-C0 baseline and post-C0/C1/C2/C3/C4/C5/C6/C7/C8/C9/C10 numbers with raw
command output are in `docs/evidence/c0-baseline.txt`,
`docs/evidence/c1/gates.txt`, `docs/evidence/c2/gates.txt`,
`docs/evidence/c3/gates.txt`, `docs/evidence/c4/gates.txt`,
`docs/evidence/c5/gates.txt`, `docs/evidence/c6/gates.txt`,
`docs/evidence/c7/gates.txt`, `docs/evidence/c8/gates.txt`,
`docs/evidence/c9/gates.txt` and `docs/evidence/c10/gates.txt`.

## What is missing (and must not be lost)

- No genesis **block**/execution, no state tree / `state_root` (S1), no
  VM (X1), no Miri/ASan/UBSan or 24 h libFuzzer campaign (H3), no state-root
  determinism gate (X7), no BLS aggregation (B2), no PQ migration (H2), no
  `cargo deny` (H1). M0 still needs S0–S2 and X0–X2.
- No consensus, no P2P, no state, no VM.
- No wallet, explorer, contracts, token.
- No audit, no bug bounty, no legal opinion.
- No users and no validators.

Each item above becomes a "✅ with receipt" line only in the patch of its
phase. If a line appears without a receipt, that is a documentation defect and
is fixed by a separate NC (following the example of `MATURITY_AUDIT.md` in
AuraLite-OS).

## How to read plan statuses

| Label | Meaning |
|---|---|
| 📋 PLAN | written, not started |
| 🚧 IN PROGRESS | open phases remain, `make test` green at the last closed one |
| ✅ CLOSED | all phases closed, receipts, NCs, CHANGELOG line present |
| ⚠️ AUDIT | external check found a plan/tree mismatch |
| ⛔ NO-GO | measurement showed the approach does not work (like PERF P4 in the Emulator) |

## Target platforms (goal, not fact)

Linux x86_64 (primary), macOS aarch64 (development), Docker (validators),
WASM (browser light client), later AuraLite OS as a light client
(`ECOSYSTEM_PLAN` E9). None of this has been verified on any platform yet.
