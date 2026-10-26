# Genesis format — C8 specification

**Status:** ✅ implemented in `crates/aura-genesis` (2026-10-26).
**Authority:** [`CORE_PLAN.md`](../plans/CORE_PLAN.md) C8 and
[`ARCHITECTURE.md`](../ARCHITECTURE.md) §1. Any change requires a new phase,
a `PROTOCOL_VERSION` bump and a migration test.

**Scope.** C8 adds the **14th crate**, `aura-genesis`, and the first new
external dependency since C3: `serde_json =1.0.154`, used **only** to parse
the genesis config file. Consensus bytes always come from `aura-codec`;
no crate uses a `serde` derive. A human-readable example lives at
[`genesis-testnet.json`](genesis-testnet.json).

## 1. JSON schema (config only)

Top-level object — exactly five keys, no extras, no defaults:

| Key | JSON type | Meaning |
|---|---|---|
| `chain_id` | string | must be a canonical `ChainId` and equal `auralite-test-1` in Stage 1 |
| `protocol_version` | integer | must equal `PROTOCOL_VERSION = 1` |
| `genesis_time_ms` | integer `u64` | consensus timestamp, **never** a local clock |
| `validators` | array | non-empty, declared order is consensus-significant |
| `allocations` | array | initial balances |

Validator object — exactly `{ address, power, public_key }`:

* `address` — bech32 `aura1...` (C1, checksum validated);
* `power` — non-zero `u64`;
* `public_key` — exactly 64 hex characters (32 bytes, opaque at C8).

Allocation object — exactly `{ address, balance }`:

* `balance` — a **decimal string**, parsed to `u128`. JSON numbers/floats
  are rejected, because a `u128` amount must never round-trip through `f64`.

Rejected at parse time: unknown keys (top level or nested), missing keys,
wrong types, invalid chain-id/version/address/public key/balance, empty
validator set, zero power, zero allocation, duplicate validator addresses,
duplicate allocation addresses, and a total supply that overflows `u128`.

Object key order and whitespace are irrelevant — the parser extracts named
fields into a struct. **Array order is preserved**: validator order changes
the hash (C8 NC), while `allocation_root` sorts by address because state is
a set.

## 2. Canonical binary document

C2 rules (minimal LEB128 varints, fixed-width little-endian integers, no
`usize`, no `serde`):

```
chain_id          = varint(len) ‖ UTF-8
protocol_version  = u32 LE
genesis_time_ms   = u64 LE
validators        = varint(count) ‖ ( address[20] ‖ power u64 LE ‖ key[32] )*
allocations       = varint(count) ‖ ( address[20] ‖ balance u128 LE )*
```

`Genesis::from_canonical_bytes` uses `decode_canonical()` (no trailing bytes,
no non-minimal varints, re-encode must match) and then re-runs every
structural check.

## 3. Commitments

```
genesis_hash       = hash_domain(chain_id, Domain::Genesis,
                        GENESIS_CONTEXT ‖ canonical_bytes)
validator_set_hash = BLAKE3(VALIDATOR_SET_CONTEXT ‖ varint(count) ‖ validators*)
allocation_root    = MerkleRoot(sorted allocation leaves)
```

`hash_domain` is C3's length-prefixed framing with tag `"genesis"`.
`validator_set_hash` is the value that fills C5's header field — C5 left it
caller-supplied; C8 is the first phase that actually computes it.

An allocation leaf is:

```
key      = 0x01 ‖ address[20]                         (C7 StateKey::account)
key_hash = BLAKE3(STATE_KEY_CONTEXT ‖ u64(len(key)) ‖ key)
account  = u64(0) ‖ u128(balance) ‖ ZERO32 ‖ ZERO32   (C7 Account)
leaf     = BLAKE3(ALLOCATION_LEAF_CONTEXT ‖ key_hash ‖ u64(len(account)) ‖ account)
```

Leaves are sorted by address and combined with the C6 tree (`node_hash`,
odd-node duplication). `allocation_root` is a **genesis allocation
commitment, not the JMT `state_root`** — S1 still owes the versioned tree.

Contexts: `GENESIS_CONTEXT = b"AuraLite-Crypto/genesis-v1\0"`,
`VALIDATOR_SET_CONTEXT = b"AuraLite-Crypto/validator-set-v1\0"`,
`ALLOCATION_LEAF_CONTEXT = b"AuraLite-Crypto/allocation-leaf-v1\0"`.

## 4. CLI

`cargo run -p aura-genesis --bin aura -- genesis <subcommand>`:

```
aura genesis hash   --file <genesis.json>          # prints the 64-hex hash
aura genesis verify --file <genesis.json> --expect <64-hex>
aura genesis show   --file <genesis.json>          # key=value summary
```

`verify` exits 1 on mismatch; bad arguments exit 2. This is the C8 slice of
the `aura` CLI; `NODE_PLAN` D8 owns the remaining subcommands.

## 5. Golden vectors and negative controls

`tools/gen_c8_vectors.py` is independent Python: pure-Python BLAKE3
(self-checked against 7 official BLAKE3 vectors), an independent BIP-173
bech32 encoder, the C3 domain framing, C5/C6 Merkle node rules and the C7
key/account encodings. `c8_golden.txt` has 44 lines: 19 JSON documents
(hex-encoded UTF-8) — three equivalent representations, two order variants
and 13 invalid ones — plus two full sets of canonical bytes, genesis hash,
validator-set hash, allocation root, total supply and counts.

The generator self-checks that object key order/whitespace do **not** change
any commitment, that validator order **does** change both `genesis_hash` and
`validator_set_hash`, and that allocation order does **not** change
`allocation_root`.

C8 DoD gate: three independent parses of the same config give one hash;
NC: validator order change → different hash. The CLI is pinned by four
integration tests against the same vectors.

`tools/nc_c8.py` runs 6 negative controls: both contexts, a golden hash,
validator-order sorting, unknown-field rejection and the Stage 1 chain-id
check. Total across C0–C8: **44 NC**.

## 6. Dependencies

C8 adds `serde_json =1.0.154` (plus its transitive `itoa`, `memchr`, `zmij`).
Because the C3 allowlist is frozen historical evidence, the delta is
recorded in `docs/spec/c8-dependency-addendum.txt` and
`tools/check_deps.py` compares `Cargo.lock` against the union of the baseline
and all addenda. `cargo deny` (licences/advisories) is still owed to H1.

## 7. Non-goals at C8

Genesis **block** construction and execution (C5/D phases), JMT `state_root`
(S1), validator key generation ceremony (W7/O5), BLS key aggregation (B2),
on-chain governance upgrades, faucet and devnet wiring (D7), and any Stage 2
mainnet genesis (`auralite-1` is explicitly rejected here).
