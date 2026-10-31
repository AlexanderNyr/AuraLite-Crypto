# D6 observability

`aurad` exposes operational probes on the existing HTTP listener (default
`127.0.0.1:8545`):

- `GET /health` returns JSON and HTTP 200 only when the PID file exists and the
  pipeline is immediately inspectable. It returns HTTP 503 otherwise.
- `GET /metrics` returns Prometheus text exposition 0.0.4. Scrape this endpoint
  with a Prometheus job; the sample Grafana dashboard is
  [`grafana-dashboard-d6.json`](grafana-dashboard-d6.json).

The metrics include committed block height, unfinalized proposal lag, mempool
size/capacity, committed gas total and a rolling 60-second gas/second rate,
committed block/transaction counters, health and whether OTLP is configured.
`aurad_peer_count` and `aurad_p2p_available` intentionally remain zero: D6 does
not implement N1 networking and does not report fictitious peers.

Logs are JSON objects on stderr. `AURA_LOG` or `RUST_LOG` controls filtering.
When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, `tracing` spans are exported using
OTLP over gRPC; for example:

```sh
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317 \
AURA_LOG=info aurad --chain-id auralite-test-1
```

The endpoint must be a reachable OTLP/gRPC collector. Invalid exporter
configuration fails startup; an absent endpoint leaves local JSON logs enabled
and explicitly reports `aurad_otel_exporter_enabled 0`. SIGTERM/SIGINT flushes
queued spans before process exit.

D6 instruments native-transfer submission and block-production spans, plus
committed gas and transactions. It adds no P2P, multi-validator, or contract
execution behavior.
