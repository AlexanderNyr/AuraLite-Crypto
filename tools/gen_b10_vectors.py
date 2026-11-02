#!/usr/bin/env python3
"""Generate or check B10 baseline + 64 deterministic BFT safety-model and long-range vectors."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b10_baseline.txt"
SAFETY_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b10_safety_model.txt"


def quorum_threshold(n: int) -> int:
    f = (n - 1) // 3
    return 2 * f + 1


def build_baseline_fixture() -> str:
    res = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "aura-consensus",
            "--",
            "report",
            "--workspace",
            str(REPO),
            "--fixture",
        ],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    if res.returncode != 0:
        sys.stderr.write(f"aura-consensus report --fixture failed: rc={res.returncode}\n{res.stderr}\n")
        raise SystemExit(1)
    return res.stdout


def build_safety_vectors() -> str:
    lines = [
        "# aura-consensus B10 deterministic safety-model and long-range analysis vectors (64 cases)",
        "# format: case_id=<id> n=<u64> f=<u64> enforce_quorum=<0|1> views=<u64> expected_safe=<0|1> min_honest_overlap=<u64> head_h=<u64> ckpt_h=<u64> window=<u64> fork_h=<u64> hist_stake_bps=<u16> enforce_ws=<0|1> expected_lr_verdict=<str>",
    ]
    for idx in range(64):
        n = 4 + (idx % 16) * 3
        max_safe_f = (n - 1) // 3
        mode = idx % 4
        if mode == 0:
            # Safe: f < n/3 and quorum check enabled
            f = max_safe_f
            enforce_quorum = 1
        elif mode == 1:
            # Safe with smaller f: f = max(0, max_safe_f - 1) and quorum check enabled
            f = max(0, max_safe_f - 1)
            enforce_quorum = 1
        elif mode == 2:
            # Unsafe: quorum check removed (NC gate)
            f = max_safe_f
            enforce_quorum = 0
        else:
            # Unsafe: f >= n/3 (Byzantine threshold exceeded)
            f = max_safe_f + 1
            enforce_quorum = 1

        views = 3
        q = quorum_threshold(n) if enforce_quorum == 1 else 1
        honest = max(0, n - f)

        counterexample_found = False
        for honest_a in range(honest + 1):
            honest_b = honest - honest_a
            if (f + honest_a) >= q and (f + honest_b) >= q:
                counterexample_found = True
                break

        if counterexample_found:
            expected_safe = 0
            min_honest_overlap = 0
        else:
            expected_safe = 1
            total_overlap = max(0, 2 * q - n)
            min_honest_overlap = max(0, total_overlap - f)

        # Long-range scenario fields
        head_h = 20_000 + idx * 100
        ckpt_h = head_h - 500
        window = 10_000
        lr_mode = (idx // 4) % 4
        if lr_mode == 0:
            # Fork older than weak-subjectivity window
            fork_h = head_h - 12_000
            hist_stake_bps = 8_000
            enforce_ws = 1
            expected_lr_verdict = "rejected_by_weak_subjectivity"
        elif lr_mode == 1:
            # Fork within window, but at/before finalized checkpoint
            fork_h = ckpt_h - 200
            hist_stake_bps = 7_500
            enforce_ws = 1
            expected_lr_verdict = "rejected_by_finalized_checkpoint"
        elif lr_mode == 2:
            # Insufficient historical stake (< 66.67%)
            fork_h = head_h - 15_000
            hist_stake_bps = 5_000
            enforce_ws = 1
            expected_lr_verdict = "insufficient_historical_stake"
        else:
            # Weak subjectivity disabled -> vulnerable
            fork_h = head_h - 15_000
            hist_stake_bps = 8_500
            enforce_ws = 0
            expected_lr_verdict = "vulnerable_without_weak_subjectivity"

        lines.append(
            f"case_id=b10_case_{idx:02d} n={n} f={f} enforce_quorum={enforce_quorum} "
            f"views={views} expected_safe={expected_safe} min_honest_overlap={min_honest_overlap} "
            f"head_h={head_h} ckpt_h={ckpt_h} window={window} fork_h={fork_h} "
            f"hist_stake_bps={hist_stake_bps} enforce_ws={enforce_ws} "
            f"expected_lr_verdict={expected_lr_verdict}"
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="verify fixtures match without writing")
    args = parser.parse_args()

    baseline_text = build_baseline_fixture()
    safety_text = build_safety_vectors()

    if args.check:
        if BASELINE_FIXTURE.read_text(encoding="utf-8") != baseline_text:
            sys.stderr.write("b10_baseline.txt is out of date\n")
            return 1
        if SAFETY_FIXTURE.read_text(encoding="utf-8") != safety_text:
            sys.stderr.write("b10_safety_model.txt is out of date\n")
            return 1
        print(
            f"OK: {BASELINE_FIXTURE.relative_to(REPO)} and {SAFETY_FIXTURE.relative_to(REPO)} match (64 safety-model vectors)"
        )
        return 0

    BASELINE_FIXTURE.write_text(baseline_text, encoding="utf-8")
    SAFETY_FIXTURE.write_text(safety_text, encoding="utf-8")
    print(f"wrote {BASELINE_FIXTURE.relative_to(REPO)} ({len(baseline_text.splitlines())} lines)")
    print(f"wrote {SAFETY_FIXTURE.relative_to(REPO)} ({len(safety_text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
