# D5 gRPC + REST contract

D5 adds a protobuf gRPC service on its own HTTP/2 listener and a versioned REST
gateway on the existing bounded D4 HTTP listener. Both adapters translate to
the same D4 `RpcService`, so D5 does not bypass transaction validation, D3
admission, finality, or storage.

## Listener and routes

- D4 JSON-RPC and REST share `rpc_addr` (default `127.0.0.1:8545`).
- Protobuf gRPC binds `grpc_addr` (default `127.0.0.1:9090`), configurable by
  CLI `--grpc-addr`, environment `AURA_GRPC_ADDR`, or TOML `grpc_addr`.
- The REST gateway serves `/api/v1/...` and `/api/v1.1/...`, plus `/openapi.yaml`
  and `/openapi.json`.
- Version 1.0 responses use `{ "result": ... }`. Version 1.1 adds
  `apiVersion: "1.1"` while preserving the exact `result` member. Old clients
  that read only `result` continue to work.
- JSON request bodies have closed schemas. Unknown fields, malformed JSON,
  unsupported API versions and unsupported transaction kinds fail with a
  structured non-2xx response; fields are never silently ignored.

REST endpoints:

| Method/path | D4 operation |
|---|---|
| `GET /api/v{1,1.1}/balances/{address}` | `get_balance` |
| `POST /api/v{1,1.1}/transactions` (`{"tx":"0x…"}`) | `submit_tx` |
| `GET /api/v{1,1.1}/transactions/{txid}` | `get_tx` |
| `GET /api/v{1,1.1}/blocks/{height\|latest}` | `get_block` |
| `POST /api/v{1,1.1}/gas-estimate` (`{"kind":"transfer"}` or `{"tx":"0x…"}`) | `estimate_gas` |

The complete machine-readable definition is [`../api/openapi-d5.yaml`](../api/openapi-d5.yaml)
and is served from both OpenAPI routes.

## Protobuf compatibility

`crates/aura-grpc-rest/proto/aura/api/v1/service.proto` is the authoritative
schema (`package aura.api.v1`, service `AuraNode`). It exposes transaction
submission, balance/transaction/block reads, gas estimation, and contract call.
Existing request field numbers and RPC paths are stable. Response field 15,
`api_version`, is additive metadata for v1.1; a v1 client decodes the original
field set while ignoring the new field under normal protobuf rules. The test
suite encodes a v1.1 balance response and decodes it with a v1 message layout.

REST uses a stricter rule than protobuf wire decoding: unknown JSON body fields
are rejected with `-32602` in the structured error body (HTTP 400). Protobuf
wire unknown fields remain forward-compatible and are ignored by the prost
codec; an unknown RPC method is rejected by tonic as `Unimplemented`.

## Error and execution boundaries

D4 RPC errors map to appropriate gRPC statuses (`InvalidArgument`,
`FailedPrecondition`, `Unimplemented`, or `Internal`). Contract calls remain
explicitly unsupported and map to `Unimplemented`; non-transfer estimation
remains an error. Only native transfers execute. D5 does not add multi-validator
transport or a new execution path.

`make test-d5` runs REST unit contract tests and a live HTTP + HTTP/2 gRPC
contract test. `make nc-d5` applies six mutations and requires each exact Rust
library unit test to turn red.
