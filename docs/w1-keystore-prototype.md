# W1 keystore prototype, version 2 (WIP)

**Status: experimental; not a finalized interoperability or security specification. Do not use for real funds.** W1 remains open; the D-W1-testnet path was accepted on 2026-11-03; see [WIP_SECURITY_AND_W1.md](WIP_SECURITY_AND_W1.md).

## Key material and account

English BIP-39 mnemonic plus its optional passphrase yields a 64-byte seed. Both mnemonic input and BIP-39 passphrase are normalized with Unicode NFKD before deriving the seed; the accepted word list is English. **After NFKD, the mnemonic must be the canonical word sentence with exactly one ASCII space between words and no leading or trailing whitespace.** For example, U+3000 separators normalize to ASCII spaces and are accepted, but tabs, newlines, double spaces and leading/trailing spaces are rejected, not silently rewritten. This restriction does not apply to the BIP-39 passphrase or keystore password. Earlier W1 prototypes silently collapsed mnemonic whitespace through the BIP-39 library: to recover a seed they produced from such an input, explicitly enter its canonical single-space word sentence and verify the resulting account against a trusted backup. This is a compatibility change to the experimental API; it does not claim to recover seeds made by other software using literal noncanonical input bytes. `tools/w1_mnemonic_reference.py` independently records NFKD/PBKDF2 results for public examples; its noncanonical hashes illustrate the ambiguity, not accepted BIP-39 sentences. On 2026-11-05 the user selected a versioned-profile policy for BIP-39 NFKD: Unicode 18.0 is the default for new mnemonic seeds, while Unicode 15.1 is available only when legacy recovery explicitly selects it. The stable IDs are `bip39-nfkd-unicode-15.1` and `bip39-nfkd-unicode-18.0`. This policy is implemented in the experimental API: version-2 keystores authenticate `seed_derivation.normalization_profile` in the AAD header, and recovery metadata carries `normalization_profile`. Missing or unknown markers are rejected rather than guessed; recovery verification authenticates the keystore, requires the selected profile to match, and checks the recovered seed. Version 1 remains markerless and does not imply either profile. Re-encryption preserves a known marker (or its absence) and never upgrades Unicode 15.1 to 18.0 in place. The same mnemonic remains a Unicode 15.1 recovery phrase; changing keys requires a new Unicode 18.0 mnemonic and explicit asset transfer. Regression vectors exercise the profile distinction and recovery binding, but W1 still needs a frozen format specification, broader vectors/fuzzing, phase evidence and independent security review. The pinned `unicode-normalization` 0.1.26 used for Unicode 18.0 reports tables for Unicode 18.0.0; the separate local fork of upstream 0.1.23 supplies frozen Unicode 15.1.0 tables. An independently reproduced example demonstrates the danger: with public passphrase U+A7F1, these tables normalize to ASCII `S`, whereas Python `unicodedata` 15.1.0 treats the scalar as unassigned and leaves it unchanged, producing a different seed. This may affect both mnemonic and passphrase normalization; a consumer must not assume an unspecified NFKD implementation gives identical bytes for every input. Regression vectors record the table-dependent output and bind it to an explicit profile, but they do not establish compatibility with unspecified external Unicode tables or constitute comprehensive interop. Existing real or experimental recovery material must not be silently renormalized under a changed policy; verify against a trusted account/backup and review before changing dependencies. The library owns and zeroizes its normalized and canonical temporary strings on drop, although library internals and caller-owned inputs can still leave copies. Only the seed is encrypted; the mnemonic and passphrase are not stored. The prototype derives an Ed25519 account via SLIP-0010 hardened private derivation, at `m/44'/1'/0'/0'/0'` for account zero. Coin type 1 is the shared testnet coin type, not an ALCRY mainnet assignment. This is the accepted D-W1-testnet deviation from the historical D-W1 BIP-32/44 path and does not claim compatibility with it. The same mnemonic and passphrase with coin type 1 derive the same keys across testnets: network isolation is not provided by this path. Mainnet coin type and migration require a separate decision.

## JSON document

Current version 2 accepts exactly the fields below; unknown or duplicate JSON keys are rejected. All binary values use hexadecimal. The UTF-8 document is limited to 1 MiB. Version 2 adds an authenticated `seed_derivation.normalization_profile` marker for BIP-39-derived seeds; `null` denotes a seed whose mnemonic profile is unspecified, not a default. Version 1 is accepted only as a legacy prototype format and has no profile marker. Both versions require exactly one public account, index 0. Account path, public key and address are checked against the decrypted seed before it is returned.

```json
{
  "format": "auralite-keystore",
  "version": 2,
  "id": "<16 random bytes, hex>",
  "kdf": {
    "name": "argon2id",
    "salt": "<16 random bytes, hex>",
    "m_cost": 65536,
    "t_cost": 3,
    "p_cost": 1
  },
  "cipher": {"name": "aes-256-gcm", "nonce": "<12 random bytes, hex>"},
  "seed_derivation": {"normalization_profile": "bip39-nfkd-unicode-18.0"},
  "ciphertext": "<64 encrypted seed bytes plus 16-byte GCM tag, hex>",
  "derivation": {
    "scheme": "slip10-ed25519",
    "coin_type": 1,
    "path_template": "m/44'/1'/0'/0'/i'"
  },
  "accounts": [{
    "index": 0,
    "path": "m/44'/1'/0'/0'/0'",
    "public_key": "<32 bytes, hex>",
    "address": "<AuraLite address>"
  }]
}
```

### Versioned mnemonic normalization and recovery metadata

New mnemonic-derived seeds default to `bip39-nfkd-unicode-18.0`. Legacy recovery must explicitly select `bip39-nfkd-unicode-15.1`; there is no table auto-detection or silent fallback. Version 2 binds the selected ID (or `null` when no mnemonic profile is specified) at `seed_derivation.normalization_profile` in the authenticated header. Version 1 has no marker and must not be interpreted as either Unicode profile. Recovery metadata uses the selected `normalization_profile` ID. `RecoveryMetadata::from_json_str` rejects a missing or unknown ID. `verify_mnemonic_recovery` requires the keystore password, authenticates the header/ciphertext, checks exact profile agreement and verifies the recovered seed. Re-encryption preserves the known marker or its absence; there is no in-place 15.1-to-18.0 profile upgrade. The same mnemonic stays a 15.1 recovery phrase; changing keys requires a new 18.0 mnemonic and explicit asset transfer. This describes only the approved profile-marker behavior; it does not add other recovery-metadata schema or compatibility rules, and it is not yet a frozen interoperability specification.

Alternatively `kdf` is `{ "name": "scrypt", "salt": "...", "log_n": 17, "r": 8, "p": 1 }`. Argon2id uses version 0x13 and a 32-byte output; `m_cost` is KiB. In the ordinary encrypt/decrypt API, even with the testing feature enabled, accepted Argon2id parameters have memory 19,456–262,144 KiB, iterations 2–10 and parallelism 1–4, subject to the library's additional validity rules. For scrypt, the normal encrypt/decrypt policy requires log2(N) 17–18, r 8–16, p 1–4, with `128*N*r <= 256 MiB`, subject to the library's additional rules. This conservative floor follows the `N=2^17, r=8, p=1` password-storage baseline in [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html), applied here as a KDF policy rather than a guarantee for encrypted wallets. The previous prototype allowed log2(N) 14–16; these weak documents are parseable but normal decryption refuses them. An explicit migration-only API described below can recover them. The keystore password is used as its UTF-8 bytes exactly as supplied; this version does not normalize it, so visually identical Unicode passwords may differ. This is separate from BIP-39 mnemonic-passphrase normalization. These are resource bounds and minimums, not assurances about password strength. The `insecure-test-kdf` feature additionally exposes explicitly named `encrypt_seed_insecure_for_testing` and `decrypt_seed_insecure_for_testing` methods with dangerously weak Argon2 and scrypt parameters. It can also allow parsing weak Argon2 headers. Ordinary encryption and decryption retain their normal minima even with this feature, but the unsafe methods remain callable in release builds when the feature is enabled. Never enable or call this feature for production; this API separation is not a complete defense against deliberate misuse.

Until password verification and a successful `decrypt_seed`, all parsed header fields — including the displayed public key and address — are **unauthenticated input**. A UI must not present them as trusted or use them as a deposit destination without independently verifying them. After decryption the implementation checks the account metadata against the derived seed. Limit concurrent KDF operations and protect resources in the caller: per-file KDF bounds do not bound aggregate memory or CPU across concurrent requests.

AES-256-GCM encrypts the seed with a fresh random 96-bit nonce and the KDF-derived 256-bit key. Associated data is the compact canonical JSON encoding of the entire document **except** `ciphertext`: object keys recursively sorted, arrays in their given order, JSON string/number encoding as produced by the current `serde_json` implementation, UTF-8 bytes, without whitespace. Authentication failure gives no plaintext. Independent Python/Rust vectors are available below; a frozen specification and broader interop review are still required before general interchange can be claimed.

### Canonical associated data: implementer-oriented draft

The following describes the current implementation, not an approved frozen wire standard:

1. Decode UTF-8 JSON, reject malformed input and duplicate object names after JSON unescaping, and validate the exact version-2 schema (or the supported legacy version-1 schema) and resource bounds. The maximum input length is 1,048,576 bytes. Numeric fields must be JSON integers accepted by `serde_json` as unsigned, not strings or floating-point numbers. Do not trust any header value before authentication.
2. Remove **only the top-level** `ciphertext` member. Do not remove nested members of the same name in a generalized JSON implementation; the exact schema itself does not allow any such extra member. Preserve every remaining JSON value, including string case. For each object, sort member names lexicographically by Unicode scalar value (equivalently, UTF-8 byte order for valid scalar strings); both accepted schemas' field names are ASCII. Preserve array order.
3. Serialize compactly, without whitespace. Strings use double quotes; escape quote and backslash, use `\b`, `\t`, `\n`, `\f`, `\r` for the respective controls and lowercase `\u00xx` for other U+0000–U+001F controls; leave `/` and valid non-control Unicode scalars as UTF-8, including U+2028 and supplementary characters. Unsigned integers use shortest ordinary decimal digits. This is the tested behavior for the constrained v1/v2 schemas, **not** a claim of general RFC 8785/JCS compatibility. The exact pinned `serde_json` implementation remains authoritative while this is a prototype.
4. The UTF-8 bytes from step 3 are AES-GCM AAD. The `ciphertext` hex decodes to 64 bytes of encrypted seed followed by the 16-byte GCM tag. Do not return plaintext unless GCM authentication succeeds and the account's public key and address exactly match values derived from the seed. The account public-key comparison expects the lowercase hex spelling produced by the implementation.

`to_json_string` normally emits a pretty-printed whole document, **not** the compact AAD representation. If pretty-printing would exceed the 1 MiB loader limit, it falls back to compact whole-document JSON; if that also exceeds the limit it returns `InvalidFormat` instead of emitting an unloadable file. This changes whitespace formatting near the size boundary, not authenticated values or the AAD algorithm. A parsed header, including an address, remains untrusted until successful decryption. Hex digit case is not normalized before AAD construction: changing the case of a header hex string changes authenticated bytes. Ciphertext is excluded from AAD but remains authenticated by GCM. `w1_aad_reference.json` records exact public AAD bytes and SHA-256 values independently generated by `tools/w1_aad_reference.py`; the third case deliberately contains an invalid account address to exercise pre-authentication string escaping and must not be mistaken for a decryptable keystore. Rust compares the exact bytes, including when `serde_json/preserve_order` is enabled; the Python reference runs without nonstandard packages. These vectors do not freeze the format or constitute a security review.

## Persistence and limitations

On Unix, `save_new` requires the **final parent directory** to be a real directory owned by the effective user and not writable by group or others; it rejects a final parent symlink. Missing directories are created with mode at most 0700 (subject to umask). It anchors temporary creation, no-overwrite hard-link publication, cleanup and directory synchronization to an open directory descriptor, checks that the temporary and published entries refer to its open file, and detects a replaced final-parent pathname before success. Files are created with mode at most 0600 (subject to umask). This is a compatibility change: previously accepted shared/writable or symlink parent paths may now fail. `load` rejects a symlink in the final file component and opens nonblocking, then accepts only regular files; FIFOs no longer block waiting for a writer. Unix `save_new` also rejects destination paths ending in `/` or `/.`, rather than silently treating them as file names. This filesystem implementation has been tested at runtime on Linux; a FreeBSD cross-check only establishes compilation. Redox and ESP-IDF reject both filesystem operations with `UnsupportedPlatform` rather than weakening the policy; their runtime behavior has not been tested. Opening a device can have effects before the non-regular-file check, and `O_NONBLOCK` cannot guarantee that every filesystem or device open is nonblocking.

These checks do **not** make a hostile filesystem safe. The caller must trust all ancestor directories and the effective-user account; earlier path components may still contain symlinks, and a same-UID process, privileged attacker, network filesystem, or hostile mount may still modify files or change names before or after checks. The filesystem must support hard links and directory synchronization. A post-publication failure can leave a destination present, including under a renamed directory; inspect paths and backups rather than blindly retrying or deleting. Newly created ancestor directories are not individually synchronized, so survival across power failure is still not guaranteed. On non-Unix, the existing fallback does not implement the Unix trust, no-symlink or directory-sync policy; ACLs, special-file behavior and durability are not established. Parsing or encrypting documents may allocate memory; zeroization of wrappers is best effort and does not remove all compiler, runtime, swap, crash-dump or caller-owned copies. The caller must protect and zeroize generated mnemonic strings and password copies. The Argon2id path owns its large working-block allocation in a `Zeroizing` wrapper and enables the upstream `zeroize` feature; this still cannot guarantee erasure of every library, stack, allocator or operating-system copy. The wallet uses a local, minimal maintenance fork of RustCrypto `scrypt` 0.11.0 which wraps its password-derived B, V and T heap buffers in `Zeroizing`. It does not erase every temporary, register, stack copy, library-internal value or operating-system copy; it does not help when a process is compromised or aborts. This is an unaudited fork with ongoing maintenance cost, not an upstream release. Original source provenance, exact patch and MIT/Apache licences are in [`third_party/scrypt-w1/PROVENANCE.md`](../third_party/scrypt-w1/PROVENANCE.md); the unpatched upstream crate is retained only for differential tests.

## Migration from the earlier weak-scrypt prototype

An earlier W1 prototype could write scrypt `log_n=14..16`, now below the
normal policy floor. To avoid silently weakening new files or stranding prior
experimental files, `Keystore::from_json_str`/`load` still parse those headers,
while normal `decrypt_seed` returns `WeakKdfParameters`. Only the explicit
`decrypt_legacy_scrypt_for_migration` method permits those scrypt parameters;
it still enforces structural/resource bounds and AES-GCM authentication and
rejects other KDFs. Keep a backup, use the returned seed only in a protected
process, re-encrypt with `KdfParams::default_argon2id()` or allowed strong
scrypt into a **new** path with `save_new`, and verify the replacement before
considering removal of the old file. There is no migration CLI yet. This is
experimental code; never rely on these testnet keys for funds.

## Independent test vectors (public test data)

`crates/aura-wallet/tests/fixtures/w1_interop_argon2id.json` and
`w1_interop_scrypt.json` use the public BIP-39 mnemonic and passphrase shown in
`tools/w1_interop_reference.py`, plus a **public password, fixed salt, nonce,
and ID**. They are deterministic test vectors, never keys for funds. Rust
checks that both Python-generated documents decrypt to the published seed,
that wrong passwords fail, and that authenticated-header tampering fails.
The Python reference independently creates the fixtures and decrypts randomly
salted/nonce Rust-generated files for both KDFs. It uses `argon2-cffi`,
`cryptography`, and `blake3` as optional test dependencies, separate from the
Rust production dependency graph. See `tools/w1-reference-requirements.txt`.

Two additional fixtures, `w1_interop_argon2id_unicode.json` and
`w1_interop_scrypt_unicode.json`, exercise NFKD BIP-39 passphrases and raw
UTF-8 keystore passwords across Python and Rust. A visually identical password
with a different Unicode byte representation is deliberately rejected: only the
BIP-39 mnemonic and its passphrase are normalized, not the keystore password.
Before this fix the mnemonic parser incorrectly assumed its input was already
NFKD, so it rejected some valid compatibility-form mnemonics. The derived seed
for previously accepted inputs is unchanged.

The public `w1_interop_scrypt_legacy.json` additionally reproduces the former
weak `log_n=14` format. Python independently regenerates and verifies it;
Rust tests the explicit migration-only path, including wrong-password and
header-tampering failures. It is **not** acceptable for new production files.

Two negative fixtures, `w1_inconsistent_public_key.json` and
`w1_inconsistent_address.json`, deliberately contain account metadata that
belongs to a different key or address, respectively. Python independently
constructs and authenticates these documents and confirms successful AES-GCM
decryption of the expected seed; Rust must nonetheless reject both after
comparing metadata to the derived account. Each fixture has a distinct public
salt, nonce and identifier. These test credentials are not for real funds.

The additional public fixture `w1_tx_reference.json` binds this same test
mnemonic, encrypted seed and SLIP-0010 account zero to an existing C4
transaction. The reference generator (`python3 -m tools.w1_tx_reference`)
uses Python PBKDF2/HMAC, the independent Python C4 encoder, two independently
checked BLAKE3 implementations and both OpenSSL and pure-Python Ed25519. Rust
tests compare the signing bytes, digest, signature, wire transaction and txid;
they also reject another derived key, a foreign chain, and a signature from a
foreign domain. The fixture contains public test data and must never be used
for funds. This is an integration test only: it is not a wallet signing API,
CLI, online authorization policy, or completed W3 offline-signing workflow.

These vectors provide a concrete cross-implementation check, not a frozen
interoperability standard or an external review. Run:

```sh
python3 -m pip install -r tools/w1-reference-requirements.txt
python3 tools/w1_interop_reference.py
```
