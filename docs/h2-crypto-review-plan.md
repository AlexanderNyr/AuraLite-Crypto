# H2 — Crypto review plan (side-channel checklist + PQ plan)

Status: 🚧 IN PROGRESS — H2
Owner: core maintainer
Date: 2026-11-14
Gate: `50 crypto vectors + side-channel checklist`
Named control: `shared domain -> signature passes, red`

## 1. Scope and DoD

SECURITY_PLAN row H2 requires an independent crypto review covering:
constant-time behaviour, secret zeroization, domain separation, signature
malleability, and a post-quantum plan. This phase delivers:

- **D-H2-1 — 50 golden crypto vectors** (`crates/aura-crypto/tests/fixtures/h2_vectors.txt`):
  keygen, sign/verify byte-for-byte, and the negative families (wrong domain,
  cross chain, tampered payload, wrong key, high-S malleability), plus digest
  layout vectors and edge payloads. Oracle: `tools/gen_h2_vectors.py`
  (independent: pure-Python RFC 8032 from `tools/gen_c4_vectors.py`, the
  python `blake3` binding, and a from-spec `domain_message` assembler).
- **D-H2-2 — signed-digest layout pin.** `domain_message` =
  `HASH_CONTEXT || u64le(prefix_len) || prefix || u64le(payload_len) || payload`,
  `prefix = chain_id || 0x00 || domain_tag || 0x00`. Digest vectors v41–v45
  pin `hash_domain` byte-for-byte; a change of `HASH_CONTEXT` is a protocol
  change (C3 keeps its NC on that constant).
- **D-H2-3 — malleability check.** `verify_raw` rejects non-canonical `S`
  (`S >= L`, RFC 8032 §5.1.7) *before* verification, with the distinct
  `CryptoError::NonCanonicalSignature` error. Review finding: the classic
  malleation is the twin `(R, S+L)` (`S' ≡ S mod L`, so the twin satisfies
  the same verification equation). ed25519-dalek 2.2.0's default
  `check_scalar` (`Scalar::from_canonical_bytes`) already rejects `S >= L`,
  but its `legacy_compatibility` feature relaxes the bound to `S < 2^253` and
  reopens malleation — dalek's own comment marks that path "permits signature
  malleability". The explicit guard makes `S < L` an invariant of *our* API,
  independent of dependency features or swaps (a supply-chain-shaped threat),
  and gives gates/callers a distinguishable error. Proven by the `high_s`
  vectors (v36–v40) and the unit test in `ed25519.rs`.
- **D-H2-4 — side-channel checklist** (below): machine-readable
  `### H2-SCxx` items with `Claim/Owner/Evidence/Status` fields, pinned by
  the oracle to `tools/fixtures/h2_review_checklist.json` and re-derived by
  `tools/check_h2_review.py`, which also greps the reviewed sources for each
  claim.
- **D-H2-5 — post-quantum plan** (below): ML-DSA (FIPS 204) as the signature
  upgrade, SLH-DSA (FIPS 205) as the stateless backup.
- **D-H2-6 — mutation controls** (`tools/nc_h2.py`, 6): see §4.
- **D-H2-7 — evidence** in `docs/evidence/h2/` (`make evidence-h2`).

## 2. The 50 vectors

Seeds: `seed_i = SHA-256("AuraLite-Crypto/h2-crypto-vectors-v1/seed/" || u32le(i))`.
Chains: `aura-test-1`, `aura-test-2`. Domains: the five `Domain::tag()` tags.

| ids | kind | count | expectation |
| --- | --- | --- | --- |
| v01–v10 | `sign_verify` | 10 | pubkey/digest/sig byte-for-byte; `verify` accepts |
| v11–v15 | `keygen` | 5 | `ed25519_public_key(seed)` byte-for-byte |
| v16–v20 | `wrong_domain` | 5 | same signature under another domain tag -> reject |
| v21–v25 | `cross_chain` | 5 | same signature under another chain -> reject |
| v26–v30 | `tampered_payload` | 5 | one flipped payload byte -> reject |
| v31–v35 | `wrong_key` | 5 | another seed's public key -> reject |
| v36–v40 | `high_s` | 5 | `(R, S+L)` twin -> `NonCanonicalSignature` |
| v41–v45 | `digest` | 5 | `hash_domain` byte-for-byte (layout pin) |
| v46–v50 | `edge` | 5 | empty/`0x00`/`0xff`/mixed/1 KiB payloads sign+verify |

`high_s` twins use `S' = S + L` (no wraparound: `S < L < 2^252`), the classic
ed25519 malleation. The suite asserts `verify_raw` rejects the twin; a unit
test in `crates/aura-crypto/src/ed25519.rs` additionally demonstrates that
ed25519-dalek's ZIP215 verify *accepts* it (the vulnerability is real).

## 3. Side-channel checklist

### H2-SC01 — Constant-time comparison for secret material
- Claim: Secret comparisons go through subtle::ConstantTimeEq via constant_time_eq; no early-exit equality on secret bytes.
- Owner: crypto reviewer
- Evidence: crates/aura-crypto/src/secret.rs
- Status: verified

### H2-SC02 — Secret material zeroized on drop
- Claim: SecretBytes wraps Zeroizing<[u8; 32]> so key material is wiped when it leaves scope.
- Owner: crypto reviewer
- Evidence: crates/aura-crypto/src/secret.rs
- Status: verified

### H2-SC03 — Debug redaction for secrets
- Claim: Debug for SecretBytes prints [REDACTED] and no Display impl can print secret bytes.
- Owner: crypto reviewer
- Evidence: crates/aura-crypto/src/secret.rs
- Status: verified

### H2-SC04 — Signature malleability check (S < L)
- Claim: verify_raw rejects non-canonical S values (S >= L) before verification with a distinct NonCanonicalSignature error, independent of dependency-internal checks.
- Owner: core maintainer
- Evidence: crates/aura-crypto/src/ed25519.rs
- Status: verified

### H2-SC05 — Domain separation covers all five domains
- Claim: Domain::tag() assigns a distinct stable tag to Tx, Vote, Block, Contract and Genesis; cross-domain signatures are rejected.
- Owner: core maintainer
- Evidence: crates/aura-types/src/primitives.rs
- Status: verified

### H2-SC06 — Chain binding in the signed digest
- Claim: hash_domain binds chain_id into the signed digest; a signature made for one chain fails on another.
- Owner: core maintainer
- Evidence: crates/aura-crypto/src/hash.rs
- Status: verified

### H2-SC07 — Unambiguous length-prefixed message layout
- Claim: domain_message encodes prefix and payload lengths as u64 little-endian before each segment; concatenation is unambiguous.
- Owner: core maintainer
- Evidence: crates/aura-crypto/src/hash.rs
- Status: verified

### H2-SC08 — Versioned hash context is a protocol constant
- Claim: HASH_CONTEXT is pinned at AuraLite-Crypto/crypto-v1 and changing it is a protocol change requiring a new phase.
- Owner: core maintainer
- Evidence: crates/aura-crypto/src/hash.rs
- Status: verified

### H2-SC09 — Post-quantum primary: ML-DSA (FIPS 204)
- Claim: ML-DSA is the designated PQ signature upgrade; the hybrid migration plan is recorded in this document.
- Owner: core maintainer
- Evidence: docs/h2-crypto-review-plan.md
- Status: planned

### H2-SC10 — Post-quantum backup: SLH-DSA (FIPS 205)
- Claim: SLH-DSA is the designated stateless hash-based backup if ML-DSA is weakened; the fallback plan is recorded in this document.
- Owner: core maintainer
- Evidence: docs/h2-crypto-review-plan.md
- Status: planned

## 4. Post-quantum plan (D-H2-5)

Current primitives and quantum exposure:

- **Signatures (Ed25519, RFC 8032; secp256k1 compat)** — broken by Shor's
  algorithm. Every long-lived public key becomes forgeable once a
  cryptographically relevant quantum computer exists. This is the urgent part.
- **Hashes (BLAKE3-256, SHA-256 for seeds)** — Grover only halves preimage
  security (256-bit -> 128-bit). Acceptable long-term; no urgent migration.

Plan:

1. **ML-DSA (FIPS 204) as the primary PQ signature.** Standardized lattice
   (Dilithium) signature with the broadest implementation ecosystem. Adopted
   as the designated upgrade for `Domain::Tx/Vote/Block/Contract/Genesis`
   signatures. Public keys and signatures are large (2.6 KB / 4.6 KB for
   ML-DSA-65), which is acceptable for consensus artifacts and is why the
   migration is a codec change (`CODEC_VERSION` bump), not a drop-in.
2. **SLH-DSA (FIPS 205) as the stateless backup.** Hash-based (SPHINCS+),
   conservative security assumptions, no lattice structure. Kept as the
   designated fallback if ML-DSA is weakened (as happened with earlier
   Rainbow/GeMSS candidates). Signatures are ~30 KB: reserved for genesis
   certificates, root-of-trust anchors and release signatures, where size is
   cheap and assumption diversity is priceless.
3. **Hybrid migration (Stage 2).** Mode H-PQ-hybrid: sign with
   `Ed25519 || ML-DSA` (composite), verify both, until the network has
   rehearsed on testnet. Wire format lands in W1-FORMAT-v2 (currently frozen
   as v1 for testnet per the W1 owner decision); the domain separation
   context bumps (`crypto-v2`) and every signature version is carried in the
   codec. Rollback window: the composite keeps the classical half until the
   PQ half is proven in production.
4. **Key inventory discipline.** No long-lived classical-only keys past the
   hybrid activation height; release/genesis keys move to SLH-DSA first.
5. **Status: planned.** Nothing in this phase changes the wire; the vectors
   and checklist pin today's primitives so the migration has a byte-exact
   baseline to diff against.

## 5. Gates and controls

- `make test` runs `tools/check_h2_review.py` (checklist gate) and, through
  `cargo test`, `crates/aura-crypto/tests/h2_crypto.rs` (the 50 vectors).
  Failure output: `error: crypto-review: …` / assertion messages
  `H2: …`, exit 1.
- `make check-h2` runs the checklist gate alone; `make gen-h2-vectors`
  regenerates the fixtures (deterministic; `--check` proves no drift).
- Negative controls (`tools/nc_h2.py`, 6):
  **NC-1 (named) shared domain -> signature passes, red** (all `Domain::tag()`
  tags collapse to one -> `H2: wrong-domain signature must be rejected`),
  NC-2 remove the S<L check -> `H2: high-S signature must be rejected`,
  NC-3 drop the chain binding -> `H2: cross-chain digest must be chain-bound`,
  NC-4 delete a checklist Owner field -> `error: checklist item H2-SC07: missing owner`,
  NC-5 delete a checklist Evidence field -> `error: checklist item H2-SC08: missing evidence`,
  NC-6 delete a checklist Status field -> `error: checklist item H2-SC04: missing status`.
- Evidence `docs/evidence/h2/`: `report.md`, `gates.txt`, `check.log`,
  `nc.log`, `inventory.txt` (cumulative TOTAL: 442), `SHA256SUMS`;
  regenerate with `make evidence-h2`.
