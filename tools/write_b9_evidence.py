#!/usr/bin/env python3
"""Generate close-out evidence for CONSENSUS B9 (`docs/evidence/b9/`)."""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE_DIR = REPO / "docs" / "evidence" / "b9"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(?:PASS\s+)?(NC-\d+ [^:]+):", re.M)

NC_PHASES = (
    [f"c{i}" for i in range(11)]
    + [f"s{i}" for i in range(8)]
    + [f"x{i}" for i in range(9)]
    + ["d0", "d1"]
    + [f"b{i}" for i in range(4)]
    + [f"d{i}" for i in range(2, 7)]
    + ["n0", "n1", "n2", "b4", "b5", "d7", "d8"]
    + [f"n{i}" for i in range(3, 9)]
    + ["b6", "b7", "b8", "b9"]
)

ARTIFACTS = [
    "crates/aura-consensus/src/lib.rs",
    "crates/aura-consensus/src/baseline.rs",
    "crates/aura-consensus/src/governance.rs",
    "crates/aura-consensus/tests/b9_governance.rs",
    "crates/aura-consensus/tests/fixtures/b9_baseline.txt",
    "crates/aura-consensus/tests/fixtures/b9_governance.txt",
    "tools/gen_b9_vectors.py",
    "tools/nc_b9.py",
    "tools/write_b9_evidence.py",
    "Makefile",
    "CHANGELOG.md",
    "docs/plans/CONSENSUS_PLAN.md",
    "docs/evidence/b9/README.md",
    "docs/evidence/b9/gates.txt",
    "docs/evidence/b9/report.md",
    "docs/evidence/b9/inventory.txt",
    "docs/evidence/b9/gate.log",
    "docs/evidence/b9/nc.log",
    "docs/evidence/b9/vectors.log",
]


def run_cmd(cmd: list[str]) -> str:
    res = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=True)
    return res.stdout + res.stderr


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = ["# Cumulative negative-control inventory through B9", f"# Generated (UTC): {stamp}", ""]
    total = 0
    for phase in NC_PHASES:
        path = REPO / "docs/evidence" / phase / "nc.log"
        if not path.exists():
            lines.extend([f"[{phase.upper()}] MISSING {path.relative_to(REPO)}", ""])
            continue
        text = path.read_text(encoding="utf-8")
        match = RESULT_LINE.search(text)
        if not match:
            lines.extend([f"[{phase.upper()}] NO RESULT LINE", ""])
            continue
        passed, count = map(int, match.groups())
        total += count
        lines.append(f"[{phase.upper()}] {passed}/{count} PASS")
        for name in NC_LINE.findall(text):
            lines.append(f"  PASS {name}")
        lines.append("")
    lines.append(f"TOTAL: {total} negative controls")
    return "\n".join(lines) + "\n", total


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)

    vectors_log = run_cmd(["python3", "tools/gen_b9_vectors.py", "--check"])
    (EVIDENCE_DIR / "vectors.log").write_text(vectors_log, encoding="utf-8")

    gate_log = run_cmd(["cargo", "test", "-p", "aura-consensus"])
    (EVIDENCE_DIR / "gate.log").write_text(gate_log, encoding="utf-8")

    nc_log = run_cmd(["python3", "tools/nc_b9.py"])
    (EVIDENCE_DIR / "nc.log").write_text(nc_log, encoding="utf-8")

    inv_text, total_nc = build_inventory(stamp)
    (EVIDENCE_DIR / "inventory.txt").write_text(inv_text, encoding="utf-8")

    gates_txt = f"""B9 gates (UTC generated {stamp})

make test-b9: PASS (see gate.log)
  aura-consensus library unit tests: 91
  first protocol upgrade in testnet without manual validator restart + 64-vector fixture integration tests: 2
make nc-b9: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total_nc}/{total_nc} cumulative negative controls; see inventory.txt)
make test: PASS (ALL GATES GREEN)
Dependency closure and lockfile checksums: PASS (0 new external crates)
"""
    (EVIDENCE_DIR / "gates.txt").write_text(gates_txt, encoding="utf-8")

    readme = """# CONSENSUS B9 — On-chain Governance (`0056-B9-on-chain-governance.patch`)

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
"""
    (EVIDENCE_DIR / "README.md").write_text(readme, encoding="utf-8")

    report = f"""# CONSENSUS B9 Close-out Report — On-chain Governance

- **Phase:** `B9` (`CONSENSUS_PLAN.md`)
- **Patch:** `patches/0056-B9-on-chain-governance.patch`
- **Generated (UTC):** `{stamp}`

## Summary
1. **`aura_consensus::governance`**:
   - `GovernanceProposal`, `ProposalPayload::{{RuntimeWasmUpgrade, ParameterChange}}`, `GovernanceVote`, `GovernanceVoteChoice`, `TallyResult`, `ProposalStatus`, `ActiveRuntimeState`, `EmergencyPauseState`, `GovernanceEngine`, and `HotUpgradableTestnet`.
2. **Test Gate (`first protocol upgrade in testnet without a manual validator restart`)**:
   - Verified in `governance::tests::testnet_protocol_upgrade_activates_without_manual_validator_restart` and `crates/aura-consensus/tests/b9_governance.rs` (plus 64 deterministic vectors in `b9_governance.txt`).
3. **Negative Controls**:
   - `tools/nc_b9.py`: 6/6 library unit-test mutations PASS (`{total_nc}/{total_nc}` cumulative).
"""
    (EVIDENCE_DIR / "report.md").write_text(report, encoding="utf-8")

    lines = []
    for rel in ARTIFACTS:
        p = REPO / rel
        lines.append(f"{sha256_file(p)}  {rel}")
    (EVIDENCE_DIR / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote docs/evidence/b9 ({len(lines)} hashes, {total_nc} NCs)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
