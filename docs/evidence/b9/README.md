# CONSENSUS B9 — On-chain Governance (`0056-B9-on-chain-governance.patch`)

## Scope & DoD
- **Proposal, Stake-Weighted BLS Voting, Tally & Timelock Queue (`crates/aura-consensus/src/governance.rs`)**:
  - Enforces proposer deposit (`DEFAULT_MIN_PROPOSER_DEPOSIT = 100_000`), WASM bytecode validation (`WASM_MAGIC_AND_VERSION`, `BLAKE3` code hash, strictly increasing `to_version`), domain-separated BLS validator votes (`Domain::Vote`), quorum/supermajority/veto thresholds, and a mandatory timelock queue (`ProposalStatus::Timelocked`).
- **Hot WASM Runtime Upgrade without Validator Restart (`HotUpgradableTestnet`)**:
  - Executes the first protocol upgrade (`v1 -> v2`) in a 4-validator testnet at the scheduled activation height without any manual validator restart (`restart_count == 0`).
- **Emergency Pause (`EmergencyPauseState`)**:
  - `2f+1` validator BLS `QuorumCertificate` triggers or lifts an emergency pause that halts governance execution until explicitly resumed.

## Files
- `gates.txt` — summary of B9 test and negative-control gates.
- `report.md` — detailed close-out report for CONSENSUS B9.
- `inventory.txt` — cumulative negative-control inventory (`327/327`).
- `gate.log` — output of `cargo test -p aura-consensus`.
- `nc.log` — output of `python3 tools/nc_b9.py` (6/6 PASS).
- `vectors.log` — output of `python3 tools/gen_b9_vectors.py --check`.
- `SHA256SUMS` — SHA-256 digests of all B9 deliverables and evidence files.
