# T7 Report — Treasury and grants

Date: 2026-11-21

The result: **ALL T7 GATES GREEN**

Gate: every payment has a proposal + tx + report; a withdrawal without a vote — red.

The `crates/aura-storage/src/treasury.rs` model: a treasury with a signed balance
(`i128`, an overdraft is visible and forbidden); a council of 5 votes on payment proposals
(a strict majority), a 2-of-3 multisig (decision W4) confirms; the life
cycle proposal -> vote -> finalize -> approve -> execute. Every payment gets a
**tx** number and a transparent `Report {tx_id, proposal_id, amount, recipient,
purpose, height}` (the treasury report prints the example as `T7-TX` lines). A withdrawal without
a voted proposal is rejected (`NoVote`, step 19 of the scenario); a payment
is not executed twice (`AlreadyExecuted`); a rejected proposal pays nothing
(`Rejected`); an amount larger than the balance — `InsufficientFunds`. The scenario
`sim/t7-treasury.json` (10 proposals, 82 steps: 6 payouts with tx+report,
2 rejections, a withdrawal attempt without a vote), the golden digest 719c93b2…3231f7
(Rust ↔ Python ↔ the `t7_treasury_example` example). The gate
`crates/aura-storage/tests/t7_treasury.rs` (10 tests + 2 metamorphic properties),
the oracle `tools/t7_treasury_check.py`.
negative controls `tools/nc_t7.py` (`nc.log`, RESULT: 7/7 negative controls PASS)
— 7 controls (the standard 6 + the literal NC of withdrawal without a vote).
