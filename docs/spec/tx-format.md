# Transaction format — C4 specification

**Status:** ✅ implemented in `crates/aura-tx` (2026-10-25).
**Authority:** [`CORE_PLAN.md`](../plans/CORE_PLAN.md) C4 and
[`ARCHITECTURE.md`](../ARCHITECTURE.md) §1. This file fixes the bytes; any
change requires a new phase, a `CODEC_VERSION` bump, regenerated golden
vectors and a migration test.

## 1. Shape

A transaction is `Tx = UnsignedTx || Signature`. Grouping the unsigned fields
is an implementation detail: the logical field list is exactly the C4 DoD —
`chain_id, signer, nonce, kind, payload, gas_limit, max_fee, tip, expiry,
sig` — plus `codec_version`, required by `ARCHITECTURE.md` §1.

`Signature` is an **envelope**: `{ algo, public_key[32], bytes[64] }`.
Ed25519 cannot recover a public key from a signature, so the key travels with
the transaction and is bound to `signer` (§4). A bare 64-byte signature would
need an on-chain key registry that does not exist yet (C7).

## 2. Wire layout

All integers are little-endian fixed width; all lengths/counts are minimal
unsigned LEB128. This is the C2 format — see
[`tools/gen_c2_vectors.py`](../../tools/gen_c2_vectors.py) and
`crates/aura-codec`. There is no `usize`, no `HashMap`, no `serde`.

`UnsignedTx` (in this exact order):

| # | Field | Type | Encoding |
|---|---|---|---|
| 1 | `codec_version` | `u8` | 1 byte, must equal `CODEC_VERSION = 1` |
| 2 | `chain_id` | `ChainId` | `varint(len) ‖ UTF-8`, validated charset (C1) |
| 3 | `signer` | `Address` | 20 raw bytes |
| 4 | `nonce` | `Nonce` | `u64` LE |
| 5 | `kind` | `TxKind` | 1-byte tag (§3) |
| 6 | `payload` | `Vec<u8>` | `varint(count) ‖ bytes`, `count ≤ 65536` |
| 7 | `gas_limit` | `Gas` | `u64` LE |
| 8 | `max_fee` | `Amount` | `u128` LE |
| 9 | `tip` | `Amount` | `u128` LE |
| 10 | `expiry` | `Height` | `u64` LE |

`Signature` (appended):

| # | Field | Type | Encoding |
|---|---|---|---|
| 11 | `algo` | `SignatureAlgo` | 1-byte tag, `0` = Ed25519 only |
| 12 | `public_key` | `[u8; 32]` | 32 raw bytes |
| 13 | `bytes` | `[u8; 64]` | 64 raw bytes |

`MAX_TX_PAYLOAD = 65536`, aligned with `aura-codec`'s `MAX_ITEMS`. Decoding
uses `decode_canonical()`: unknown tags, non-minimal varints, trailing bytes
and re-encode mismatches are errors.

## 3. Tags

| `TxKind` | tag | `SignatureAlgo` | tag |
|---|---|---|---|
| `Transfer` | 0 | `Ed25519` | 0 |
| `Deploy` | 1 | | |
| `Call` | 2 | | |
| `Delegate` | 3 | | |
| `Undelegate` | 4 | | |

All other tags are rejected. Tag `1` for `SignatureAlgo` (secp256k1) is
**reserved but not accepted** in the Stage 1 tx format, even though
`aura-crypto` implements secp256k1 for compatibility. PQ algorithms will get
a separate tag range (H2).

## 4. Signing

```
signing_bytes = SIGNING_CONTEXT ‖ canonical(UnsignedTx)
signing_hash  = BLAKE3(C3_domain_message(chain_id, "tx", signing_bytes))
signature     = Ed25519_sign(seed, signing_hash)      // RFC 8032, pure
signer        = BLAKE3(ADDRESS_CONTEXT ‖ public_key)[0..20]
```

* `SIGNING_CONTEXT = b"AuraLite-Crypto/tx-signing-v1\0"` — distinguishes the
  tx body from every other message the codec can produce.
* `C3_domain_message` is the length-prefixed framing from `aura-crypto`:
  `HASH_CONTEXT ‖ u64(len(prefix)) ‖ prefix ‖ u64(len(payload)) ‖ payload`,
  `prefix = chain_id ‖ 0 ‖ "tx" ‖ 0`. It adds chain-id and purpose separation
  a second time (rule D5), so a testnet signature cannot be replayed on
  mainnet and a vote cannot be replayed as a transaction.
* `ADDRESS_CONTEXT = b"AuraLite-Crypto/address-v1\0"`.
* `sign()` refuses a seed whose public key does not derive to `signer`.

Verification requires **all** of: `algo == Ed25519`,
`signer_address(public_key) == signer`, and a valid Ed25519 signature over
`signing_hash`.

## 5. Transaction id

```
txid = BLAKE3(TXID_CONTEXT ‖ canonical(Tx))
TXID_CONTEXT = b"AuraLite-Crypto/txid-v1\0"
```

The signature is included, so two different signatures over the same
unsigned body have different ids. `txid` is not used for consensus ordering
yet — that is C5 (`tx_root`).

## 6. Validation order

`Tx::validate(expected_chain, height)` checks, in order:

1. `codec_version == CODEC_VERSION`;
2. `chain_id == expected_chain` — **replay protection**;
3. `payload.len() <= MAX_TX_PAYLOAD`;
4. `gas_limit != 0`;
5. `tip <= max_fee`;
6. `height <= expiry`;
7. `verify_signature()` (§4).

`height` and `expiry` are consensus heights, never a local clock. Nonce/state
checks belong to C7 and the mempool (D3), not here.

## 7. Golden vectors and negative controls

`tools/gen_c4_vectors.py` is an independent Python implementation. It
self-checks pure-Python BLAKE3 against 7 official BLAKE3 vectors and
pure-Python Ed25519 against RFC 8032 §7.1 TEST 1/2/3 **before** emitting
`crates/aura-tx/tests/fixtures/c4_golden.txt`. The fixture holds 5 canonical
transactions (including u64/u128 maxima and a mainnet-chain tx) and 9
rejection vectors: trailing byte, truncation, bad version, unknown kind,
unknown algorithm, tampered signer, tampered signature, non-minimal chain
length, non-minimal payload length.

`tools/nc_c4.py` runs 6 negative controls: both contexts, a golden fixture,
replay protection, signer↔key binding and signature verification. Total
across C0–C4: **20 NC**.

## 8. Non-goals at C4

Block/header format and `tx_root` (C5), Merkle proofs (C6), accounts and key
layout (C7), genesis (C8), fuzzing (C9), secp256k1 in tx signatures, hardware
keys (W5), PQ signatures (H2). Fee *charging*, gas metering and execution are
not implemented — C4 only fixes the bytes and the checks that can be done
without state.
