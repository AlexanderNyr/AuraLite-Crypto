# W1 independent review brief — experimental testnet keystore

**Status: request for review, not a review report.** No external reviewer has
been engaged or approved this implementation through this document. Do not use
experimental wallet keys for funds. W1 remains open.

## Purpose and expected deliverables

Ask an independent reviewer, not the W1 implementation author or the agent who
prepared these tests, to examine the exact source snapshot, dependency lockfiles,
compiled configurations and candidate format. Ask for a written, attributable
report stating reviewer identity/affiliation, scope, exact snapshot identifier
and hashes, methods, environment, exclusions, dated findings with severity and
reproduction, and the disposition and retest of each material finding. Record
residual risks and limitations; do not infer a security certification merely
from a lack of findings. This is the W1 WIP review request, not a substitute for
the separate Stage-2 project-wide audit.

The owner and reviewer should agree on scope, timing, handling of discoveries,
responsible disclosure and acceptance criteria before the review. This brief is
not a contract or proof of independence.

## Inputs

- Candidate format: [w1-format-draft.md](w1-format-draft.md). It is not frozen.
- Implementation: `crates/aura-wallet/src/{keystore,derivation,secret}.rs`,
  `crates/aura-crypto/src/{ed25519,secret}.rs`, `crates/aura-tx/src/tx.rs`;
  filesystem behavior in the keystore module is part of the requested scope.
- Cryptographic dependencies and provenance: workspace `Cargo.toml`,
  `Cargo.lock`, `crates/aura-wallet/Cargo.toml`, `third_party/scrypt-w1/`
  including `PROVENANCE.md`, `third_party/unicode-normalization-15-1/` with
  `LOCAL-FORK.md`, and the offline provenance guards
  `tools/check_scrypt_fork.py` and `tools/check_unicode_15_1_fork.py` (each with
  its `tools/test_*_fork_guard.py` mutation test).
- Tests and independent examples: `crates/aura-wallet/tests/w1_keystore.rs`,
  `crates/aura-wallet/tests/fixtures/w1_*.json`, `tools/w1_*reference*.py`,
  `tools/nc_w1.py`, `tools/nc_w1_fuzz.py`, `fuzz/fuzz_targets/fuzz_keystore_*.rs`,
  `fuzz/fuzz_targets/fuzz_scrypt_differential.rs`,
  `fuzz/fuzz_targets/fuzz_recovery_metadata.rs`, and `fuzz/src/lib.rs`.
- Existing known limitations and local evidence: [WIP_SECURITY_AND_W1.md](WIP_SECURITY_AND_W1.md),
  [w1-preclose-checklist.md](w1-preclose-checklist.md) and
  [w1-fuzz-observation-2026-11-08.md](w1-fuzz-observation-2026-11-08.md).
  Historical evidence and committed checksums must remain unchanged.

An external patch or archive is only transport. Before reviewing, establish a
single immutable source snapshot, verify its digests and state whether the W1
prototype already present in `c749fc0` and earlier changes are included. The
local `AuraLite-W1-review.patch` contains only WIP changes after `c749fc0`; it
is **not** a complete patch from the W0 phase and is not proof of closure.

## Technical questions for the reviewer

1. **Derivation and recovery:** correctness of English BIP-39, PBKDF2 and
   SLIP-0010 hardened derivation, input validation, accepted testnet path,
   addresses, normalization profiles, provenance of the two Unicode tables,
   version-sensitive text and recovery metadata. Check downgrade, ambiguity,
   migration and mistaken key-selection risks. Test seed comparisons and avoid
   treating a constant-time byte primitive as proof of whole-function timing
   resistance.
2. **AEAD and format:** exact accepted v1/v2 schema, duplicate-key handling,
   canonical AAD across JSON parsers, AES-256-GCM nonce handling, cryptographic
   authentication before plaintext use, bound public metadata and profile
   markers, invalid encodings, ciphertext/tag corruption and downgrade paths.
   Consider tests for fully reauthenticated but semantically inconsistent data.
3. **KDFs and denial of service:** Argon2id and scrypt policy vs bounds,
   integer conversion/overflow, memory and CPU amplification across concurrent
   requests, weak-scrypt migration-only behavior and wrong-password handling.
   Audit the minimal zeroizing scrypt fork against upstream and its provenance.
   Confirm unsafe test methods/features cannot accidentally be used by a
   production integration; they remain callable when the feature is enabled.
4. **Secret handling:** best-effort zeroization of seed, mnemonic, passphrase,
   temporary buffers, KDF state and expanded cipher keys; compiler/runtime/OS
   copies, swaps, core dumps and failure/unwind paths; inadvertent logs and
   error output; timing and observable behavior. Document what is not assured.
5. **File operations:** Unix owner/mode checks, no-overwrite publication,
   symlink and race behavior, directory replacement, failure after publication,
   syncing and crash/power-loss semantics. Review hostile ancestors and same-UID
   attack limitations; examine separately the non-Unix and unsupported-platform
   behavior rather than inferring it from cross-compilation.
6. **Tests and evidence:** compare the parser matrix with actual accepted JSON;
   validate independent reference assumptions, negative-control quality and
   fuzz target bounds. Examine any counterexamples and preserved fuzz artifacts.
   Verify that test-only parameters cannot be mistaken for production policy.

## Suggested local verification commands

Use an isolated machine with no real credentials, strict resource limits, and
review all downloaded dependencies before execution. The mutation controls
modify source temporarily; never run them alongside builds, fuzzing, editors or
other mutation controls. Install Rust 1.99.1 and pinned optional Python test
dependencies from `tools/w1-reference-requirements.txt`.

```sh
make test-w1
make test-w1-interop
make nc-w1
make test
python3 tools/test_w1_fuzz_campaign.py
```

Run wallet fuzzing only with public test corpus and its explicitly insecure
fuzz-only feature; see `tools/w1_fuzz_campaign.py --help`. Version-2 local
campaign directories record unkeyed SHA-256 manifests for tracked sources,
initial seed corpora and evolved corpora and can be checked for internal
consistency (and optional checkout match) via `tools/verify_w1_fuzz_evidence.py`;
this check is not a signature, external audit or proof of execution time. A
bounded passing run is not a substitute for agreed sustained fuzzing or
independent review.

## Review completion and release discipline

A reviewer report must refer to the final immutable snapshot; if files change
after review, repeat or explicitly scope the reviewer retest. Do not update
`WALLET_PLAN`, a W1 receipt, or `docs/STATUS.md` to a completed state based
solely on this brief. The owner must separately decide the candidate-format
freeze and verify W1 phase artifacts and the intended gate policy.
