#!/usr/bin/env python3
"""Write the pinned B6 consensus-baseline and slashing fixtures.

Usage:
    python3 tools/gen_b6_vectors.py
    python3 tools/gen_b6_vectors.py --check
    make gen-b6-vectors
"""

from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b6_baseline.txt"
SLASHING_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b6_slashing.txt"

ATTACK_SPECS = [
    ("DoubleSignProposal", 1000, True),
    ("DoubleSignVote", 1000, True),
    ("SurroundVote", 1500, True),
    ("AmnesiaVote", 500, False),
    ("InvalidVrfLeaderClaim", 500, False),
    ("ExtendedDowntime", 100, False),
]


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


def build_slashing_vectors() -> str:
    lines = [
        "# AuraLite B6 Slashing & Evidence Deterministic Vectors (64 cases)",
        "# Format: vector_XX=attack_kind|stake_before|slash_bps|total_slashed|reporter_reward|treasury_share|burned_amount|stake_after|jailed|tombstoned|vector_digest",
    ]
    for idx in range(64):
        kind, bps, tombstoned = ATTACK_SPECS[idx % len(ATTACK_SPECS)]
        stake_before = 10_000_000 + idx * 250_000
        total_slashed = (stake_before * bps) // 10_000
        reporter_reward = (total_slashed * 1_000) // 10_000
        treasury_share = (total_slashed * 4_000) // 10_000
        burned_amount = total_slashed - reporter_reward - treasury_share
        stake_after = stake_before - total_slashed
        jailed = "true"
        tomb_str = "true" if tombstoned else "false"
        digest = hashlib.sha256(
            f"b6|{idx}|{kind}|{stake_before}|{bps}|{total_slashed}|{reporter_reward}|{treasury_share}|{burned_amount}".encode(
                "utf-8"
            )
        ).hexdigest()
        lines.append(
            f"vector_{idx:02d}={kind}|{stake_before}|{bps}|{total_slashed}|{reporter_reward}|{treasury_share}|{burned_amount}|{stake_after}|{jailed}|{tomb_str}|{digest}"
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    baseline_text = measure_baseline()
    slashing_text = build_slashing_vectors()

    if "--check" in sys.argv[1:]:
        for path, expected in ((BASELINE_FIXTURE, baseline_text), (SLASHING_FIXTURE, slashing_text)):
            if not path.exists():
                print(f"MISSING: {path.relative_to(REPO)}")
                return 1
            committed = path.read_text(encoding="utf-8")
            if committed != expected:
                print(f"DRIFT in {path.relative_to(REPO)} (run `make gen-b6-vectors`)")
                return 1
        print(
            f"B6 FIXTURES: OK ({BASELINE_FIXTURE.relative_to(REPO)}, {SLASHING_FIXTURE.relative_to(REPO)})"
        )
        return 0

    BASELINE_FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    BASELINE_FIXTURE.write_text(baseline_text, encoding="utf-8")
    SLASHING_FIXTURE.write_text(slashing_text, encoding="utf-8")
    print(f"wrote {BASELINE_FIXTURE.relative_to(REPO)} ({len(baseline_text.splitlines())} lines)")
    print(f"wrote {SLASHING_FIXTURE.relative_to(REPO)} ({len(slashing_text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
