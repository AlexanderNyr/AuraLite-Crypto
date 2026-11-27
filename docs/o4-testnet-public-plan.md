# O4 — Public testnet (M2): metrics, triage and the 10-validator window (design of record)

Phase **O4** (OPS_PLAN #0107). Gate: *"30 days, >= 99% uptime, >= 100 000
txs, 0 unfixed criticals"* (OPS_PLAN row O4; `ROADMAP.md` §5 M2). DoD:
faucet, explorer, docs, bug bounty, public metrics, >= 10 external
validators. Precedent: `docs/o3-testnet-ops-plan.md` (receipts, not
adjectives). Stage 1 of the operations plan ends here.

## Device model — what is measured, and how

- **D-O4-1 — the public testnet receipt** (`rule testnet-v1`), strict
  text format, checked by `aura-testnet check`
  (`crates/aura-testkit/src/testnet.rs`):
  `days <n>` (>= 30), `uptime <n>` (percent, >= 99), `validators <n>`
  (>= 10), `txs <n>` (>= 100 000), `criticals <n>` (== 0). Unknown lines
  are refused (`unknown receipt line: ...`) so the format cannot carry
  commands. The named refusal `unfixed_criticals_refused`
  (`error: testnet: criticals: <n>`, exit 65) is the zero-criticals gate
  dropped — the public twin of `restore_mismatch_refused`.
- **D-O4-2 — the public metrics ledger.** `release/metrics-30d.txt`
  records one row per operator poll:
  `metrics <d> <h> <height> <txs_total> <root-hex-64>`. The rollup
  (`aura-testnet metrics`) counts complete day-buckets, derives uptime as
  `floor(100 * present / (span_days * 24))` and reports the total
  transaction count; a regressing height or transaction total is refused
  at its first occurrence (`tx regression at <d> <h>`) — the metrics
  twin of the O3 state-loss rule. Rows are strictly formatted and
  duplicate slots are refused.
- **D-O4-3 — the bug-bounty triage ledger.** `release/triage.txt` is the
  findings journal (`triage <id> <severity> <status>`; severity
  `critical|high|medium|low|informational`, status
  `open|fixed|accepted|not-applicable`). The rollup
  (`aura-testnet triage`) prints
  `triage ledger: items=<n> open=<n> criticals=<n>` where `criticals`
  counts **open** findings in the critical class (`critical`/`high`, the
  H8 gate convention "0 unfixed critical/high"). An open critical is
  refused by id (`open critical <id>`) — the named control
  `unfixed_criticals_refused` is exactly this check dropped. The journal
  seeds from the real H8 audit/bounty findings
  (`docs/audit/h8-public-audit-report.md`, machine copy
  `tools/fixtures/h8_findings.json`); new public-testnet findings are
  appended by the triage process, never fabricated by the measurer.
- **D-O4-4 — the 10-validator measurement.** `aurad-testnet` opens ten
  independent `Pipeline` nodes (distinct validator seeds, one genesis
  allocation set) and records `validators <n>` = the count whose genesis
  state root matches the first node's root byte-for-byte. External
  validators are an organizational criterion (the O3 method): the
  measurement proves ten independent nodes reproduce the same genesis;
  onboarding happens through `docs/testnet-public.md`. `DevnetCluster`
  tops out at 4 validators, so the window is measured as ten independent
  single-validator pipelines.
- **D-O4-5 — the public surface.** `aurad-faucet` and `aurad-explorer`
  (`crates/aura-node/src/bin/`) are the public services over the same
  `RpcBackend` (`PipelineRpcBackend`) the node RPC exposes: the faucet
  pays capped `payout <address> <units> <nonce>` requests through
  `submit_tx` with a deterministic per-run budget, the explorer answers
  `head` / `block <h>` / `tx <hash>` / `balance <address>` queries
  through `get_block`/`get_tx`/`get_balance`. `docs/testnet-public.md`
  is the public guide (faucet, explorer, metrics, bounty);
  `docs/testnet-bounty.md` is the bounty policy (scope, response SLA,
  safe harbor — it inherits the H8 program in `SECURITY.md`).

## The measurer never fabricates

`aurad-testnet` (`crates/aura-node/src/bin/`) measures the schedule
window on the shipped pipeline (30 x 24 hourly polls, 140 transfers per
poll = 100 800 transactions; heights, roots and the transaction total
come from the live chain), opens the ten validator nodes and reads the
triage journal. The receipt `release/testnet.txt` and the ledger
`release/metrics-30d.txt` are what the run measured; `aura-testnet`
validates them and re-derives the rollups independently.

## Scenarios and controls

12 deterministic cases from the independent oracle
`tools/gen_o4_vectors.py`, asserted byte-for-byte by
`tests/o4_testnet_public.rs` (13 tests: 12 cases + the count guard)
against `aura_testkit::testnet`: the gate pass and the pinned summary,
the five gate refusals (days / uptime / validators / txs /
criticals [named]), the foreign-line refusal, and the two ledger pairs
(metrics pass + `tx regression at 3 2`; triage pass + `open critical
T-0002`). Mutation controls `tools/nc_o4.py` (7): the named
criticals check, the four remaining gate checks, the metrics regression
check and the triage open-critical check.

## Honest boundaries

- The 30-day window is the scripted operator schedule run locally (the
  same fidelity as O3's 7-day schedule): days and hours come from the
  schedule, heights/roots/tx counts from the live chain. Wall-clock
  uptime of a hosted deployment is an ops duty
  (`docs/oncall-runbook.md`), not something this workspace can measure.
- Ten "external" validators are ten independent nodes in this workspace;
  external operation is organizational (invited operators, the O3
  model), proven here by identical genesis reproduction.
- `criticals` in the receipt is counted from the triage journal
  (D-O4-3); the measurer reads the journal and refuses to write a
  passing receipt while an open critical exists.
