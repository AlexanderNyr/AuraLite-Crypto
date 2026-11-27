# O2 closeout report — M0/M1 gates (the milestone receipts)

Date: 2026-11-25. Gate (OPS_PLAN row O2): "ROADMAP §5 M0 and M1 criteria
met" — **satisfied on measured receipts** (`milestone.txt`, recorded above;
`aurad-receipt` produced these values on the shipped pipeline and cluster —
nothing in this bundle is fabricated).

- M0 (genesis lib): `make test` green; genesis -> 1000 tx -> byte-identical
  state root on 2 runs (`m0 state_root` lines identical).
- M1 (devnet): 4 validators, block interval <= 1 s, >= 100 committed
  blocks without desync, RPC returns a positive balance (the balance read
  goes through the shipped `aura_rpc::RpcService` dispatch; the socket path
  is the multi-process acceptance below).

- **The gate**: `m0_root_drift_refused` (the named control) feeds the checker a receipt
  whose two `m0 state_root` lines differ and expects `error: milestone:
  m0 state root drift` (exit 65); mutation NC-1 (`tools/nc_o2.py`) drops
  the root-equality check and the case must turn red.
- **Scenarios**: 10 deterministic cases from the independent oracle
  `tools/gen_o2_vectors.py`, asserted byte-for-byte by `tests/o2_milestones.rs`
  (11/11 with the count guard) against `aura_testkit::milestone`.
- **Mutation controls**: 6/6 PASS (root-equality check [named],
  validator-count check, desync check, height check, rpc-balance check,
  unknown-line strictness).
- **M1 acceptance (multi-process)**: `tests/d7_devnet.rs` — four real
  `aurad` processes, 100 finalized blocks with identical head/state_root,
  JSON-RPC balance after a faucet transfer, survival of a 1-of-4 outage
  (`m1-acceptance.log`).
- **Device model** (docs/o2-m0-m1-plan.md): D-O2-1 the checked receipt
  format (`rule milestones-v1`, strict, no commands); D-O2-2 M0 measured on
  the real D2/D3 pipeline (two fresh replays); D-O2-3 M1 measured on the
  real 4-validator `DevnetCluster` + the real RPC dispatch; D-O2-4 the
  "how to run" doc (`docs/how-to-run.md`) and these first receipts;
  D-O2-5 two independent implementations (Python oracle vs the shipped
  checker).

Measured receipt (milestone.txt):

```
# M0/M1 milestone receipts (O2) — measured by `aurad-receipt`, checked by `aura-milestone check`.
rule milestones-v1
m0 txs 1000
m0 state_root 8c5c26f4c76f733e5c4db2a1bc74437fe2f37c6e968a4b945c7626b9e98885aa
m0 state_root 8c5c26f4c76f733e5c4db2a1bc74437fe2f37c6e968a4b945c7626b9e98885aa
m1 validators 4
m1 block_ms 32
m1 height 100
m1 desync 0
m1 rpc_balance 250000
```

Receipts: `gates.txt`, `test.log`, `nc.log`, `receipts.log`,
`milestone-check.log`, `m1-acceptance.log`, `milestone.txt`,
`inventory.txt`, `SHA256SUMS`. Regenerate with `make evidence-o2`.
