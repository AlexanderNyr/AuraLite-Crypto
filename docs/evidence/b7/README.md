# CONSENSUS B7 — Rewards (`0054-B7-rewards.patch`)

## Scope & DoD
- **Inflation / Block Reward & Fee Burn (`crates/aura-consensus/src/rewards.rs`)**:
  - Deterministic block inflation (`base_block_reward`), configurable transaction fee burn (`fee_burn_bps`), and proposer bonus (`proposer_bonus_bps`).
- **Delegation & Validator Commission**:
  - Validator commission (`commission_bps`) deducted first; remaining delegator pool split proportionally across validator self-stake and delegators.
- **100,000-Block Conservation Invariant**:
  - `sum(balances) + burned == issued` verified within 1 unit (exact `0` drift) over 100,000 blocks.
- **Negative Controls (`tools/nc_b7.py`)**: 6/6 library unit-test negative controls pass (`NC-1`..`NC-6`).
