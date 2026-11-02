# B4 — Validator set: staking contract, epoch, enter/exit queue, and QC-verified set update evidence receipt

Generated (UTC): 2026-10-31T13:06:52+00:00.

## Delivered behavior

`aura-consensus` completes the **B4** validator-set phase (`aura_consensus::validator_set`)
and updates `aura_consensus::baseline` to report 5 implemented consensus layers
(`proposer`, `block-signature`, `qc`, `finality`, `validator-set`) and 1 remaining
placeholder (`vrf-leader` B5):

- **Staking contract (`StakingContract`)**: tracks validator self-stake and per-delegator
  delegations, enforces `min_stake` (`DEFAULT_MIN_STAKE = 1_000_000`), and drives
  `ValidatorSet::request_enter` / `ValidatorSet::request_exit`.
- **Epoch boundaries & enter/exit queues (`EpochInfo`, `EnterRequest`, `ExitRequest`)**:
  processes queued entries and exits deterministically at epoch boundaries without
  halting block production or 3-chain finality.
- **QC-verified set updates (`SetUpdate`, `apply_set_update`, `apply_verified_set_update`)**:
  enforces epoch progression (`epoch == current + 1`), minimum stake across all updated
  validators (`SetUpdateError::InsufficientStake`), 2f+1 quorum certificate binding to
  `current_tip_hash` (`SetUpdateError::InvalidQc`), and BLS signature verification over
  the chain ID and `Domain::Vote`.
- **4 -> 7 validator transition at height H without stopping the network**:
  both unit (`validator_set::tests::four_to_seven_validator_change_at_height_h_without_stopping_network`)
  and integration (`crates/aura-consensus/tests/b4_validator_set.rs`) tests run 100
  continuous blocks across 7 nodes with an epoch transition from 4 to 7 active validators
  at height `H = 50`, dynamically shifting quorum from 3-of-4 to 5-of-7 and finalizing
  >= 95 blocks with identical finalized state across all 7 nodes.

## Gate results

- `make test-b4`: 59 `aura-consensus` library unit tests and 2 `b4_validator_set`
  integration tests passed.
- `make nc-b4`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact validator_set::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **249/249** cumulative negative controls through B4; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean N2-baseline (`2cf0321a83d6b308950a196b476ab086bf50c42c` + `0040-N0` +
  `0041-N1` + `0042-N2`) apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
