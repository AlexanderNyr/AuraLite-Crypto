# H2 closeout report — crypto review

Date: 2026-11-14. Gate (SECURITY_PLAN row H2): "50 crypto vectors +
side-channel checklist; NC: shared domain -> signature passes, red" —
**satisfied**.

- **Vectors**: 50 golden crypto vectors from the independent oracle
  `tools/gen_h2_vectors.py` (pure-Python RFC 8032, python blake3, from-spec
  `domain_message`): keygen, sign/verify byte-for-byte, wrong domain, cross
  chain, tampered payload, wrong key, high-S malleation, digest layout,
  edge payloads. `crates/aura-crypto/tests/h2_crypto.rs` asserts every
  expectation (`h2_vector_count_is_50` pins the count).
- **Malleability (D-H2-3)**: `verify_raw` rejects `S >= L` with the distinct
  `CryptoError::NonCanonicalSignature`. Review finding: the classic twin
  `(R, S+L)` satisfies the same verification equation; ed25519-dalek 2.2.0's
  default `check_scalar` rejects it, but its `legacy_compatibility` feature
  reopens malleation ("permits signature malleability" — dalek's own
  comment). The guard makes `S < L` our own invariant.
- **Side-channel checklist (D-H2-4)**: 10 machine-readable items
  (`### H2-SCxx`, `Claim/Owner/Evidence/Status`) pinned by
  `tools/fixtures/h2_review_checklist.json`, re-derived by
  `tools/check_h2_review.py`, which also greps the reviewed sources for each
  claim (constant-time compare, Zeroizing, Debug redaction, S<L guard, five
  domain tags, chain binding, length prefixes, hash context).
- **Post-quantum (D-H2-5)**: ML-DSA (FIPS 204) primary, SLH-DSA (FIPS 205)
  stateless backup; hybrid `Ed25519 || ML-DSA` migration planned for
  W1-FORMAT-v2 / Stage 2 (plan §4).
- **Named control** (shared_domain): `tools/nc_h2.py` NC-1 collapses every
  `Domain::tag()` to one tag and requires the h2 suite to go red with
  `H2: wrong-domain signature must be rejected`.
- **Mutation controls: 6/6 PASS** (shared domain, canonical-S guard, chain
  binding, checklist owner, checklist evidence, checklist status).

Receipts: `gates.txt`, `check.log`, `nc.log`, `inventory.txt`, `SHA256SUMS`.
Regenerate with `make evidence-h2`. Public project material only.
