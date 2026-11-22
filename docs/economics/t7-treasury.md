# T7 Treasury and grants

**Scope**: ECONOMICS_PLAN T7 (treasury and grants) — an on-chain treasury
with an m-of-n multisig (decision **W4**), transparent reporting and a
grants process. **Legal perimeter**: this document is an engineering
description of the instrument; the plain wording is an instrument, not
legal advice (see `disclaimer.md`; ECONOMICS_PLAN T8 / `docs/ecosystem/e8-paper.md`
set the perimeter).

## Model

- **Treasury**: signed balance (`i128`, so an overdraw would be visible
  rather than wrapping); the pinned scenario starts at `500 000 ALCRY`
  with one transparent inflow of `200 000 ALCRY`.
- **Council**: `T7_COUNCIL = 5` members vote on payout proposals;
  a proposal passes with a **strict majority** (`yes > 2`).
- **Multisig (W4)**: `T7_SIGNERS = 3` with `T7_SIGNER_THRESHOLD = 2`
  (2-of-3); a passed proposal needs that many approvals to execute.
- **Lifecycle**: `propose -> vote -> finalize -> approve -> execute`.
  Execution numbers the **tx** and creates the **report** in one atomic
  step.

## Gate: every payout has proposal + tx + report

Every executed payout (`Payout`) carries:

1. **proposal** — `proposal_id` of the authorizing proposal, which must
   be in state `Executed`;
2. **tx** — `tx_id`, numbered from 1 in execution order;
3. **report** — a transparent `Report {tx_id, proposal_id, amount,
   recipient, purpose, height}` mirroring the payout fields.

A **withdrawal without a vote** is rejected: `execute` on a proposal that
has not passed its council vote returns `T7Error::NoVote` (the literal
gate control; `t7_a_withdrawal_without_a_vote_must_be_rejected`, run
step 19). The treasury cannot overdraw (`InsufficientFunds` before any
balance change; conservation `final + paid = initial + inflows` is
asserted); a payout cannot execute twice (`AlreadyExecuted`); a rejected
proposal never pays out (`Rejected`).

## Grants process (numbered)

1. **Proposal** — a council member proposes `amount`, `recipient`,
   `purpose` (this document is the public grant description).
2. **Council vote** — every member votes once (`AlreadyVoted` guards
   double votes); `finalize` closes the vote.
3. **Multisig** — `T7_SIGNER_THRESHOLD` signers approve (`W4`).
4. **Execution** — tx + report are created; the example prints the
   treasury report as `T7-TX` lines.
5. **Reporting** — every payout's `Report` is public data (transparent
   reporting); nothing here is unaudited by construction.

## Determinism and oracle

The pinned scenario [`run_t7_scenario`] (`crates/aura-storage/src/treasury.rs`)
drives 10 proposals: P1 valid grant (100 000), P2 rejected by vote,
P3 **withdrawal without vote** (rejected), P4 multisig threshold then
valid payout, P5 insufficient funds, P6 double execution, P7 double vote,
P8 double approval, P9 inflow + grant round (250 000), P10 council tie
(rejected). The final step reports `balance = 225 000 ALCRY`,
`payouts = 6`, `paid = 475 000 ALCRY`. Digest:
`719c93b244ff42ebf94538a5e9fb5b9e8f52eac07035abaf3c526e4c0a3231f7`.

Oracle: `tools/t7_treasury_check.py` (independent Python step-log model,
same digest) cross-checks the Rust run, `tests/t7_treasury.rs` and
`sim/t7-treasury.json` (schema `auralite-sim-1`).

## Named messages (governance)

1. `T7: a withdrawal without a vote must be rejected`
2. `T7: every payout must have a proposal, a tx and a report`
3. `T7: the multisig threshold must be enforced`
4. `T7: the treasury must not overdraw`
5. `T7: a payout must not execute twice`
6. `T7: a rejected proposal must not pay out`
7. `T7: the reference simulation must agree with the pinned digest`

Each named message is a pinned gate test with named failing output;
`tools/nc_t7.py` proves 7/7 mutations go red (the literal NC:
withdrawal without vote -> red is mutant 1).
