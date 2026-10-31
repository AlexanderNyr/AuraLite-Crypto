# D5 — gRPC + REST evidence receipt

Generated (UTC): 2026-10-30T17:00:59+00:00.

## Delivered behavior

`aura-grpc-rest` provides a protobuf gRPC service on a separate configurable
listener and a versioned REST gateway on the existing bounded D4 HTTP listener.
Both adapters call the same D4 `RpcService`; neither bypasses transaction
validation, D3 admission, finality or storage. REST v1.1 adds `apiVersion`
without changing `result`. Closed JSON request schemas reject unknown fields
and unsupported versions. OpenAPI 3.1.0 definitions for API 1.1.0 are served as
both YAML and JSON.

Only native transfers execute. Contract calls and unsupported transaction
kinds remain explicit errors. D5 does not add contract execution, P2P,
multi-validator networking or claim unsupported operations succeed. Protobuf
v1 response fields and RPC paths are preserved; version metadata is additive.

## Gate results

- `make test-d5`: 7 library unit tests, 1 live REST + HTTP/2 gRPC contract test,
  and 4 daemon CLI tests passed.
- `make nc-d5`: **6/6** exact library unit-test mutations reddened their named
  tests; post-restoration unit-test check is green.
- `make nc-all`: **219/219** cumulative negative controls passed across all
  phases through D5; per-phase inventory is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — format, workspace Clippy with warnings
  denied, workspace tests, documentation guard, dependency checks, vector
  inventory, test-rig wiring, and close-out audit.
- Clean D4-baseline patch application and revalidation: see `clean-apply.txt`.

Protocol: [`../../spec/grpc-rest.md`](../../spec/grpc-rest.md).
OpenAPI: [`../../api/openapi-d5.yaml`](../../api/openapi-d5.yaml) and
[`../../api/openapi-d5.json`](../../api/openapi-d5.json).
