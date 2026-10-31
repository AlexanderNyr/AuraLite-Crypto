# D6 — observability evidence receipt

Generated (UTC): 2026-10-30T20:55:45+00:00.

## Delivered behavior

`aurad` emits JSON structured logs and optionally exports OpenTelemetry spans
through OTLP/gRPC when `OTEL_EXPORTER_OTLP_ENDPOINT` is configured. The shared
bounded HTTP listener serves `/health` (200 only when the PID file and pipeline
are ready; 503 otherwise) and Prometheus text exposition at `/metrics`.

Pipeline snapshots export committed height, finality lag, mempool occupancy and
capacity. Durable commits update block/transaction/gas counters and a rolling
60-second committed gas-per-second gauge. The Grafana dashboard covers height,
finality lag, mempool size, peer count and gas/s. P2P is not implemented, so
peer count and availability are deliberately zero.

D6 does not add P2P, multi-validator networking, contract execution, or claim
those capabilities work. Details: [`../../observability/README.md`](../../observability/README.md).

## Gate results

- `make test-d6`: 4 observability unit tests, 1 structured-JSON unit test, 1
  in-memory OTLP span-export unit test, and 2 live HTTP endpoint contract tests.
- `make nc-d6`: **6/6** unit-test mutation controls reddened their exact named
  library tests; the restored aura-node library suite passed.
- `make nc-all`: **225/225** cumulative negative controls through D6; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean D5-baseline apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
- Dependency audit records the exact D6 OTLP dependency closure in
  `../d6-dependency-addendum.txt`; all lockfile packages remain checksummed and
  allowlisted.
