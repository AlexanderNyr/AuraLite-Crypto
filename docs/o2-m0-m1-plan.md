# O2 — M0/M1 gates: the milestone receipts (design)

**Gate (OPS_PLAN):** ROADMAP §5 M0 and M1 criteria met. The criteria are the
single source of truth:

- **M0 — Genesis lib**: `make test` green; **genesis → 1000 tx →
  byte-identical state root on 2 runs**.
- **M1 — Devnet**: **4 validators, 1 block/s, 100 blocks without desync,
  RPC returns a balance**.

**Named scenario: `m0_root_drift_refused`.** M0's exit criterion is the
project's determinism doctrine (D3) as a measurement; a receipt whose two
state roots differ is `error: milestone: m0 state root drift` (exit 65).

The milestones are measured, not asserted: `aurad-receipt` runs the real
pipeline twice for M0 and a real 4-validator cluster for M1, writes the
measured `milestone.txt` receipt, and `aura-milestone check` validates the
receipt against the criteria. The multi-process acceptance
(`tests/d7_devnet.rs`, four real `aurad` processes over JSON-RPC) stays the
deep M1 gate inside `make test`; the "how to run" doc ships with the phase.

## Decisions

- **D-O2-1 (milestones are receipts).** `milestone.txt` (strict text):
  `rule milestones-v1`, `m0 txs <n>`, `m0 state_root <hex-64>` ×2,
  `m1 validators <n>`, `m1 block_ms <n>`, `m1 height <n>`, `m1 desync <n>`,
  `m1 rpc_balance <n>`. `aura-milestone check` refuses, in order: unknown
  lines (the format cannot carry commands), a wrong `rule`, missing lines,
  and then each criterion (`m0 tx count: …` must be 1000, the two
  `m0 state_root` lines must be identical — the named gate — `m1
  validators: …` must be 4, `m1 height: …` ≥ 100, `m1 desync: …` must be
  0, `m1 rpc balance: …` must be > 0, `m1 block rate: …` must be ≤ 1000 ms
  per block). `aura-milestone summary` prints the normalized report.
- **D-O2-2 (M0 measured on the real pipeline).** `aurad-receipt` runs the
  genesis → 1000 signed faucet transfers → committed state-root replay
  **twice** (fresh `DevnetCluster` stores each time) through the shipped
  D2/D3 pipeline and records both roots. Byte-identical roots are the M0
  criterion; drift is the named refusal.
- **D-O2-3 (M1 measured on the real cluster).** `aurad-receipt` opens the
  in-process 4-validator `DevnetCluster` (the same N1/N2 + B3–B5 stack the
  devnet uses), steps 100 committed blocks (measuring the block interval),
  compares all four heads (height + state root) for desync, then submits a
  faucet transfer and reads the recipient balance through the shipped
  JSON-RPC dispatch (`aura_rpc::RpcService` + `PipelineRpcBackend` — the
  same handler the HTTP listener calls; the socket path is proven by
  `tests/d7_devnet.rs`).
- **D-O2-4 (how to run + first receipts).** `docs/how-to-run.md` documents
  the build, the 4-validator devnet (`aurad` process flags as the D7 gate
  uses them), the RPC examples (`get_balance`), and the milestone
  measurement commands. `docs/evidence/o2/` carries the first receipts:
  `milestone.txt` (measured), `test.log`, `nc.log`, `receipts.log`,
  `milestone-check.log`, `m1-acceptance.log` (the multi-process D7 gate).
- **D-O2-5 (two implementations).** `tools/gen_o2_vectors.py` computes the
  expected verdicts of ten synthetic receipts independently (pure Python);
  `crates/aura-testkit/src/milestone.rs` implements the shipped checker and
  must match the fixture byte-for-byte
  (`tests/o2_milestones.rs` + `tests/fixtures/o2_milestone_vectors.json`).

## Test scenarios (10, fixture `o2_milestone_vectors.json`)

| # | name | scenario | exit |
|---|---|---|---|
| 1 | milestones_pass | the measured receipt satisfies M0 + M1 | 0 `milestones=ok m0=ok m1=ok` |
| 2 | milestone_summary_deterministic | `summary` prints the pinned report (byte-for-byte) | 0 |
| 3 | **m0_root_drift_refused (NAMED)** | two runs disagree -> `m0 state root drift` | 65 |
| 4 | m1_validator_count_refused | 3 validators -> `m1 validators: 3` | 65 |
| 5 | m1_desync_refused | heads disagree -> `m1 desync: 1` | 65 |
| 6 | m1_height_refused | 99 blocks -> `m1 height: 99` | 65 |
| 7 | m1_rpc_balance_refused | RPC balance 0 -> `m1 rpc balance: 0` | 65 |
| 8 | m0_tx_count_refused | 999 tx -> `m0 tx count: 999` | 65 |
| 9 | m1_block_rate_refused | 1500 ms/block -> `m1 block rate: 1500` | 65 |
| 10 | foreign_receipt_line_refused | `exec curl …` line -> `unknown receipt line: …` | 65 |

## Negative controls (`tools/nc_o2.py`, 6)

| NC | mutation in `milestone.rs` | must turn red |
|---|---|---|
| 1 | root-equality check dropped | `m0_root_drift_refused` (**named**) |
| 2 | validator-count check dropped | `m1_validator_count_refused` |
| 3 | desync check dropped | `m1_desync_refused` |
| 4 | height check dropped | `m1_height_refused` |
| 5 | rpc-balance check dropped | `m1_rpc_balance_refused` |
| 6 | unknown-line strictness dropped | `foreign_receipt_line_refused` |

## Make targets / evidence

- `make milestone-receipts` — `aurad-receipt` measures M0 + M1 and writes
  `release/milestone.txt`.
- `make milestone-check` — `aura-milestone check` over the measured receipt.
- `make {gen-o2-vectors,test-o2,nc-o2,evidence-o2}` — the phase loop.
- `docs/evidence/o2/`: `milestone.txt` (the first receipts), `test.log`,
  `nc.log`, `receipts.log`, `milestone-check.log`, `m1-acceptance.log`
  (multi-process `d7_devnet`), `report.md`, `gates.txt`, `SHA256SUMS`,
  `README.md`, `inventory.txt` (`TOTAL: 686`).
