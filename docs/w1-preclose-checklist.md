# W1 pre-close checklist — work in progress

**W1 remains OPEN.** This is a review handoff, not a phase receipt, audit,
format freeze, or approval for real funds. The requirements below combine the
W1 plan with the repository's own WIP security/format commitments. The local
observations are tied to the current experimental tree and its recorded logs;
they must not be attributed to a different commit or to hosted CI.

## Evidence available locally

| Item | Current observation | Limitation |
|---|---|---|
| BIP-39 / SLIP-0010 testnet derivation | Official and independent reference vectors pass in `make test-w1` | Shared testnet coin type 1; not a mainnet allocation or literal historical BIP-32/44 path |
| Keystore encryption / profile metadata | Python/Rust AES-GCM / Argon2id / scrypt vectors and profile-bound recovery checks pass | Prototype format, public test credentials; not a security certification |
| 1,000 wrong passwords | W1 regression test passes | The repeated test uses the explicitly weak test-only KDF to keep the campaign bounded; normal KDF policy is checked separately |
| Strict parser | 71 Python/Rust parse-only cases pass; duplicate-key, extra-field and KDF-limit bypass mutation controls turn the named test red | Accepted parsing is not authentication; the matrix exercises default-feature parser policy |
| Mutation testing | `make nc-w1`: 52/52 wallet and 15/15 fuzz-oracle controls, including recovery marker size, duplicate, extra-field and error-classification bypasses, post-GCM case-insensitive account comparison bypasses, missing-AAD `seed_derivation`/`kdf`, parser extra-field/duplicate/oversize/profile-accessor bypasses, recovery-oracle escaped-duplicate/raw-text-profile-cross-check bypasses and scrypt-fork divergence detection | Negative controls test detection by selected tests, not completeness or resistance to all attacks |
| Post-GCM account canonicalization | Uppercase-hex v2 header with canonical lowercase account decrypts in Python and Rust; changing `id`/`salt`/`nonce` hex case breaks AAD while ciphertext case is inert; valid-GCM uppercase `public_key`/`address` and inconsistent legacy-scrypt account documents fail closed | Exact-match comparison only; public test credentials; not a security certification |
| Unicode 15.1 fork provenance | `tools/check_unicode_15_1_fork.py` + mutation-tested guard (run by `make test-w1`): 23 unmodified files byte-equal to crates.io `unicode-normalization` 0.1.23, the two adapted files reverse to exact upstream digests, extra files/symlinks rejected, workspace bindings pinned | Provenance only, not Unicode correctness or cryptographic review |
| Project gate | Latest `make test`: workspace 756 passed, 0 failed, 7 ignored; fmt, Clippy, P2P controls, docs, deps and close-out audit passed | Local run, not hosted CI or every optional backend; the negative controls deliberately create failing test runs |
| Recovery comparison | Fixed-length recovered and stored seeds use `aura_crypto::constant_time_eq`; the bypass mutation is detected | No claim that the entire recovery process is constant-time |
| Campaign infrastructure | Sequential ASan runner (`format_version: 2`), `verify_w1_fuzz_evidence.py` self-consistency/checkout check and 12 fail-closed unit tests pass in `make test-w1` | Unkeyed SHA-256 self-consistency only; not a signed receipt, wall-clock proof or independent audit |
| CORE shard | `make test-core` passed, including 5,000 portable fuzz iterations for each of six core targets | Not a substitute for wallet-specific or long-running fuzzing |
| Portable core fuzz | `make fuzz-smoke`: 24,327,391 iterations, 0 crashes, 60 seconds total across six targets | Portable core harness, not a long-running wallet-specific or sanitizer campaign |
| Wallet sanitizer fuzz | Bounded sequential 301-second-per-target ASan runs (latest `w1-campaign-v2-oracles`, all four targets, no crashes, recorded source hashes and corpora) | Weak test-only KDF for decrypt and tiny scrypt parameters; see [observation](w1-fuzz-extended-observation.md); not H3's 24-hour campaign |
| Recovery metadata schema | 37 independent Python/Rust public cases (`w1_recovery_metadata_reference.py` + guard) plus 25 Rust cases and 4096/4097-byte UTF-8 boundaries; the same matrix and boundaries run in the `fuzz_recovery_metadata` driver unit test with raw-text profile cross-checks; 6 focused mutation controls turn named tests red | Parse/serialize conformance only; not an independent external review or authentication of recovery metadata |
| Recovery metadata sanitizer fuzz | Latest sustained version-2 run (`w1-campaign-v2-oracles`, 301 s per target): recovery parser 12,689,274 iterations, no artifacts; `verify_w1_fuzz_evidence.py --check-checkout` agrees with the live checkout | Limited duration and transformations; see [observation](w1-fuzz-extended-observation.md), not proof of complete rejection behavior |
| Platform compile | W1 cross-checks passed for FreeBSD, Redox and Windows GNU | Compile only, no execution or filesystem semantics validated on those platforms |
| Release policy | The CI's named release-mode normal-KDF-policy test passed with the insecure test feature enabled | The feature still exposes intentionally unsafe test-only methods and must never be enabled for production |

## Explicitly incomplete or awaiting a decision

1. **Format review and freeze.** [Candidate draft](w1-format-draft.md) is not
   approved. Review exact schema, UTF-8/JSON edge cases and AAD bytes, KDF
   resource/policy bounds, normalization profiles and provenance, recovery,
   migrations and post-authentication account binding. Freeze only a specific
   version and exact artifact after the owner/review process agrees; record
   compatibility and change-control decisions. No silent normalization upgrade.
2. **Independent security review.** Obtain a real external reviewer and a
   traceable review of implementation, tests, dependencies, custom scrypt fork,
   side channels, filesystem durability and threat assumptions. The assistant's
   review and local Python references are not independent external review. Triage
   findings and rerun affected gates. The project-wide Stage-2 external audit
   remains a separate later milestone; this item is the explicit W1 WIP review
   requirement, not a claim to have completed that later audit.
3. **Longer fuzzing.** Agree and run the W1 campaign with duration, corpus,
   target/feature matrix, resource limits, crash triage, toolchain and logs.
   The 301-second-per-target observations are only bounded checks. H3's separate
   plan calls for a 24-hour total campaign across broader targets; do not label
   the present observations as meeting it.
4. **Phase artifacts and status.** Once all agreed criteria are met, prepare a
   real numbered W1 phase patch against the correct predecessor, a phase receipt
   with gate logs/NC and checksums, and update plan, STATUS and CHANGELOG in the
   same phase. Preserve the historical evidence and source-drift ledger. The
   current review patch is a diff on top of upstream commit `125de72`
   (`W1 update 2`), which itself records the earlier W1 increments after
   `c749fc0`; the `c749fc0` W1 prototype and unrelated security fixes are
   already committed upstream. It is not the complete W1-from-W0 phase patch
   and must not be called `0059` or treated as closure.
5. **Other mandatory gates.** `make test-sanitize` and `make test-determinism`
   currently print explicit SKIP messages; do not report them as passed. The
   `cargo deny` gate is planned for H1 and was not run in this environment.
   Decide which phase-specific and later-phase gates are prerequisites for the
   intended W1 closure, without rewriting a skip into a pass.

## Local log locations

- `/home/user/w1-gate-v2-final.log`, `/home/user/w1-nc-v2-final.log`, and
  `/home/user/w1-interop-v2-final.log` — earlier W1 tests (including 37
  recovery-marker Python/Rust cases and 12 campaign/verifier unit tests), 50
  wallet plus 2 fuzz-oracle mutation controls, and Python/Rust interop checks.
  The current counts are 52 wallet plus 15 fuzz-oracle controls after the
  post-GCM canonicalization, hex-case AAD, parser-oracle and
  recovery/scrypt-oracle additions. Latest final-snapshot runs are
  `/home/user/w1-nc-oracles.log`, `/home/user/w1-gate-oracles.log`,
  `/home/user/w1-interop-oracles.log` and `/home/user/w1-make-test-oracles.log`.
- `/home/user/w1-make-test-schema.log` — earlier project-wide gate.
- `/home/user/w1-make-test-recovery-final.log` — later run passed preceding
  stages but failed the close-out audit because the `fuzz/Cargo.toml` current
  drift hash had not yet been updated. The ledger was corrected without changing
  historical receipts; `/home/user/w1-make-test-ledger-final.log` and
  `/home/user/w1-make-test-metadata-final.log` record passing reruns, and the
  latest passing full run is `/home/user/w1-make-test-v2-final.log`.
- `/home/user/w1-campaign-extended/`, `/home/user/w1-campaign-recovery/`,
  `/home/user/w1-campaign-metadata-metamorphic/`, and
  `/home/user/w1-campaign-v2-verified/` — recorded sequential ASan campaigns;
  the last directory uses `format_version: 2` corpus manifests verified against
  the current checkout.
- `/home/user/w1-portable-fuzz-smoke.log` and `/home/user/w1-test-core.log` —
  portable 60-second core smoke and CORE shard.
- `/home/user/w1-profile-interop-final.log` — previous matching interop run;
  schema matrix changes did not change the interop implementation or fixtures.
- `/home/user/AuraLite-W1-test-evidence.tar.gz` — collected local logs and
  fuzz output, not a signed artifact or a phase receipt.

Local paths and installed toolchains/caches may not survive environment resets;
retain the archive separately if the evidence must be reviewed later.
