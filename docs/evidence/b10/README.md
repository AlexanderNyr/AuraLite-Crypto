# CONSENSUS B10 — Safety Evidence & Close-out (`0057-B10-safety-evidence.patch`)

## Scope & DoD
- **TLA+/Apalache Model & Bounded Model Checker (`HotStuff3ChainSafety.tla`, `crates/aura-consensus/src/safety_model.rs`)**:
  - Formalizes `QuorumThreshold == 2 * F + 1`, `HonestSingleVotePerView`, `QuorumIntersection`, `ThreeChainCommit`, and `AccountableSafetyInvariant`.
  - Proves safety (`ModelCheckResult::Safe`) for all `f < n / 3` (`3 * f < n`), and constructs an explicit counterexample (`ModelCheckResult::Counterexample`) when the quorum check is removed (`NC-1`) or `f >= n / 3`.
- **100-Node BFT Simulation (`run_hundred_node_simulation`)**:
  - Simulates `n = 100` validators (`f = 33` Byzantine equivocators, `67` honest validators, quorum `67`) across 50 heights with zero finalized-head divergence.
- **Long-Range & Weak-Subjectivity Analysis (`analyze_long_range_attack`)**:
  - Proves that posterior-corruption forks signed by exited historical validators are rejected by `D-B3` weak-subjectivity windows and finalized checkpoints.

## Files
- `HotStuff3ChainSafety.tla` — TLA+/Apalache specification of 3-chain HotStuff BFT safety.
- `gates.txt` — summary of B10 test and negative-control gates.
- `report.md` — detailed close-out report for CONSENSUS B10 and `CONSENSUS_PLAN` (`B0`–`B10`).
- `inventory.txt` — cumulative negative-control inventory (`333/333`).
- `gate.log` — output of `cargo test -p aura-consensus`.
- `nc.log` — output of `python3 tools/nc_b10.py` (6/6 PASS).
- `vectors.log` — output of `python3 tools/gen_b10_vectors.py --check`.
- `SHA256SUMS` — SHA-256 digests of all B10 deliverables and evidence files.
