# D2 node transaction pipeline

This specification fixes the D2 in-process transaction path. It does not add a
network protocol or RPC API; those belong to D4 and NET.

## Transfer transaction payload

A D2 `TxKind::Transfer` payload is exactly 36 bytes:

```text
recipient[20] || amount_be[16]
```

`amount_be` is the unsigned `u128` number of smallest native units. Any other
payload length is rejected. The signed C4 transaction envelope still provides
chain ID, signer, nonce, expiry, gas limit, fee cap and tip.

## Deterministic execution

- Base fee: one smallest unit per gas unit.
- Transfer gas: exactly 21,000.
- Effective gas price: `base_fee + tip`; it must not exceed `max_fee`.
- The sender must have the expected sequential nonce and enough funds for the
  value plus the actual gas fee.
- The base-fee portion is burned; `tip * gas_used` is credited to the local
  proposer. All arithmetic is checked.
- D2 executes native transfers only. `Deploy`, `Call`, `Delegate` and
  `Undelegate` are rejected explicitly until their node-level dispatch is
  integrated; a transaction is never acknowledged as executed merely because
  its signature is valid.

## Block, finality, storage and event order

1. Validate chain, signature, expiry and D2 admission constraints.
2. Sort transactions by `(signer, nonce)` and execute in that order.
3. Build a C5 block with the deterministic timestamp, computed state root,
   transaction root, current validator-set commitment and prior QC.
4. Obtain a BLS precommit QC from the local one-validator B3 composition.
5. Wait for the B3 three-chain rule to finalize the block.
6. Commit account updates and canonical receipts to S2 storage; require the
   committed root to equal the header's state root.
7. Publish the commit event only after storage succeeds.

The in-process D2 composition is for deterministic integration and local use;
it is not a multi-validator devnet. A one-validator QC is not a substitute for
NET transport, validator-set management or devnet testing.

## Recovery boundary

S2 state and receipts are durable. D2 does not yet persist/replay the complete
finality checkpoint; if committed receipts are found on reopen, the pipeline
fails closed with `RecoveryCheckpointRequired` rather than silently anchoring a
new consensus history over existing state. Checkpoint recovery and reorg-aware
pool behavior remain later node work.
