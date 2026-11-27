# O3 — Closed testnet: monitoring, on-call, backup/restore drills (design of record)

Phase **O3** (OPS_PLAN #0106). Gate: *"7 days uptime, 0 state losses, 1
restore drill"* (OPS_PLAN row O3; the M2 exit criterion in `ROADMAP.md` §5
includes "7 days uptime, 0 state losses" for the public testnet — O3 is
the closed-testnet rehearsal of those very criteria). DoD: invited
operators, monitoring, on-call, backup/restore drills. Precedent:
`docs/o2-m0-m1-plan.md` (receipts, not adjectives).

## Device model — what is measured, and how

The doctrine of O2 holds verbatim: **the milestones are receipts**. The
three O3 criteria are operational, and each is turned into a measurement
with the deepest fidelity this workspace can honestly run:

- **D-O3-1 — the ops receipt** (`rule testnet-ops-v1`), strict text format,
  checked by `aura-ops check` (`crates/aura-testkit/src/ops.rs`):
  `uptime days <n>`, `state losses <n>`, `restore drills <n>`, and
  `restore state_root <hex-64>` x2 (the pre-backup head root and the
  post-restore head root). Unknown lines are refused (`unknown receipt
  line: ...`) so the format cannot carry commands.
- **D-O3-2 — the restore drill exercises the shipped S2 backup format.**
  `aurad-ops` (the measurer) opens a real `Pipeline`, commits real
  blocks (transfers + receipts), records the head (height + state root),
  drops the node, creates the backup bundle
  (`aura_storage::create_backup` — manifest + WAL records + sha256),
  **destroys the original store**, then restores from the bundle with
  `aura_storage::restore_backup` — every record is replayed through the
  real commit path and every root recomputed (a tampered bundle is
  rejected by the format itself). The restored store is reopened and its
  head compared with the recorded one. "1 restore drill, 0 state losses"
  means the two heads agree byte-for-byte. The named refusal
  `restore_mismatch_refused` (`error: testnet-ops: restore mismatch`,
  exit 65) is exactly this equality dropped — the operational twin of
  `m0_root_drift_refused`. (Honest boundary: the D2 pipeline refuses to
  reopen a receipt-bearing store by design — restart checkpointing is a
  later node integration task, `Pipeline::open` — so the drill verifies
  restoration at the S2 storage layer the backup format targets.)
- **D-O3-3 — the uptime ledger.** `release/uptime-7d.txt` records one
  heartbeat per operator poll: `uptime <day> <hour> <height> <root-hex>`
  (strict, ordered, one poll per slot). `aurad-ops` runs the closed
  testnet's **scripted 7-day operator schedule** (7 days x 24 hourly
  polls) against the real pipeline — day/hour come from the schedule,
  heights and roots come from the live chain. The rollup: `uptime days`
  counts day-buckets with all 24 polls present; `state losses` counts
  head regressions (height went backwards, or the same height was seen
  with a different root). `aura-ops ledger` re-derives the rollup from
  the committed ledger **independently** (D-O3-5) and the evidence script
  requires the two implementations to agree.
- **D-O3-4 — on-call and invited operators.** `docs/oncall-runbook.md`:
  monitoring duties (the uptime ledger + alert thresholds), incident
  response, backup cadence, restore-drill cadence, escalation. The
  runbook is the "invited operators / on-call" half of the DoD; the
  drills and the ledger are its measured half.
- **D-O3-5 — two independent implementations** (three with the oracle):
  `aura_testkit::ops` (Rust checker + rollup), `aurad-ops` (measurer
  rollup), `tools/gen_o3_vectors.py` (Python oracle). The evidence
  cross-checks the two Rust rollups over the same ledger.

## Receipt format (D-O3-1)

```
# closed-testnet ops receipts (O3) — measured by `aurad-ops`, checked by `aura-ops check`.
rule testnet-ops-v1
uptime days 7
state losses 0
restore drills 1
restore state_root <hex-64>   # pre-backup head root
restore state_root <hex-64>   # post-restore head root
```

Check order (the refusal messages are exact):

| order | condition | refusal message |
|---|---|---|
| 1 | line shape | `unknown receipt line: <line>` |
| 2 | `rule` missing / wrong | `missing rule line` / `bad rule: <name>` |
| 3 | required value missing | `missing receipt line: <name>` |
| 4 | roots != 2 | `restore state root count: <n>` |
| 5 | roots differ (**named**) | `restore mismatch` |
| 6 | `state losses` != 0 | `state losses: <n>` |
| 7 | `restore drills` < 1 | `restore drills: <n>` |
| 8 | `uptime days` < 7 | `uptime days: <n>` |

ok line: `testnet-ops=ok uptime=ok state=ok drills=ok`; summary
(`aura-ops summary`): `uptime: days=<n> state_losses=<n>` /
`restore: drills=<n> roots=identical|drift`. The ledger command prints
`uptime ledger: days=<n> state_losses=<n>` and refuses a loss with
`state loss at <day> <hour>` (exit 65) or a malformed row with
`bad heartbeat: <line>`.

## Scenarios (`tools/gen_o3_vectors.py` -> `tests/fixtures/o3_testnet_ops_vectors.json`)

1. `testnet_ops_pass` — the valid receipt (check).
2. `testnet_ops_summary_deterministic` — pinned summary.
3. `restore_mismatch_refused` — **NAMED** (nc 1): restored root differs.
4. `state_losses_refused` (nc 2) — `state losses: 1`.
5. `uptime_days_refused` (nc 3) — `uptime days: 6`.
6. `restore_drills_refused` (nc 4) — `restore drills: 0`.
7. `restore_root_count_refused` (nc 5) — one root only.
8. `foreign_ops_line_refused` (nc 6) — the format cannot carry commands.
9. `ledger_rollup_deterministic` — the ledger re-derivation.
10. `ledger_state_loss_refused` — a regressing head in the ledger.

Plus the count guard `vectors_file_has_exactly_10_unique_cases` (11/11).

## NC targets (`tools/nc_o3.py`, 6)

1. root-equality check dropped -> `restore_mismatch_refused` red (named);
2. state-losses check dropped -> `state_losses_refused` red;
3. uptime-days check dropped -> `uptime_days_refused` red;
4. restore-drills check dropped -> `restore_drills_refused` red;
5. root-count check dropped -> `restore_root_count_refused` red;
6. unknown-line strictness dropped -> `foreign_ops_line_refused` red.

## Make targets

`gen-o3-vectors`, `test-o3`, `nc-o3`, `evidence-o3`,
`testnet-ops-receipts` (`aurad-ops`: session + drill -> receipt +
ledger), `testnet-ops-check` (`aura-ops check`).

## Evidence (`docs/evidence/o3/`)

`test.log`, `nc.log`, `receipts.log` (the live `aurad-ops` run),
`check.log`, `ledger.log` (the independent rollup cross-check),
`uptime-7d.txt` (the committed closed-testnet ledger), `testnet-ops.txt`
(the measured receipt), `runbook.md` sha, `inventory.txt` (`TOTAL: 692`),
`gates.txt`, `report.md`, `SHA256SUMS`, `README.md`.
