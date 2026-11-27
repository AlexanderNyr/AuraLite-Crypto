# O2 evidence — M0/M1 gates (10 scenarios)

Gate (OPS_PLAN row O2): "ROADMAP §5 M0 and M1 criteria met". Named control:
m0_root_drift_refused. The receipts in `milestone.txt` are measured by `aurad-receipt` on
the shipped pipeline/cluster; `aura-milestone check` validates them against
the criteria. `docs/how-to-run.md` is the operator quickstart.

- `test.log`: `cargo test -p aura-testkit --test o2_milestones` (11 tests).
- `nc.log`: `tools/nc_o2.py` mutation controls (6/6).
- `receipts.log`: the measured M0/M1 run (`aurad-receipt`).
- `milestone-check.log`: `aura-milestone check` over the measured receipts.
- `m1-acceptance.log`: the multi-process `d7_devnet` acceptance (four real
  `aurad` processes).
- `milestone.txt`: the first M0/M1 receipts (committed with the phase).
- `inventory.txt`: case -> NC mapping and sha256 of fixture/generator/
  runner/module/receipt bin; ends with `TOTAL: 686`.
- `gates.txt` / `report.md` / `SHA256SUMS`: the receipts.

Regenerate with `make evidence-o2`; every sha256 in `SHA256SUMS` can be
recomputed with `sha256sum -c`.
