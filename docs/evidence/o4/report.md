# O4 closeout report — public testnet (M2) window and public surface

Date: 2026-11-27. Gate (OPS_PLAN row O4): "30 days, >= 99% uptime, >=
100 000 txs, 0 unfixed criticals" — **satisfied on measured receipts**
(`testnet.txt` above; `aurad-testnet` produced these values on the
shipped pipeline: the scripted 30-day public schedule (30 x 24 hourly
polls, 140 transfers per poll = 100 800 transactions — day/hour from the
schedule, heights/roots/tx totals from the live chain), the
ten-validator window and the bug-bounty triage journal).

- **100 800 transactions, 30 days, 100% uptime** (D-O4-2): the
  committed metrics ledger `metrics-30d.txt`; the rollup re-derives
  `metrics ledger: days=30 uptime=100 txs=100800` independently and
  refuses any regressing window (`tx regression at <d> <h>`).
- **0 unfixed criticals** (D-O4-3): the triage journal `triage.txt`
  seeds from the real H8 audit/bounty findings
  (`tools/fixtures/h8_findings.json`); `aura-testnet triage` re-derives
  `triage ledger: items=12 open=3 criticals=0` and refuses an open
  critical-class finding by id (`open critical <id>`). The named gate
  `unfixed_criticals_refused` feeds the checker a receipt with `criticals 1` and expects
  `error: testnet: criticals: 1` (exit 65); mutation NC-1 drops the
  criticals check and the case must turn red.
- **10 validators** (D-O4-4): `aurad-testnet` opens ten independent
  single-validator pipelines (distinct seeds, one genesis allocation
  set) and records `validators 10` only when all ten genesis state roots
  agree byte-for-byte. External operation is organizational (the O3
  invited-operator method; onboarding via `docs/testnet-public.md`).
- **The public surface** (D-O4-5): `aurad-faucet` (capped payouts
  through the node `RpcBackend::submit_tx`) and `aurad-explorer`
  (`head`/`block`/`tx`/`balance` queries through
  `get_block`/`get_tx`/`get_balance`) — exercised in
  `public-surface.log`; `docs/testnet-public.md` is the public guide and
  `docs/testnet-bounty.md` the bounty policy (H8 program inherited).
- **Scenarios**: 12 deterministic cases from the independent oracle
  `tools/gen_o4_vectors.py`, asserted byte-for-byte by
  `tests/o4_testnet_public.rs` (13 tests: 12 scenarios + the count
  guard) against `aura_testkit::testnet`: the gate pass and pinned
  summary, the five gate refusals (days/uptime/validators/txs/criticals
  [named]), the foreign-line refusal, and the two ledger pairs (metrics
  pass + `tx regression at 3 2`; triage pass + `open critical T-0002`).
- **Mutation controls**: 7/7 PASS (criticals [named], days, uptime,
  validators, txs, metrics regression, triage open-critical).

Measured receipt (testnet.txt):

```
# public testnet receipts (O4) — measured by `aurad-testnet`, checked by `aura-testnet check`.
rule testnet-v1
days 30
uptime 100
validators 10
txs 100800
criticals 0
```

Receipts: `gates.txt`, `test.log`, `nc.log`, `receipts.log`,
`testnet-check.log`, `summary.log`, `public-surface.log`, `testnet.txt`,
`metrics-30d.txt`, `triage.txt`, `inventory.txt`, `SHA256SUMS`.
Regenerate with `make evidence-o4`.
