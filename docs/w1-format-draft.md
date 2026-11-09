# W1 testnet keystore interoperability — candidate draft

**Status: DRAFT, not frozen, not approved, not a security specification.** This
document describes the current experimental W1 prototype for independent review.
The repository implementation remains authoritative where this draft and code
disagree. Do not use prototype keys or files for real funds. Completing this
draft, obtaining a review, deciding compatibility, and recording the phase gate
are prerequisites to W1 closure; the presence of this file does not satisfy them.

Reference implementation: `crates/aura-wallet/src/keystore.rs` and
`derivation.rs`; narrative and filesystem caveats: [w1-keystore-prototype.md](w1-keystore-prototype.md).
The notation MUST/SHOULD below is a proposal for the candidate, not a claim
that the format has been frozen.

## 1. Scope and versions

A document is UTF-8 JSON, at most 1,048,576 bytes. It encrypts one 64-byte seed;
it does not contain a mnemonic or mnemonic passphrase. Every document has one
public account at index zero. Version 2 includes authenticated normalization
metadata; version 1 is a legacy prototype with no such metadata. This draft
covers the Stage 1 testnet path only, not a mainnet path or allocation.

A parser MUST reject invalid JSON, duplicate object member names after JSON
unescaping, extra/missing members, wrong JSON types, unknown version/algorithm
names, and values outside the applicable size and parameter limits. All JSON
numbers below are non-negative integral values and must fit the implementation's
specified integer width; floating-point forms are not accepted as integers.

The v1 top-level object has exactly `format`, `version`, `id`, `kdf`, `cipher`,
`ciphertext`, `derivation`, `accounts`. V2 adds exactly `seed_derivation`.
`format` is the literal `auralite-keystore`; `version` is the JSON integer 1
or 2. No metadata is authenticated until decryption has succeeded.

## 2. Exact field shapes

| Field | Accepted shape |
|---|---|
| `id` | 16 bytes encoded as 32 ASCII hex digits |
| `cipher` | object exactly `{name, nonce}`; `name` = `aes-256-gcm`, `nonce` = 12-byte hex |
| `ciphertext` | 80-byte hex: 64 encrypted seed bytes followed by the 16-byte GCM tag |
| `derivation` | exactly `{scheme, coin_type, path_template}`; values `slip10-ed25519`, integer `1`, `m/44'/1'/0'/0'/i'` |
| `accounts` | array of exactly one object, exactly `{index, path, public_key, address}` |
| account | index integer `0`; path `m/44'/1'/0'/0'/0'`; 32-byte public-key hex; nonempty address string |
| `seed_derivation` | v2 only, object exactly `{normalization_profile}`; value is `null`, `bip39-nfkd-unicode-15.1`, or `bip39-nfkd-unicode-18.0` |

Hex fields are case-insensitive for decoding, but their original string case is
part of AAD. The post-decryption account public key comparison requires the
exact lowercase hexadecimal output of the implementation; the address must
match the derived canonical lowercase address exactly. A nonempty address and a
well-formed public key are only preliminary parsing checks, not proof of
ownership or even a trusted deposit destination.

The KDF object is one of the following exact member sets; `salt` is 16-byte hex:

* Argon2id: `{name, salt, m_cost, t_cost, p_cost}`, with name `argon2id`;
  the parameters fit `u32`, `m_cost` is KiB, and the algorithm is Argon2id
  version 0x13 with a 32-byte output.
* scrypt: `{name, salt, log_n, r, p}`, with name `scrypt`; `log_n` fits `u8`,
  `r` and `p` fit `u32`, N = 2^log_n, output length 32 bytes.

Normal creation and decryption enforce Argon2id m=19,456..262,144 KiB,
t=2..10, p=1..4; scrypt log_n=17..18, r=8..16, p=1..4 and
`128 * N * r <= 268,435,456`, with additional algorithm-library validity rules.
Parsing accepts bounded legacy scrypt log_n=14..16, but ordinary decryption
refuses it. The explicit migration-only API accepts those weak parameters and
requires re-encryption to a new file; it is not a general policy relaxation.
The test-only feature and test-only weak-KDF methods are outside this candidate
and MUST NOT be used for real files. Aggregate concurrent KDF memory/CPU must
also be bounded by the caller.

### 2.1 Candidate recovery metadata marker

A companion non-secret recovery marker (`RecoveryMetadata`) is UTF-8 JSON of at
most 4,096 bytes. It MUST NOT contain a mnemonic, passphrase, password or seed.
The top-level value is a JSON object with the single member
`normalization_profile`, whose value is either `bip39-nfkd-unicode-15.1` or
`bip39-nfkd-unicode-18.0`; `null`, missing/extra members, duplicate member names
after JSON unescaping, BOM, trailing tokens and unsupported profile strings are
rejected without substituting a default. Canonical output is compact JSON
`{"normalization_profile":"<id>"}`. Parsing this marker does not authenticate it.

## 3. Normalization and key derivation

For mnemonic-derived seeds, English BIP-39 sentence and mnemonic passphrase
are separately normalized with the selected NFKD table version. The normalized
sentence must consist of valid checksum-verified English BIP-39 words separated
by exactly one ASCII space, without leading or trailing spaces. The passphrase
has no such spacing rule. BIP-39 derives 64 bytes using PBKDF2-HMAC-SHA512,
2048 iterations and salt `UTF8("mnemonic" + normalized_passphrase)`.

For v2, a profile ID identifies the exact Unicode table set. `null` means the
seed's mnemonic normalization profile is unspecified; it is not a default.
V1 has no profile marker and no inference is permitted. New mnemonic creation
uses 18.0; 15.1 is an explicit legacy recovery option. The marker is only
trusted after successful authentication. A recovery operation that uses a
mnemonic and trusted recovery metadata must authenticate the file, require the
selected profile to match, derive the seed and compare it to the stored seed.
The implementation uses `aura_crypto::constant_time_eq` on these fixed-size
seed bytes. This is not a claim that the whole recovery operation is
constant-time, nor that all side channels have been addressed.
Never silently retry another profile or change a legacy mnemonic's profile in
place. Re-encryption preserves the authenticated marker or its absence.

The Ed25519 key comes from SLIP-0010 hardened derivation at
`m/44'/1'/0'/0'/0'`: master HMAC-SHA512 key `ed25519 seed`; each hardened
child uses `0x00 || parent_private_key || uint32_be(index + 2^31)` as HMAC
message under the parent chain code. Encode the derived Ed25519 public key as
32 bytes. Compute the 20-byte address payload as the first 20 bytes of
`BLAKE3(b"AuraLite-Crypto/address-v1\x00" || public_key)`, and encode byte
`0x00` followed by this payload in classic lowercase bech32 with HRP `aura`.
See `aura-types` and `aura-tx` for the authoritative address implementation.
Coin type 1 is shared among testnets; it does not provide chain separation.

The keystore password is the exact UTF-8 byte encoding of the supplied string;
it is not normalized. Password and mnemonic passphrase are different inputs.

## 4. AAD and authentication

1. Parse and validate the complete schema and resource limits. Preserve decoded
   string values including their case. Do not treat any parsed header as trusted.
2. Remove only the top-level `ciphertext` member. Recursively sort each object
   by member name using Unicode scalar/UTF-8 lexicographic order; retain array
   order. The supported schemas use ASCII member names.
3. Serialize compact JSON without whitespace, using the pinned prototype's
   string escaping: `\"`, `\\`, `\b`, `\t`, `\n`, `\f`, `\r` and lowercase
   `\u00xx` for other U+0000–U+001F controls; leave slash and valid other
   Unicode scalar values as UTF-8. Emit unsigned integers as shortest decimal.
   This is not a claim of general RFC 8785/JCS support.
4. Use those UTF-8 bytes as AES-256-GCM associated data; use the KDF output as
   its 256-bit key and the stored 12-byte nonce. Verify the 16-byte tag before
   exposing the 64-byte plaintext. A wrong password or changed authenticated
   header/ciphertext must fail closed.
5. Derive account zero from the plaintext and require the exact public key and
   address strings to match. Reject the file if either differs even if GCM
   authentication succeeds. Do not return the seed before both checks pass.

Writers use a CSPRNG for a fresh 16-byte ID, 16-byte salt and 12-byte nonce;
public fixed values in test fixtures are never suitable for real files. AAD is
separate from whole-document serialization; the latter may be pretty or compact.
There is no in-place overwrite API. See the prototype's platform-specific
publication and zeroization limitations before implementing file operations.

## 5. Test material and limits of evidence

- `crates/aura-wallet/tests/fixtures/w1_aad_reference.json`: six independent
  exact AAD byte vectors; invalid-address and altered-profile cases are AAD-only
  and deliberately not decryptable.
- `w1_interop_argon2id*.json` and `w1_interop_scrypt*.json`: public test-only
  authenticated vectors. The `profile_null` and `profile_15_1` fixtures use
  an ASCII mnemonic passphrase; their seeds coincide across profiles. The
  `profile_sensitive` Argon2id fixtures use U+A7F1, where the specified NFKD
  results differ: U+A7F1 for Unicode 15.1 and ASCII `S` for Unicode 18.0.
  The independent Python generator uses these explicitly specified normalized
  results; it does not claim to independently implement both Unicode tables.
- `tools/w1_schema_reference.py` generates 71 independently checked public
  parse-only cases for schema, KDF resource limits, escaped duplicate names,
  invalid Unicode and malformed JSON. Rust checks the same cases under the
  default feature set. Acceptance of a parse-only case does not authenticate
  its metadata or prove the file can decrypt. The test-only weak-KDF feature
  intentionally changes parsing limits and is not part of this matrix.
- `tools/w1_recovery_metadata_reference.py` generates 37 independently checked
  public cases for the 4,096-byte recovery-metadata JSON marker, covering
  multi-byte UTF-8 byte limits, escaped duplicate keys, error categories and
  canonical output. Acceptance of a marker does not authenticate it.
- `make test-w1-interop`: checks Python-generated fixtures and Python decryption
  of freshly Rust-generated documents, plus transaction reference checks.
- `make test-w1` and `make nc-w1`: Rust regression and mutation tests. A test
  pass, short fuzz run or mutation control is not a cryptographic audit.

All fixture passwords, salts, nonces, seeds and keys are PUBLIC test data.

## 6. Review and freeze checklist — unresolved

Before presenting a frozen interoperability format, independently review and
agree the field schema, canonical byte rules and cross-language edge cases,
profile semantics, Unicode table provenance, resource limits and their future
upgrade policy, legacy migration, account binding and failure handling. Review
filesystem behavior and supported platforms separately. Add independent
implementations/vectors beyond the existing Python and Rust pair, a sustained
fuzz campaign and a W1 phase receipt. Review any format change as a versioned
compatibility decision; do not silently rewrite historical receipts. An external
security review cannot be generated by this repository's own local tests.
