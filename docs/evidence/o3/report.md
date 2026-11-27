# O3 closeout report — closed testnet ops (uptime ledger + restore drill)

Date: 2026-11-25. Gate (OPS_PLAN row O3): "7 days uptime, 0 state losses,
1 restore drill" — **satisfied on measured receipts** (`testnet-ops.txt`
above; `aurad-ops` produced these values on the shipped pipeline and the
shipped S2 backup format — nothing in this bundle is fabricated for the
drill; the 7-day window is the scripted operator schedule of the closed
testnet (D-O3-3: day/hour from the schedule, heights and state roots from
the live chain).

- **1 restore drill, 0 state losses** (D-O3-2): real state through the
  pipeline (8 blocks with transfers and receipts), the shipped
  `aura_storage::create_backup` bundle (manifest + WAL records + sha256),
  the original store **destroyed**, then `restore_backup` — every record
  replayed through the real commit path and every root recomputed. The
  restored store reopens with a byte-identical head (the two
  `restore state_root` lines). Honest boundary: the D2 pipeline refuses
  to reopen a receipt-bearing store by design (restart checkpointing is
  a later node integration task), so restoration is verified at the S2
  storage layer the backup format targets.
- **7 days uptime** (D-O3-3): the scripted 7-day operator schedule (7 x
  24 hourly polls) on the real pipeline writes the committed ledger
  `uptime-7d.txt`; the rollup counts 7 complete day-buckets and zero
  head regressions. `aura-ops ledger` re-derives
  `uptime ledger: days=7 state_losses=0` independently (D-O3-5).
- **The gate**: `restore_mismatch_refused` (the named control) feeds the checker a receipt
  whose two `restore state_root` lines differ and expects
  `error: testnet-ops: restore mismatch` (exit 65); mutation NC-1
  (`tools/nc_o3.py`) drops the restore root-equality check and the case
  must turn red. "The restore did not restore -> CI red" — the O3 twin of
  `m0_root_drift_refused`.
- **Scenarios**: 10 deterministic cases from the independent oracle
  `tools/gen_o3_vectors.py`, asserted byte-for-byte by
  `tests/o3_testnet_ops.rs` (11/11 with the count guard) against
  `aura_testkit::ops`: the valid receipt (check + pinned summary), the
  restore-mismatch / state-losses / uptime-days / restore-drills /
  root-count / foreign-line refusals, and the ledger rollup (happy path +
  a refused state loss `state loss at 3 2`).
- **Mutation controls**: 6/6 PASS (restore root-equality [named],
  state-losses check, uptime-days check, restore-drills check, root-count
  check, unknown-line strictness).
- **Invited operators / on-call** (D-O3-4): `docs/oncall-runbook.md` —
  monitoring duties and alert thresholds over the uptime ledger, backup
  cadence and verification, the weekly restore drill, incident response
  (stall / state-loss / suspect-backup) and the written handover.

Measured receipt (testnet-ops.txt):

```
# closed-testnet ops receipts (O3) — measured by `aurad-ops`, checked by `aura-ops check`.
rule testnet-ops-v1
uptime days 7
state losses 0
restore drills 1
restore state_root cd2464aa5861e2c2efb9a747e7d6d48406895300cdf45799b1328937df773da3
restore state_root cd2464aa5861e2c2efb9a747e7d6d48406895300cdf45799b1328937df773da3
```

Receipts: `gates.txt`, `test.log`, `nc.log`, `receipts.log`,
`testnet-ops-check.log`, `summary.log`, `testnet-ops.txt`,
`uptime-7d.txt`, `inventory.txt`, `SHA256SUMS`. Regenerate with
`make evidence-o3`.
