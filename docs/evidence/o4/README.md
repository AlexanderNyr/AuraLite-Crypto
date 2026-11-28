# O4 evidence — public testnet (M2) gate (12 scenarios)

Gate (OPS_PLAN row O4): "30 days, >= 99% uptime, >= 100 000 txs, 0
unfixed criticals". Named control: unfixed_criticals_refused. The receipts in
`testnet.txt` and the ledgers in `metrics-30d.txt` / `triage.txt` are
measured by `aurad-testnet` (the scripted 30-day public schedule on the
real pipeline, 140 transfers per poll, plus the ten-validator window);
`aura-testnet check/metrics/triage` validate them and independently
re-derive the rollups. `docs/testnet-public.md` and
`docs/testnet-bounty.md` are the public guide and bounty policy.

- `test.log`: `cargo test -p aura-testkit --test o4_testnet_public` (13 tests).
- `nc.log`: `tools/nc_o4.py` mutation controls (7/7).
- `receipts.log`: the measured `aurad-testnet` run (schedule + validators + journal).
- `testnet-check.log`: `aura-testnet check` + `metrics` + `triage`.
- `summary.log`: the pinned normalized report.
- `public-surface.log`: `aurad-faucet` + `aurad-explorer` over the node RpcBackend.
- `testnet.txt` / `metrics-30d.txt` / `triage.txt`: the first O4 receipts
  (committed with the phase).
- `inventory.txt`: case -> NC mapping and sha256 of fixture/generator/
  runner/module/bins/docs; ends with `TOTAL: 699`.
- `gates.txt` / `report.md` / `SHA256SUMS`: the receipts.

Regenerate with `make evidence-o4`; every sha256 in `SHA256SUMS` can be
recomputed with `sha256sum -c`.
