# D3 — bounded fee-priority transaction pool

Generated (UTC): 2026-10-30T07:41:00+00:00.

## Delivered behavior

`aura-mempool` implements a deterministic 16,384-entry pool keyed by raw
`(signer, nonce)`. It selects only contiguous sender-lane heads, prioritises
ready entries by signed tip, uses deterministic address/nonce tie-breaking,
requires a checked 10% replacement bump (rounded up, minimum one unit), and
only evicts the lowest-tip entry for a strictly higher tip. Duplicate IDs are
rejected. Reverted-branch transactions can be reinserted after caller-side
revalidation against the new account/nonce view.

`aura_node::pipeline` validates chain, expiry, signature, projected nonce,
balance, transfer gas floor and fee cap before pool insertion. The selected set
is encoded canonically by `(signer, nonce)` before C5 block production; S2
receipts/events remain behind the B3 three-chain durability boundary.

## Gate results

- 10,000-transfer acceptance: **passed**; unique durable receipts, sequential
  nonce, exact sender/recipient balances, gas, state root and event ordering.
- Mempool unit tests: **6/6 passed**.
- D3 negative controls: **6/6 passed**, all using exact Rust unit tests.
- Cumulative negative controls: **207 passed** by `make nc-all`.
- `make test`, dependency, docs, vector, rig and strict close-out gates: green
  on the source tree (raw output in `gates.txt`).
- Clean baseline application and clean-copy regression result are recorded in
  [`clean-apply.txt`](clean-apply.txt).

## Explicit boundaries

The pool's reorg hook only reinserts caller-provided transactions; the caller
must revalidate them against the new chain/account state. D3 does not add a
distributed reorg detector, multi-validator transport, contract transaction
dispatch or a persisted finality checkpoint. The D2 pipeline still fails closed
when reopening committed receipts without a checkpoint.

Protocol: [`../../spec/mempool.md`](../../spec/mempool.md).
