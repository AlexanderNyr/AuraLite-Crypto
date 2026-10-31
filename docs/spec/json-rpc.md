# D4 JSON-RPC 2.0 contract

This document fixes the D4 request/response surface and transport behavior. It
is an application API, not a consensus wire format. RPC arguments never bypass
`Tx::from_canonical_bytes`, `Pipeline::submit`, D3 admission, finality or S2
persistence.

## Transports and resource limits

- HTTP JSON-RPC: `POST /rpc` (also `POST /`), `Content-Type: application/json`.
- WebSocket: RFC 6455 upgrade at `GET /ws`; text messages carry JSON-RPC 2.0.
  The standard WebSocket accept-key GUID/SHA-1 handshake is used. Client frames
  must be masked; binary and fragmented messages are rejected.
- HTTP bodies and WebSocket messages are capped at 1 MiB; batches contain at
  most 32 requests; at most 128 concurrent TCP connections are admitted.
  Incomplete HTTP requests time out after 15 seconds; Transfer-Encoding/chunked
  requests are rejected rather than partially parsed.
- JSON-RPC notifications have no response body (HTTP uses 204); malformed
  JSON is `-32700`, invalid request envelopes `-32600`, unknown methods
  `-32601`, and invalid method parameters `-32602`.
- A backend call returns owned data before response serialization or socket
  writes. No node-state lock is held while waiting on network I/O.

## Request methods

All requests use `{"jsonrpc":"2.0","id":...,"method":"...","params":{...}}`.
IDs may be strings, integers or null. `params` may be an object or array;
method-specific inputs below use objects.

| Method | Parameters | Result / behavior |
|---|---|---|
| `submit_tx` | `{"tx":"0x<canonical-signed-tx-bytes>"}` | `0x`-prefixed transaction hash. Canonical decoding, chain/signature/expiry checks, nonce/balance/gas/fee validation and bounded pool admission all run before success. |
| `get_balance` | `{"address":"aura1…"}` | `{address,balance,nonce}`. `balance` is a decimal string in smallest units; unknown addresses read as zero balance/nonce. |
| `get_tx` | `{"txid":"0x<64 hex chars>"}` | `null` if unknown; `{txid,status:"pending"}` for D3 pool entries; or `{txid,status:"committed",receipt:{…}}` after durable commit. |
| `get_block` | `{}` / `{"height":null}` for latest, or `{"height":N}` | `null` if that height is outside the retained window; otherwise block hash, parent/state/transaction roots, chain, timestamp, gas/base fee and transaction count. The node retains the latest 256 committed blocks in memory; historical block replay is not yet persisted. |
| `estimate_gas` | `{"kind":"transfer"}` or `{"tx":"0x…"}` | `{gas:"21000",gasUnits:21000}` for native transfers. Other transaction kinds return `-32004`. |
| `call` | object | Recognized but returns `-32004` while contract execution is not available in the native-transfer-only D2 executor. It never fabricates a result. |
| `subscribe` | `{}`; WebSocket only | Acknowledges `{subscription:"sub-N"}` and streams durable-commit notifications as `{"jsonrpc":"2.0","method":"subscription","params":{"subscription":"sub-N","result":…}}`. HTTP returns `-32005`. |

`submit_tx` accepts raw canonical C4 transaction bytes, not a JSON object that
could accidentally omit signed fields. Amounts and gas values that can exceed
JavaScript's exact integer range are returned as decimal strings. Node-specific
errors use `-32004` (unsupported execution), `-32005` (transport restriction),
and `-32010` (transaction rejected); the original human-readable reason is in
`error.message`.

## Commit event and state-lock boundary

A WebSocket subscription is attached to the pipeline's post-storage event
channel. Proposed blocks and merely finalized-but-not-persisted state are never
announced. The adapter holds the pipeline mutex only to create the subscription
or obtain an owned snapshot; the transport serializes and writes after the
mutex is released. Slow clients do not block transaction execution or commit.

## Honest scope boundaries

The local node runs the existing one-validator B3 composition and advances
block production only while transactions await inclusion/finalization; it is not
a P2P/multi-validator devnet. The default
validator seed is development-only and is logged as such; `--validator-key`
accepts exactly 32 raw bytes. This D4 shell starts with no funded genesis
allocations, so funded accounts must be supplied by a later genesis/devnet
phase. Contract `call` and non-transfer gas estimation are explicit errors.
The block lookup window is process-local, and restart checkpoint recovery
remains fail-closed as documented by D2.

## Contract vectors

`crates/aura-rpc/tests/fixtures/d4_json_rpc.txt` contains 50 wire-level vectors
for valid methods, notifications, batches, malformed JSON, invalid IDs, invalid
params and bounded requests. `make test-d4` runs those vectors, actual HTTP and
WebSocket transport tests, and a node-backed transfer/query/subscription
contract test. `make nc-d4` mutates the JSON-RPC version check, notification
behavior, batch/body limits, handshake GUID and client-frame masking; each
mutation must make one exact Rust library unit test fail.
