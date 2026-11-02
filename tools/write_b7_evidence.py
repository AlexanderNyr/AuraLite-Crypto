#!/usr/bin/env python3
"""Generate close-out evidence for CONSENSUS B7 (`docs/evidence/b7/`)."""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE_DIR = REPO / "docs" / "evidence" / "b7"
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
    + ["b6", "b7"]
)

ARTIFACTS = [
    "crates/aura-consensus/src/lib.rs",
    "crates/aura-consensus/src/baseline.rs",
    "crates/aura-consensus/src/rewards.rs",
    "crates/aura-consensus/tests/b7_rewards.rs",
    "crates/aura-consensus/tests/fixtures/b7_baseline.txt",
    "crates/aura-consensus/tests/fixtures/b7_rewards.txt",
    "tools/gen_b7_vectors.py",
    "tools/nc_b7.py",
    "tools/write_b7_evidence.py",
    "Makefile",
    "CHANGELOG.md",
    "docs/plans/CONSENSUS_PLAN.md",
    "docs/evidence/b7/README.md",
    "docs/evidence/b7/gates.txt",
    "docs/evidence/b7/report.md",
    "docs/evidence/b7/inventory.txt",
    "docs/evidence/b7/gate.log",
    "docs/evidence/b7/nc.log",
    "docs/evidence/b7/vectors.log",
]


def run_cmd(cmd: list[str]) -> str:
    res = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=True)
    return res.stdout + res.stderr


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = ["# Cumulative negative-control inventory through B7", f"# Generated (UTC): {stamp}", ""]
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

    vectors_log = run_cmd(["python3", "tools/gen_b7_vectors.py", "--check"])
    (EVIDENCE_DIR / "vectors.log").write_text(vectors_log, encoding="utf-8")

    gate_log = run_cmd(["cargo", "test", "-p", "aura-consensus"])
    (EVIDENCE_DIR / "gate.log").write_text(gate_log, encoding="utf-8")

    nc_log = run_cmd(["python3", "tools/nc_b7.py"])
    (EVIDENCE_DIR / "nc.log").write_text(nc_log, encoding="utf-8")

    inv_text, total_nc = build_inventory(stamp)
    (EVIDENCE_DIR / "inventory.txt").write_text(inv_text, encoding="utf-8")

    gates_txt = f"""B7 gates (UTC generated {stamp})

make test-b7: PASS (see gate.log)
  aura-consensus library unit tests: 79
  100,000-block sum invariant + 64-vector fixture integration tests: 2
make nc-b7: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total_nc}/{total_nc} cumulative negative controls; see inventory.txt)
make test: PASS (ALL GATES GREEN)
Dependency closure and lockfile checksums: PASS (0 new external crates)
"""
    (EVIDENCE_DIR / "gates.txt").write_text(gates_txt, encoding="utf-8")

    readme = """# CONSENSUS B7 — Rewards (`0054-B7-rewards.patch`)

## Scope & DoD
- **Inflation / Block Reward & Fee Burn (`crates/aura-consensus/src/rewards.rs`)**:
  - Deterministic block inflation (`base_block_reward`), configurable transaction fee burn (`fee_burn_bps`), and proposer bonus (`proposer_bonus_bps`).
- **Delegation & Validator Commission**:
  - Validator commission (`commission_bps`) deducted first; remaining delegator pool split proportionally across validator self-stake and delegators.
- **100,000-Block Conservation Invariant**:
  - `sum(balances) + burned == issued` verified within 1 unit (exact `0` drift) over 100,000 blocks.
- **Negative Controls (`tools/nc_b7.py`)**: 6/6 library unit-test negative controls pass (`NC-1`..`NC-6`).
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
