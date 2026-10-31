# D2 — deterministic native-transfer pipeline

Generated (UTC): 2026-10-29T21:36:28+00:00.

## Delivered path

`aura_node::pipeline` validates signed C4 transactions, admits a bounded queue,
executes native transfers in canonical `(signer, nonce)` order, builds C5
blocks, drives the B3 one-validator BLS/QC and three-chain path, commits state
and receipts through the S2 WAL-backed store, and publishes commit events only
after storage succeeds.

The fixed D2 transfer payload is `recipient[20] || amount_be[16]`. Each
transfer consumes 21,000 gas at a base fee of one smallest unit per gas unit;
the base-fee portion is burned and the tip is credited to the local proposer.
Balances, fees, nonce increments and storage roots use checked arithmetic.

## Gate results

- Integration tests: **9/9 passed**, including 1,000 signed transfers,
  transaction receipts, balances, gas, delayed three-chain commit, durable
  event ordering, rejection paths and fail-closed reopen behavior.
- D2 negative controls: **6/6 passed**.
- Full project negative-control inventory: **201 controls**.
- `make test`, `make nc-all`, dependency, docs, vector, rig and close-out
  gates: all green on the source tree (raw output is in `gates.txt`).
- The patch applies to a fresh baseline copy; clean-copy `make test`, D2 NC 6/6
  and strict close-out audit passed. The clean-copy `nc-all` caveat is recorded
  explicitly in [`clean-apply.txt`](clean-apply.txt).

## Explicit limits

The composition is single-process and one-validator; it is not a multi-validator
devnet. Only native transfers execute; unsupported transaction kinds are
rejected. D2 persists S2 state and receipts, but does not yet persist a replayable
finality checkpoint. Reopening a store with committed receipts fails closed with
`RecoveryCheckpointRequired` rather than silently resetting consensus history.
RPC, full mempool policy, contract dispatch, transport and checkpoint recovery
remain later work.

Protocol details: [`../../spec/node-pipeline.md`](../../spec/node-pipeline.md).
