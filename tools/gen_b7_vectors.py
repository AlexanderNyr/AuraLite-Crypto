#!/usr/bin/env python3
"""Write the pinned B7 consensus-baseline and rewards fixtures.

Usage:
    python3 tools/gen_b7_vectors.py
    python3 tools/gen_b7_vectors.py --check
    make gen-b7-vectors
"""

from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b7_baseline.txt"
REWARDS_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b7_rewards.txt"


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


def build_rewards_vectors() -> str:
    lines = [
        "# AuraLite B7 Rewards, Fee Burn, Delegation & Commission Vectors (64 cases)",
        "# Format: vector_XX=base_reward|tx_fees|fee_burn_bps|commission_bps|self_stake|delegated_stake|burned_fees|distributable|validator_gain|delegator_gain|digest",
    ]
    for idx in range(64):
        base_reward = 50_000 + (idx % 5) * 10_000
        tx_fees = 2_000 + idx * 350
        fee_burn_bps = 5_000
        commission_bps = (idx % 6) * 500
        self_stake = 10_000_000 + (idx % 4) * 5_000_000
        delegated_stake = 10_000_000 + (idx % 8) * 5_000_000

        burned_fees = (tx_fees * fee_burn_bps) // 10_000
        distributable = base_reward + (tx_fees - burned_fees)
        proposer_bonus = (distributable * 1_000) // 10_000
        committee_pool = distributable - proposer_bonus

        total_stake = self_stake + delegated_stake
        # Single-validator exact breakdown:
        # 1) proposer_bonus
        c1 = (proposer_bonus * commission_bps) // 10_000
        pool1 = proposer_bonus - c1
        s1 = (pool1 * self_stake) // total_stake
        d1 = (pool1 * delegated_stake) // total_stake
        dust1 = proposer_bonus - (c1 + s1 + d1)

        # 2) committee_pool
        c2 = (committee_pool * commission_bps) // 10_000
        pool2 = committee_pool - c2
        s2 = (pool2 * self_stake) // total_stake
        d2 = (pool2 * delegated_stake) // total_stake
        dust2 = committee_pool - (c2 + s2 + d2)

        validator_gain = (c1 + s1 + dust1) + (c2 + s2 + dust2)
        delegator_gain = d1 + d2
        assert validator_gain + delegator_gain + burned_fees == base_reward + tx_fees

        digest = hashlib.sha256(
            f"b7|{idx}|{base_reward}|{tx_fees}|{burned_fees}|{validator_gain}|{delegator_gain}".encode(
                "utf-8"
            )
        ).hexdigest()
        lines.append(
            f"vector_{idx:02d}={base_reward}|{tx_fees}|{fee_burn_bps}|{commission_bps}|{self_stake}|{delegated_stake}|{burned_fees}|{distributable}|{validator_gain}|{delegator_gain}|{digest}"
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    baseline_text = measure_baseline()
    rewards_text = build_rewards_vectors()

    if "--check" in sys.argv[1:]:
        for path, expected in ((BASELINE_FIXTURE, baseline_text), (REWARDS_FIXTURE, rewards_text)):
            if not path.exists():
                print(f"MISSING: {path.relative_to(REPO)}")
                return 1
            committed = path.read_text(encoding="utf-8")
            if committed != expected:
                print(f"DRIFT in {path.relative_to(REPO)} (run `make gen-b7-vectors`)")
                return 1
        print(
            f"B7 FIXTURES: OK ({BASELINE_FIXTURE.relative_to(REPO)}, {REWARDS_FIXTURE.relative_to(REPO)})"
        )
        return 0

    BASELINE_FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    BASELINE_FIXTURE.write_text(baseline_text, encoding="utf-8")
    REWARDS_FIXTURE.write_text(rewards_text, encoding="utf-8")
    print(f"wrote {BASELINE_FIXTURE.relative_to(REPO)} ({len(baseline_text.splitlines())} lines)")
    print(f"wrote {REWARDS_FIXTURE.relative_to(REPO)} ({len(rewards_text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
