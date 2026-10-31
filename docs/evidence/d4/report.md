# D4 — JSON-RPC 2.0 evidence receipt

Generated (UTC): 2026-10-30T12:25:44+00:00.

## Delivered behavior

`aura-rpc` implements JSON-RPC 2.0 request, batch, notification and error
semantics over bounded HTTP (`POST /rpc`, also `/`) and RFC 6455 WebSocket
(`/ws`). The pipeline-backed adapter exposes transaction submission, balance
and transaction queries, recent committed blocks, gas estimation, and durable
commit subscriptions. WebSocket subscriptions unregister on drop, including
when no later commit arrives.

Only native transfers execute. Contract calls and unsupported transaction
execution return explicit errors; D4 does not add P2P, multiple validators,
smart-contract execution or checkpoint recovery. HTTP bodies and WebSocket
messages are capped at 1 MiB, batches at 32 requests, concurrent connections
at 128, and incomplete HTTP requests time out after 15 seconds.

## Gate results

- `make test-d4`: RPC library **11/11**, 50 golden vectors **1/1**, node-backed
  contract tests **2/2**, managed-subscription lifecycle unit test **1/1**.
- `make nc-d4`: **6/6** unit-test-based mutation controls.
- `make nc-all`: **213/213** cumulative negative controls across D0–D4 and
  earlier phases; per-phase receipts are summarized in `inventory.txt`.
- Clean-copy `make test`: passed, including workspace formatting, workspace
  clippy with warnings denied, workspace tests, docs links, dependency checks,
  vector inventory, rig wiring and the close-out audit. Raw output:
  `make-test.log`.
- `fuzz_rpc`: 60-second libFuzzer shard across HTTP and WebSocket parser entry
  points; **733,119 executions, 0 crashes**, peak RSS 469 MiB. Raw output:
  `fuzz-rpc.log`. This is not the full H3 campaign.
- Clean-baseline apply and revalidation: see `clean-apply.txt`.

## Remaining H3 scope

The full 24-hour campaign and planned p2p-frame, WASM-verifier and wallet-
keystore targets remain H3 work; D4 claims only its RPC parser shard.

Protocol: [`../../spec/json-rpc.md`](../../spec/json-rpc.md).
