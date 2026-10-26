# Block / header format — C5 specification

**Status:** ✅ implemented in `crates/aura-block` (2026-10-25).
**Authority:** [`CORE_PLAN.md`](../plans/CORE_PLAN.md) C5 and
[`ARCHITECTURE.md`](../ARCHITECTURE.md) §1. This file fixes the bytes; any
change requires a new phase, a `PROTOCOL_VERSION` bump, regenerated golden
vectors and a migration test.

## 1. Shape

`Block = Header || Body`. `Body` is a list of [`aura_tx::Tx`](tx-format.md)
canonically sorted by `(signer, nonce)`. The header commits to the body via
`tx_root`; `block_hash` hashes the header only.

C5 adds **one field** to the `ARCHITECTURE.md` §1 header definition:
`chain_id` (decision D-C7 in `CORE_PLAN.md`). Reason: rule D5 requires
chain-id separation for every signed/hashed object; without it a testnet
header hash is indistinguishable from a mainnet one. `ARCHITECTURE.md` §1 has
been amended in the same patch. No migration is needed because the network
has produced **0 blocks**.

`state_root`, `validator_set_hash` and `qc` are **supplied by the caller** at
C5. State (S1), validator sets (B phases) and quorum certificates (B phases)
do not exist yet. C5 fixes their representation and validation — it does not
pretend to compute them.

## 2. Header wire layout

C2 rules: fixed-width little-endian integers, minimal LEB128 length prefixes,
strict `Option`/enum tags, no `usize`, no `HashMap`, no `serde`.

| # | Field | Type | Encoding |
|---|---|---|---|
| 1 | `version` | `u32` | LE, must equal `PROTOCOL_VERSION = 1` |
| 2 | `chain_id` | `ChainId` | `varint(len) ‖ UTF-8` |
| 3 | `height` | `Height` | `u64` LE |
| 4 | `parent_hash` | `Hash` | 32 raw bytes |
| 5 | `tx_root` | `Hash` | 32 raw bytes |
| 6 | `state_root` | `Hash` | 32 raw bytes |
| 7 | `validator_set_hash` | `Hash` | 32 raw bytes |
| 8 | `timestamp_ms` | `u64` | LE — **consensus value**, not a local clock |
| 9 | `proposer` | `Address` | 20 raw bytes |
| 10 | `qc` | `Option<Vec<u8>>` | tag `0`/`1`, then `varint(len) ‖ bytes` |
| 11 | `gas_used` | `Gas` | `u64` LE |
| 12 | `base_fee` | `Amount` | `u128` LE |

`Body` = `varint(count) ‖ Tx ‖ Tx ‖ …`. `MAX_BLOCK_TXS = 65536`,
`MAX_QC_BYTES = 65536`, both aligned with `aura-codec`'s `MAX_ITEMS`.

`HeaderParams` groups the 11 caller-supplied fields (`tx_root` is **absent**):
it is always derived from the body, so it cannot be forged at construction.

## 3. `tx_root` — binary BLAKE3 Merkle tree

```
leaf(i)  = BLAKE3(LEAF_CONTEXT ‖ txid_i)          txid = C4 Tx::txid()
node(l,r)= BLAKE3(NODE_CONTEXT ‖ l ‖ r)
```

* empty body → `Hash::ZERO` (well-defined sentinel);
* one leaf → the leaf itself is the root;
* at each level, an odd final node is **duplicated** before pairing;
* distinct contexts for leaves and nodes prevent second-preimage tricks.

The root is **order-sensitive**: this is the C5 DoD property. C6 adds proof
generation/verification over exactly this tree — the shape is fixed now so C6
does not have to change the root scheme.

## 4. `block_hash`

```
block_hash = hash_domain(chain_id, Domain::Block,
                         BLOCK_HASH_PAYLOAD_CONTEXT ‖ canonical(Header))
```

`hash_domain` is C3's length-prefixed framing
(`HASH_CONTEXT ‖ u64(len(prefix)) ‖ prefix ‖ u64(len(payload)) ‖ payload`,
`prefix = chain_id ‖ 0 ‖ "block" ‖ 0`). The body is not hashed directly;
`tx_root` commits to it. Changing any header field or the chain-id changes
the hash.

## 5. Canonical ordering and validation

`Body::new` sorts by `(Address, Nonce)` (both derive `Ord`) and rejects a
duplicate `(signer, nonce)` key. `Block::validate(expected_chain)` checks, in
order:

1. `version == PROTOCOL_VERSION`;
2. `chain_id == expected_chain`;
3. `body.len() <= MAX_BLOCK_TXS`;
4. `qc.len() <= MAX_QC_BYTES` (when present);
5. body is strictly ascending by `(signer, nonce)` → `UnsortedTransactions`;
6. duplicates → `DuplicateTxKey`;
7. recomputed `tx_root == header.tx_root` → `TxRootMismatch`.

`Block::validate_transactions(expected_chain)` is separate: it calls
`Tx::validate(expected_chain, header.height)` for each tx (signature, chain,
expiry, fees). Block-level validation alone does not verify signatures, so a
block can be checked structurally before expensive crypto work.

`decode_canonical()` rejects trailing bytes, non-minimal varints, unknown
enum/option tags and any input that does not re-encode byte-identically.

## 6. Golden vectors and negative controls

`tools/gen_c5_vectors.py` is independent Python: pure-Python BLAKE3
(self-checked against 7 official vectors), a C2/C3 wire transcription and the
committed C4 transaction vectors as bodies. `c5_golden.txt` has 95 lines:

* 4 canonical blocks with 2 / 3 / 0 / 1 transactions — covering even and odd
  tree levels, the empty tree, a real `b1 → b2` parent link, `Some(qc)` and
  u64/u128 maxima;
* an order-sensitivity pair (`tx_root_forward` ≠ `tx_root_backward`);
* 9 rejection vectors: trailing byte, truncation, bad version, bad root,
  reordered body, reordered body **with a recomputed root** (so only the
  ordering check can catch it), duplicate tx key, bad QC option tag,
  non-minimal body count.

`tools/nc_c5.py` runs 6 negative controls: both Merkle contexts, the block
hash context, a golden fixture, the `tx_root` check and the canonical-order
check. Total across C0–C5: **26 NC**.

## 7. Non-goals at C5

Merkle proofs (C6), accounts and `state_root` computation (C7/S1), genesis
(C8), fuzzing (C9), QC structure and BLS aggregation (B phases), consensus
timestamps from QC median (B3), gas metering and fee charging (X/D phases).
`timestamp_ms` is a value supplied by the caller — C5 never calls
`SystemTime::now()`.
