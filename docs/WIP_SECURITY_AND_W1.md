# Review fixes and W1 — work in progress

This branch is **not production-ready**. Passing unit/integration tests does not establish consensus safety, wallet interoperability, or a completed W1 phase. Historical phase receipts and their checksums are intentionally not rewritten.

## Implemented changes under review

* QC aggregation verifies individual BLS signatures, membership, uniqueness and the exact common vote payload; finality accepts only precommit certificates for known blocks with matching view/round. The 3-chain check requires parent/child linkage, adjacent rounds and a common view. Duplicate block replacement, repeated finalization and orphan/fork ancestry are rejected locally. The proposal harness signs the block commitment, validates epoch/view/round and tracks votes in memory. This is not a complete BFT safety proof.
* The devnet certificate commits to a block proposal instead of a bare round identifier. Read-only previews are compared and quorum votes collected before pipelines advance; a changed preview is rejected. The local pipeline still self-finalizes with a one-validator QC, separate node writes are not crash-atomic, and retries/catch-up are incomplete. These changes do not make devnet distributed finality safe.
* P2P wire version 2 uses signed ephemeral X25519, HKDF-SHA256 and AES-256-GCM, rejects all-zero DH results, and has directional keys and monotonic nonces. This is a custom handshake, **not** a formal implementation of the Noise protocol; independent security review is required. V1 and V2 peers cannot interoperate.
* A P2P review found and reproduced a permanent peer-ban DoS: inbound ID/public-key mismatch, invalid inbound/outbound handshake signatures, and expected-peer substitution could auto-ban an attacker-chosen `claimed_peer_id` before signature proof. The WIP fix removes automatic bans for these handshake failures, checks the existing peer ban list only after a valid transcript signature, and rejects even authenticated expected-ID mismatch without auto-banning (an unrelated endpoint may be honest). No IP ban is inferred. Five loopback regressions pass, and four mutation controls reintroducing the vulnerable behaviors go red. The historical N1 receipt still records the earlier, flawed behavior and remains unchanged; the legacy `evidence-n1` writer now refuses to rewrite that receipt or its ancestor manifests. These new local tests are not independent review or production evidence.
* A separate N6 review found a pre-crypto BanList/rate-bucket poisoning defect in `DosDefenceEngine::ingest`: it trusted the unauthenticated `IngressEnvelope.sender` before verifying the signer key/signature, so a crafted frame could ban a victim PeerId. The API now requires a separate `authenticated_peer` from the completed transport session; pre-crypto bans, budgets, and strikes use that source, invalid signatures ban only that source, and chain-ID bans occur only after the claimed signer signature verifies. Four focused regression tests cover claimed-ID ban, message-rate bucket poisoning, signature-budget poisoning, invalid-signature attribution, and wrong-chain ordering (some assertions share a test). `make test-n6` passes 67 library + 1 integration test; `make nc-n6` passes the unchanged six historical controls plus five supplemental named-test mutation controls. The broader `make nc-p2p-review` passes 4 transport plus all N6 controls. Review of other auto-ban call sites found they use explicit `from_peer`/authenticated transport-source arguments rather than the message's claimed signer; this trust contract is not enforced by a distinct Rust type. `DosDefenceEngine` is currently used only in the chaos harness/tests, not wired into `aura-node` runtime. The old N6 receipt is unchanged; these controls are local regression evidence, not independent review or production evidence.
* A negative contract return rolls back the staged diff and propagates through nested calls. Upgrade is currently restricted to `caller == contract`; callers of the lifecycle API must themselves be authenticated. This is not an owner/admin authorization implementation.
* Experimental wallet library: BIP-39 mnemonic-to-seed, SLIP-0010 hardened Ed25519 derivation, versioned AES-256-GCM encrypted seed document with authenticated metadata, Argon2id or scrypt parameters, redacted and zeroizing secret wrappers, owner-only Unix file mode, bounded load and atomic no-overwrite publication. No wallet CLI is implemented.

## W1 is not closed

The historical D-W1 BIP-32/44 path `m/44'/ALCRY'/0'/0/i` is incompatible with hardened-only SLIP-0010 Ed25519. On 2026-11-03 the user accepted Ed25519 and `m/44'/1'/0'/0'/i'` for testnet. Coin type 1 is shared testnet, **not** a mainnet allocation, and identical mnemonic/passphrase pairs produce identical keys across testnets. Mainnet derivation and migration need a separate decision. The BIP-32 vector test checks the independent secp256k1 library, not the Ed25519 wallet. Do not use prototype keys for funds.

`make test-w1` runs official BIP-39 seed and BIP-32 secp256k1 vectors, SLIP-0010 Ed25519 master and hardened child vectors, round trips, 1000 wrong passwords, deterministic byte mutations, and permission/no-overwrite and concurrency tests. The wrong-password/mutation tests compile with `insecure-test-kdf` to keep them fast and explicitly call `encrypt_seed_insecure_for_testing`/`decrypt_seed_insecure_for_testing`. The ordinary `encrypt_seed`/`decrypt_seed` methods keep production KDF floors even when that feature is enabled. The feature still exposes intentionally unsafe test methods, including in release builds, and **must never be enabled or called for production**. The deterministic mutation test is complemented by short coverage-guided libFuzzer parser and decrypt smoke runs with AddressSanitizer; these are not a long-running campaign. Zeroizing wrappers do not guarantee removal of every compiler/library/OS copy, swap, or core dump; mnemonic strings are caller-managed.

W1 now has independent Python/Rust scrypt and Argon2id keystore interoperability vectors, including Unicode cases, and short coverage-guided parser/decrypt fuzz runs. The explicit normalization-profile policy is implemented in the experimental API: Unicode 18.0 is the default for new mnemonic seeds; Unicode 15.1 is used only when legacy recovery explicitly selects it. The stable IDs are `bip39-nfkd-unicode-15.1` and `bip39-nfkd-unicode-18.0`; the keystore AAD header carries `seed_derivation.normalization_profile`, and recovery metadata carries `normalization_profile`. Missing/unknown markers, profile mismatches and silent fallback are rejected. Version 1 remains markerless and does not imply a profile; re-encryption preserves the known marker or its absence. On 2026-11-07 the user approved no in-place 15.1→18.0 upgrade: the same mnemonic remains a 15.1 recovery phrase, while changing keys requires a new 18.0 mnemonic and explicit asset transfer. Unicode 18.0 tables come from pinned `unicode-normalization` 0.1.26; Unicode 15.1 tables come from a checksum-verified local fork of upstream 0.1.23, with normalization source/tables retained and stylistic Clippy lints suppressed rather than changing the frozen algorithm. A reproduced version-sensitive example and the experimental canonical mnemonic whitespace rule are documented in `w1-keystore-prototype.md`. W1 remains open: it still needs a frozen interoperable format specification, broader vectors, longer fuzzing, phase evidence, and independent security review. The current `tools/nc_w1.py` run passes 39 wallet mutation controls, including password bypass, authenticated-header/profile omissions, unknown/mismatched profile fallback, profile loss on re-encryption, serialized-size bounds, duplicate fields, overwrite, weak production Argon2/scrypt parameters, mnemonic/passphrase normalization, unintended keystore password normalization, unsafe directory acceptance, parent-path redirection, temporary-file replacement, symlink following, and directory-sync bypass. `make nc-w1` also runs two controls of the structured decrypt fuzz oracle (wrong-password bypass and AAD-ID omission). These controls test tests, not cryptosystem security. Separate tests mutate every ciphertext/tag byte with weak test-only Argon2, exercise the ciphertext/tag boundaries for both KDFs, and check rejection of schema-fixed values. Ordinary production-policy APIs are also tested against modified ciphertext and tag using independent Argon2id and scrypt fixtures. These tests do not establish security against every mutation. W2 CLI, W3 offline signing and W4 multisig are not implemented.

## Known integration gaps

* The node pipeline is not wired to the contract execution engine or the actual P2P swarm; it is not a complete decentralized node.
* Devnet previews precede quorum and local advancement, but independently advancing one-validator pipelines still need a true quorum-driven prepare/validate/commit redesign, durable vote locks, rollback, catch-up and crash-consistent cross-node handling.
* Leader selection currently hashes public input rather than implementing a secret VRF lottery; proposal authorization, validator locking and view change need a full protocol review. Same-view 3-chain has a liveness cost across view changes.
* B8 checkpoint verification now requires a trusted active committee and a valid same-view, adjacent-round 3-chain; it checks QC/block context and block hashes. A root must be supplied from an **independently authenticated** source: `FinalityBlock` does not commit `state_root`, so the proof alone does not establish it. The fork-choice score (`justified_qc_view`) is still caller-supplied, and validator addresses are not bound to proposer Ed25519 public keys in this tree. B8 is not a production fork-choice or slashing authority; committee rotation and consensus locking are also not solved.
* Storage root and execution state contexts need integration and end-to-end verification. The X4 example WASM rebuild differs from pinned shipped artifacts and requires a separately reviewed reproducibility update. B10 Apalache model checking was not run.
* Historical `docs/evidence/*/SHA256SUMS` include mutable source files and manifests. The close-out auditor verifies those files or an explicitly recorded old/new source hash pair in `docs/evidence/review-w1-source-drift.txt`; historical evidence files themselves cannot be exempted. Unrecorded, stale or altered drift fails closed. This ledger records changes but is not an independent security review or a W1 receipt. The standalone historical `sha256sum -c` commands still report source drift; use the version-aware audit for the current tree.

## Current negative-control maintenance

Historical scripts were adapted where intentional API/semantic changes invalidated the original mutation targets. X5 NC-6 now targets executor rollback for a write followed by a negative status (the old contract-only mutant is neutralized by that rollback); this is a different control, not evidence that the old mutant still goes red. X6 NC-5 targets the new early negative-return propagation. B2 tests the `n-f` threshold and explicit rejection of a duplicate; B3 tests transaction-content binding with all other block fields fixed. N1 tests fail-closed AES-GCM error handling. `nc_rust.py` now distinguishes a passing baseline from a failing mutation and requires the named test to actually fail. X2/X5/X6 do not count a cargo compilation error as a passing negative control; their full test-suite failure is not necessarily tied to a particular named test. Historic receipts remain unchanged.

AES's own `zeroize` feature is explicitly enabled in addition to `aes-gcm`'s feature for both wallet and P2P builds, so AES expanded round keys are wiped on drop when supported by the crate. This remains best-effort zeroization, not a guarantee about all copies.

## W1 fuzz and interop additions

`fuzz_keystore_parse` exercises arbitrary UTF-8 plus public valid and invalid
JSON corpus samples without deriving an attacker-controlled KDF. The separate
`fuzz_keystore_decrypt` mutates authentication data under explicitly weak
`insecure-wallet-fuzz` settings; never enable that feature in production.
Short AddressSanitizer/libFuzzer runs do not show absence of vulnerabilities.
Python interoperability dependencies are optional test tools, not the audited
Rust production dependency set. W1 now has a dedicated CI workflow, but its
hosted run has not been observed from this workspace.

## Scrypt policy and persistence hardening

The prior prototype accepted scrypt `N=2^14, r=8, p=1`, below the conservative
OWASP password-storage baseline. Production-policy creation and ordinary
`decrypt_seed` now require `log_n >= 17`, subject to the existing resource cap.
Existing weak-scrypt files can be parsed, but require an explicitly named
migration-only decryption method; this changes default compatibility. The test
feature can still allow weak parameters and must not be used in production.
The old public fixture is retained to test migration.

On Unix `save_new` now syncs the parent directory after publication and
removing its temporary name. If directory sync fails, the returned error can
follow successful publication: callers must check the destination and must not
assume no side effect. Newly created ancestor directories, non-Unix ACLs and
non-Unix directory durability remain open limitations.

## KDF working-memory residuals

The Argon2 0.5.3 allocating convenience function drops its working blocks
without wiping them. W1 now allocates those blocks explicitly in a `Zeroizing`
wrapper and enables Argon2's upstream `zeroize` feature; the feature also wipes
some upstream temporary values. Tests compare derived bytes against the original
API and check the block zeroization implementation. This is best effort and is
not proof that all transient copies are erased.

Upstream scrypt 0.11.0 allocates password-derived working buffers without
zeroizing them on deallocation, and its public API provides no scratch-memory
hook. Wallet runtime now uses a minimal local RustCrypto 0.11.0 maintenance
fork that wraps B, V and T allocations in `Zeroizing`; `scrypt` upstream is
retained as a dev-only differential reference. The pinned provenance guard,
RFC 7914 and Python/Rust interop tests check the local change and compatibility.
This fork is not an independent audit; it creates a maintenance obligation and
does not wipe every stack/register/library/OS copy. See
[`PROVENANCE.md`](../third_party/scrypt-w1/PROVENANCE.md) and upstream
[issue #938](https://github.com/RustCrypto/password-hashes/issues/938).
Neither KDF protects secrets from a compromised live process, swap or dumps.


## Keystore path safety (Unix only)

Unix saves now reject a final parent that is a symlink, is not owned by the
effective UID, or is group/other writable, and create missing directories
with mode at most 0700. Publication and fsync use an open parent descriptor,
with inode checks for temporary/destination entries and final parent identity.
Unix loading rejects final symlinks and uses a nonblocking open before requiring
a regular file. These are deliberately stricter than earlier prototypes.
They do not defend against same-UID or privileged attackers, hostile ancestors,
unsafe/network filesystems, or name changes after the function returns. A
post-publication error can leave a file behind. Non-Unix behavior has not
received equivalent path, ACL, or durability hardening.

## Latest WIP verification (2026-11-07)

`make test-w1` passed: 14 wallet library tests, 34 ordinary keystore integration
tests, 43 `insecure-test-kdf` integration tests, and the fuzz-driver boundary
unit test (1/1), alongside the independent Python reference and local scrypt
provenance guards. `make nc-w1` passed 39/39 wallet mutation controls and 2/2
structured fuzz-oracle controls. Formatting, workspace Clippy with warnings
denied, the dependency/docs guards, and the direct close-out audit also passed.
The full `make test` run completed its Rust tests and doctests, then initially
failed only because four current source hashes in the WIP drift ledger were
stale. Their current-hash columns were updated without changing the frozen
historical hashes or receipts; `python3 tools/audit_closeout.py` now passes and
records 41 explicitly pinned drift paths. A post-correction aggregate `make test` rerun was attempted but stopped before
running because the sandbox's temporary Rust/Cargo cache had disappeared; this
does not change the prior Rust test results. These are
WIP regression results, not production-readiness evidence. W1 and independent
P2P review remain open.
