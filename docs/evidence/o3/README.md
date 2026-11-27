# O3 evidence — closed-testnet ops gate (10 scenarios)

Gate (OPS_PLAN row O3): "7 days uptime, 0 state losses, 1 restore drill".
Named control: restore_mismatch_refused. The receipts in `testnet-ops.txt` and the ledger
in `uptime-7d.txt` are measured by `aurad-ops` (the scripted 7-day
operator schedule on the real pipeline + the live backup/restore drill
over the shipped S2 backup format); `aura-ops check` validates the
receipt and `aura-ops ledger` independently re-derives the rollup.
`docs/oncall-runbook.md` is the invited-operators procedure.

- `test.log`: `cargo test -p aura-testkit --test o3_testnet_ops` (11 tests).
- `nc.log`: `tools/nc_o3.py` mutation controls (6/6).
- `receipts.log`: the measured `aurad-ops` run (schedule + drill).
- `testnet-ops-check.log`: `aura-ops check` + `aura-ops ledger`.
- `summary.log`: the pinned normalized report.
- `testnet-ops.txt` / `uptime-7d.txt`: the first O3 receipts (committed
  with the phase).
- `inventory.txt`: case -> NC mapping and sha256 of fixture/generator/
  runner/module/drill bin; ends with `TOTAL: 692`.
- `gates.txt` / `report.md` / `SHA256SUMS`: the receipts.

Regenerate with `make evidence-o3`; every sha256 in `SHA256SUMS` can be
recomputed with `sha256sum -c`.
