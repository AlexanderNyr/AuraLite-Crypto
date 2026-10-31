# D3 mempool contract

This document defines the bounded transaction-pool behavior provided by
`aura_mempool` and integrated by `aura_node::pipeline`.

## Admission boundary

The pool itself owns ordering and capacity policy, not signature or state
validation. The pipeline validates chain ID, expiry and signature before
insertion, then runs the native-transfer executor over the canonical pending
nonce sequence to reject nonce mismatch, insufficient balance, gas-floor and
fee-cap failures. A future nonce gap is rejected rather than left parked in the
pool. Callers of `Mempool::insert` must perform equivalent validation first.

## Sender lanes and priority

The pool is keyed by `(signer[20], nonce)`. Each sender forms a contiguous nonce
lane. `select_ready(base_nonces, limit)` initially exposes only the transaction
at that sender's next expected nonce, chooses the highest-tip ready head, then
makes that sender's next nonce eligible. Ties resolve by raw signer bytes and
nonce, in ascending order. Thus a high-tip transaction cannot leap over an
unsubmitted nonce. The pipeline re-encodes the selected set in canonical
`(signer, nonce)` order before execution and block construction.

For native transfers the D2 base fee remains one unit per gas and the priority
score is the signed transaction tip. Admission separately checks that
`max_fee >= base_fee + tip`.

## Capacity, eviction and replacement

The default pool capacity is **16,384**. At capacity, a new sender/nonce entry
is accepted only when its tip is strictly greater than the current lowest tip;
the deterministic lowest-tip entry is evicted (ties select the ascending
`(signer, nonce)` key). Equal or lower priority returns `PoolFull` without
changing pool contents.

A transaction at an occupied `(signer, nonce)` may be replaced only if its tip
is at least `old_tip + max(1, ceil(old_tip * 1000 / 10000))`. Arithmetic is
checked. Replacement preserves pool length and returns the replaced transaction
ID. Identical transaction IDs are rejected as duplicates.

## Proposed blocks and reorg reinsertion

Transactions are removed from the pool when selected into a proposed block;
account/nonce projection advances from the proposed branch while B3's three-
chain rule delays durable commit and events. A caller handling a discarded
branch can call `reinsert_reorged` for its transactions after revalidating them
against the new chain, nonce and balance view. D3 does not add distributed
reorg detection or change the D2 single-process finality/checkpoint boundary.

## Acceptance

`crates/aura-node/tests/d3_mempool.rs` submits 10,000 signed contiguous
transfers and verifies unique receipts, exact nonce/balance/gas accounting,
durable state-root agreement, and no commit event before three-chain finality.
Unit tests pin priority, nonce-lane eligibility, replacement, eviction,
duplicate rejection and reorg reinsertion.
