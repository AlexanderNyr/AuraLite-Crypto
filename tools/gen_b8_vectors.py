#!/usr/bin/env python3
"""Write the pinned B8 consensus-baseline and fork-choice/finality fixtures.

Usage:
    python3 tools/gen_b8_vectors.py
    python3 tools/gen_b8_vectors.py --check
    make gen-b8-vectors
"""

from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b8_baseline.txt"
FORK_CHOICE_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b8_fork_choice.txt"


def measure_baseline() -> str:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "aura-consensus",
            "--bin",
            "aura-consensus",
            "--",
            "report",
            "--fixture",
            "--workspace",
            str(REPO),
        ],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(f"aura-consensus report --fixture failed: rc={result.returncode}\n")
        sys.stderr.write(result.stderr)
        sys.exit(1)
    return result.stdout


def build_fork_choice_vectors() -> str:
    lines = [
        "# AuraLite B8 Fork Choice, Finalized Checkpoint, Light Client & Deep Reorg Defence Vectors (64 cases)",
        "# Format: vector_XX=finalized_height|candidate_height|descends_from_finalized|candidate_qc_view|competing_qc_view|outcome|digest",
    ]
    for idx in range(64):
        finalized_height = 10 + (idx % 8) * 5
        if idx % 4 == 0:
            # Deep reorg at or below finalized height -> REJECT_WITH_EVIDENCE
            candidate_height = finalized_height - (idx % 3)
            descends = "false"
            cand_qc = 20 + idx
            comp_qc = 15 + idx
            outcome = "REJECT_DEEP_REORG_WITH_EVIDENCE"
        elif idx % 4 == 1:
            # Fork above finalized height that does NOT descend from finalized checkpoint -> REJECT_DEEP_REORG_WITH_EVIDENCE
            candidate_height = finalized_height + 2
            descends = "false"
            cand_qc = 30 + idx
            comp_qc = 20 + idx
            outcome = "REJECT_DEEP_REORG_WITH_EVIDENCE"
        elif idx % 4 == 2:
            # Valid branch descending from finalized with higher QC view -> SELECT_CANDIDATE
            candidate_height = finalized_height + 3
            descends = "true"
            cand_qc = 40 + idx
            comp_qc = 25 + idx
            outcome = "SELECT_CANDIDATE"
        else:
            # Valid branch descending from finalized with lower QC view -> KEEP_COMPETING
            candidate_height = finalized_height + 3
            descends = "true"
            cand_qc = 18 + idx
            comp_qc = 35 + idx
            outcome = "KEEP_COMPETING"

        digest = hashlib.sha256(
            f"b8|{idx}|{finalized_height}|{candidate_height}|{descends}|{cand_qc}|{comp_qc}|{outcome}".encode(
                "utf-8"
            )
        ).hexdigest()
        lines.append(
            f"vector_{idx:02d}={finalized_height}|{candidate_height}|{descends}|{cand_qc}|{comp_qc}|{outcome}|{digest}"
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    baseline_text = measure_baseline()
    fc_text = build_fork_choice_vectors()

    if "--check" in sys.argv[1:]:
        for path, expected in ((BASELINE_FIXTURE, baseline_text), (FORK_CHOICE_FIXTURE, fc_text)):
            if not path.exists():
                print(f"MISSING: {path.relative_to(REPO)}")
                return 1
            committed = path.read_text(encoding="utf-8")
            if committed != expected:
                print(f"DRIFT in {path.relative_to(REPO)} (run `make gen-b8-vectors`)")
                return 1
        print(
            f"B8 FIXTURES: OK ({BASELINE_FIXTURE.relative_to(REPO)}, {FORK_CHOICE_FIXTURE.relative_to(REPO)})"
        )
        return 0

    BASELINE_FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    BASELINE_FIXTURE.write_text(baseline_text, encoding="utf-8")
    FORK_CHOICE_FIXTURE.write_text(fc_text, encoding="utf-8")
    print(f"wrote {BASELINE_FIXTURE.relative_to(REPO)} ({len(baseline_text.splitlines())} lines)")
    print(f"wrote {FORK_CHOICE_FIXTURE.relative_to(REPO)} ({len(fc_text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
