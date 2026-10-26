# Account model & state key layout — C7 specification

**Status:** ✅ implemented in `crates/aura-state` (2026-10-26).
**Authority:** [`CORE_PLAN.md`](../plans/CORE_PLAN.md) C7 and
[`ARCHITECTURE.md`](../ARCHITECTURE.md) §1. This file fixes the state key
bytes and the account value. Any change requires a new phase, a
`PROTOCOL_VERSION` bump and a migration test.

**Scope.** C7 is the **first real API** of `aura-state`. It defines the layout
only: no storage backend, no JMT, no `state_root` computation and no
persistence — those are `STATE_PLAN` S1–S7. There is deliberately no fake
`Ok(())`: every function either validates or returns a typed error.

## 1. Key namespaces

Only five one-byte prefixes are valid. `0x00` and `0x06`–`0xFF` are
**reserved for the protocol** and can never be turned into a [`StateKey`].

| Tag | `KeySpace` | Key bytes | Total length |
|---|---|---|---|
| `0x01` | `Account` | `tag ‖ address[20]` | 21 |
| `0x02` | `ContractCode` | `tag ‖ address[20]` | 21 |
| `0x03` | `ContractStorage` | `tag ‖ contract[20] ‖ slot[32]` | 53 |
| `0x04` | `Staking` | `tag ‖ validator[20]` | 21 |
| `0x05` | `Receipt` | `tag ‖ txid[32]` | 33 |

`MAX_STATE_KEY_LEN = 128`. A key is accepted only if its prefix is known
**and** its total length equals `KeySpace::expected_len()`. This gives three
properties at once:

* two different accounts cannot collide (the address is the whole payload);
* a key in one namespace cannot alias a key in another (different tag and/or
  length);
* reserved space is unreachable — `StateKey::from_raw` rejects those prefixes
  before anything else can happen.

## 2. Contract isolation

`assert_contract_may_write(contract, key)` requires:

1. `key.space() == ContractStorage` — otherwise `ForbiddenContractWrite`;
2. `key[1..21] == contract` — otherwise `WrongContractStorage`.

A contract can therefore write only to `0x03 ‖ its_own_address ‖ slot`. It
cannot reach account balances, code, staking metadata, receipts, another
contract's storage, or any reserved prefix. The VM host ABI (X phases) must
call this check for every storage write.

## 3. Key digest (JMT path)

```
StateKey::digest = BLAKE3(STATE_KEY_CONTEXT ‖ u64_le(len(raw)) ‖ raw)
STATE_KEY_CONTEXT = b"AuraLite-Crypto/state-key-v1\0"
```

The explicit length prefix removes ambiguous concatenation; the context
separates state paths from tx/event leaves (C5/C6). S1 will use this digest as
the JMT path. C7 does **not** compute a state root.

## 4. Wire format

`StateKey` = `varint(len) ‖ raw`, with a **minimal** unsigned LEB128 length.
`StateKey::from_bytes` checks: minimal varint, `len ≤ MAX_STATE_KEY_LEN`,
exact byte count, no trailing bytes, known prefix, namespace length, and that
re-encoding reproduces the input byte for byte.

`Account` is fixed-width, no length prefixes:

| Field | Type | Bytes |
|---|---|---|
| `nonce` | `Nonce` (`u64` LE) | 8 |
| `balance` | `Amount` (`u128` LE) | 16 |
| `code_hash` | `Hash` | 32 |
| `storage_root` | `Hash` | 32 |

Total **88 bytes**. `Account::from_canonical_bytes` uses C2
`decode_canonical()`. `NO_CODE = EMPTY_STORAGE_ROOT = Hash::ZERO` are
sentinels for an externally owned account.

## 5. Checked arithmetic

`credit`/`debit` use `Amount::checked_add`/`checked_sub` and never wrap;
`bump_nonce` uses `Nonce::checked_next`. Failures are `BalanceOverflow`,
`InsufficientBalance` and `NonceOverflow`. A failed debit leaves the balance
unchanged.

## 6. Golden vectors and negative controls

`tools/gen_c7_vectors.py` is independent Python: pure-Python BLAKE3
(self-checked against 7 official BLAKE3 vectors) plus a transcription of the
key layout, digests and account encoding. `c7_golden.txt` has 107 lines:

* raw keys, encoded keys and digests for 5 namespaces × 4 indices — the
  generator asserts all **20 digests are distinct**;
* 3 canonical accounts (empty, typical, `u64`/`u128` maxima);
* contract-isolation operands for two contracts;
* 12 rejection vectors: 5 reserved prefixes, too-short/too-long account and
  storage keys, empty key, non-minimal length varint, trailing byte,
  truncation.

The Rust gate additionally checks **1000 distinct addresses** (5000 keys and
digests across all namespaces) with zero collisions, and iterates **all 256
prefixes**, asserting exactly five are reachable.

`tools/nc_c7.py` runs 6 negative controls: the key-digest context, a golden
digest, the reserved-prefix rejection, the namespace-length check, the
contract namespace check and the contract-address check. Total across
C0–C7: **38 NC**.

## 7. Non-goals at C7

Versioned Merkle state tree, inserts/deletes/lookups, state proofs and
`state_root` (S1); RocksDB/WAL and snapshots (S2–S4); execution overlays and
rollback (S3); genesis allocation (C8); contract code hashing and the VM host
ABI (X phases); gas charging and fee distribution (T/D phases).
