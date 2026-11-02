# CONSENSUS B6 — Slashing & Evidence (`0053-B6-slashing-evidence.patch`)

## Scope & DoD (`D-B4`)
- **Six Slashable Attack Types (`crates/aura-consensus/src/slashing.rs`)**:
  1. `DoubleSignProposal` (10% slash + permanent tombstone/jail)
  2. `DoubleSignVote` (10% slash + permanent tombstone/jail)
  3. `SurroundVote` (15% slash + permanent tombstone/jail)
  4. `AmnesiaVote` (5% slash + jail)
  5. `InvalidVrfLeaderClaim` (5% slash + jail)
  6. `ExtendedDowntime` (1% slash + jail when `missed_blocks >= max_missed_blocks_per_window`)
- **Penalty Distribution (`PenaltyDistribution`)**:
  - 10% reporter reward (`1_000` bps), 40% protocol treasury share (`4_000` bps), 50% burned (`reporter + treasury + burned == total_slashed`).
- **Negative Controls (`tools/nc_b6.py`)**: 6/6 library unit-test negative controls pass (`NC-1`..`NC-6`), including `NC-1: slashing off -> test red`.
