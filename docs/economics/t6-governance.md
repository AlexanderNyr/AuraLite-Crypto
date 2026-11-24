# T6 — Governance (ECONOMICS_PLAN)

**Status:** ✅ closed (2026-11-21). **Gate:** 20 proposals: valid ones pass,
flash-stake attack rejected — met literally.

## Decisions

- **Proposal threshold.** A proposal can be created by a stake holder with ≥
  `T6_PROPOSAL_THRESHOLD` = **10 000 `ALCRY`** (boundary: exactly 10 000 —
  allowed; less — `BelowThreshold`, red).
- **Quorum.** Voting power ≥ **2000 bps** (20%) of the total
  proposal snapshot, else the proposal is rejected. Boundary: votes == quorum
  exactly → passes (≥). A tie `yes == no` — rejection (a strict
  majority `yes > no` is required); abstain counts toward the quorum but not the majority.
- **Voting period.** 120 960 blocks (7 days of 5 s). A vote is accepted
  in the half-interval `[created, created + period)` — at the closing block it is red
  (`VotingClosed`), at the last block of the window — green.
- **48-hour timelock.** Execution is possible no earlier than
  `created + period + 34 560` (48 h of 5 s). One block earlier — red
  (`Timelock`), at the boundary — green.
- **Veto/emergency with sunset.** An emergency veto moves Passed to Vetoed;
  the power **sunsets** at block `T6_VETO_SUNSET_BLOCK = 1 521 000`: one block
  before — it works, at the sunset and after — red (`VetoSunset`).
- **The flash-stake attack is rejected.** Voting power is fixed by the
  **snapshot** at proposal-creation time; stake taken after the
  snapshot (including flash loans) yields zero power — the vote is rejected
  (`NoVotingPower`), the snapshot does not move.

## Scenario `sim/t6-governance.json` (20 proposals, 113 steps)

Voters: 80 000 / 50 000 / 40 000 / 20 000 / 10 000 / 0 `ALCRY`
(total snapshot 200 000; quorum 40 000 = voter 2's power exactly —
the boundary case). 20 proposals: 15 executed (valid ones pass),
4 rejected (majority against / quorum missed / tie / no votes),
1 vetoed one block before the sunset; separately: refusing creation below the threshold,
a late vote at the boundary, early execution at the timelock boundary, a veto
after the sunset (`VetoSunset`), **a flash-stake of 1 000 000 `ALCRY` in the middle of
voting → `NoVotingPower`**, a double vote, a double finalize,
a double execution, late stake not moving the snapshot (a 20 000 vote, not
40 000).

The golden digest
`828b13cf7ca3abcaea65386f7c62e1e155f4d207cb000aceccd86397b5c729aa`
is pinned in `crates/aura-storage/tests/t6_governance.rs` and recomputed
by the independent Python oracle `tools/t6_governance_check.py`, which also
runs the `t6_governance_example` example.

## Negative controls (`tools/nc_t6.py`, 8/8)

| # | Mutation | Named message |
|---|---|---|
| 1 | the acceptance condition flipped | `T6: a valid proposal must pass` |
| 2 | the snapshot replaced by current stake | `T6: a flash-stake attack must be rejected` |
| 3 | the proposal threshold unchecked | `T6: a proposal below the threshold must be rejected` |
| 4 | the voting window +1 block | `T6: a vote outside the voting period must be rejected` |
| 5 | quorum = 0 | `T6: the quorum must be met for a proposal to pass` |
| 6 | timelock = 0 | `T6: execution must wait for the timelock` |
| 7 | sunset removed | `T6: the veto power must sunset` |
| 8 | `T6_VOTING_PERIOD_BLOCKS` drift | `T6: the reference simulation must agree with the pinned digest` |

Evidence: `docs/evidence/t6/` (mutation controls cumulative: **558** over
23 phases). Phase patch: `patches/0084-T6-governance.patch`.
