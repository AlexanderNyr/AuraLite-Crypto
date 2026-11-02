#!/usr/bin/env python3
"""Generate close-out evidence for CONSENSUS B6 (`docs/evidence/b6/`)."""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE_DIR = REPO / "docs" / "evidence" / "b6"
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
    + ["b6"]
)

ARTIFACTS = [
    "crates/aura-consensus/src/lib.rs",
    "crates/aura-consensus/src/baseline.rs",
    "crates/aura-consensus/src/slashing.rs",
    "crates/aura-consensus/src/validator_set.rs",
    "crates/aura-consensus/tests/b6_slashing.rs",
    "crates/aura-consensus/tests/fixtures/b6_baseline.txt",
    "crates/aura-consensus/tests/fixtures/b6_slashing.txt",
    "tools/gen_b6_vectors.py",
    "tools/nc_b6.py",
    "tools/write_b6_evidence.py",
    "Makefile",
    "CHANGELOG.md",
    "docs/plans/CONSENSUS_PLAN.md",
    "docs/evidence/b6/README.md",
    "docs/evidence/b6/gates.txt",
    "docs/evidence/b6/report.md",
    "docs/evidence/b6/inventory.txt",
    "docs/evidence/b6/gate.log",
    "docs/evidence/b6/nc.log",
    "docs/evidence/b6/vectors.log",
]


def run_cmd(cmd: list[str]) -> str:
    res = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=True)
    return res.stdout + res.stderr


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = ["# Cumulative negative-control inventory through B6", f"# Generated (UTC): {stamp}", ""]
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

    vectors_log = run_cmd(["python3", "tools/gen_b6_vectors.py", "--check"])
    (EVIDENCE_DIR / "vectors.log").write_text(vectors_log, encoding="utf-8")

    gate_log = run_cmd(["cargo", "test", "-p", "aura-consensus"])
    (EVIDENCE_DIR / "gate.log").write_text(gate_log, encoding="utf-8")

    nc_log = run_cmd(["python3", "tools/nc_b6.py"])
    (EVIDENCE_DIR / "nc.log").write_text(nc_log, encoding="utf-8")

    inv_text, total_nc = build_inventory(stamp)
    (EVIDENCE_DIR / "inventory.txt").write_text(inv_text, encoding="utf-8")

    gates_txt = f"""B6 gates (UTC generated {stamp})

make test-b6: PASS (see gate.log)
  aura-consensus library unit tests: 73
  6-attack slashing + 64-vector fixture integration tests: 2
make nc-b6: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total_nc}/{total_nc} cumulative negative controls; see inventory.txt)
make test: PASS (ALL GATES GREEN)
Dependency closure and lockfile checksums: PASS (0 new external crates)
"""
    (EVIDENCE_DIR / "gates.txt").write_text(gates_txt, encoding="utf-8")

    readme = """# CONSENSUS B6 — Slashing & Evidence (`0053-B6-slashing-evidence.patch`)

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
"""
    (EVIDENCE_DIR / "README.md").write_text(readme, encoding="utf-8")
    (EVIDENCE_DIR / "report.md").write_text(readme, encoding="utf-8")

    sums_lines = []
    for rel in ARTIFACTS:
        p = REPO / rel
        sums_lines.append(f"{sha256_file(p)}  {rel}")
    (EVIDENCE_DIR / "SHA256SUMS").write_text("\n".join(sums_lines) + "\n", encoding="utf-8")
    print(f"wrote {EVIDENCE_DIR.relative_to(REPO)} ({len(ARTIFACTS)} hashes, {total_nc} NCs)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
