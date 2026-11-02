#!/usr/bin/env python3
"""Generate or check B9 baseline + 64 deterministic on-chain governance vectors."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b9_baseline.txt"
GOV_FIXTURE = REPO / "crates" / "aura-consensus" / "tests" / "fixtures" / "b9_governance.txt"

BPS_DENOM = 10_000
QUORUM_BPS = 3_334
PASS_BPS = 6_667
VETO_BPS = 3_334
VOTING_PERIOD = 10
TIMELOCK_DELAY = 5
MIN_DEPOSIT = 100_000


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


def build_governance_vectors() -> str:
    lines = [
        "# aura-consensus B9 deterministic on-chain governance vectors (64 cases)",
        "# format: case_id=<id> total_stake=<u128> yes=<u128> no=<u128> abstain=<u128> veto=<u128> submit_h=<u64> tally_h=<u64> activation_h=<u64> paused=<0|1> expected_status=<str> earliest_exec_h=<u64> deposit_burned=<0|1>",
    ]
    total_stake = 40_000_000
    for idx in range(64):
        submit_h = 1 + idx * 3
        tally_h = submit_h + VOTING_PERIOD + 1
        activation_h = tally_h + TIMELOCK_DELAY + (idx % 4)
        paused = 1 if (idx % 16 == 15) else 0

        mode = idx % 4
        if mode == 0:
            # Passing supermajority (30M Yes, 10M Abstain/No)
            yes = 30_000_000
            no = 10_000_000 if (idx % 8 == 0) else 0
            abstain = 0 if (idx % 8 == 0) else 10_000_000
            veto = 0
        elif mode == 1:
            # Below quorum (10M Yes out of 40M = 25% < 33.34%)
            yes = 10_000_000
            no = 0
            abstain = 0
            veto = 0
        elif mode == 2:
            # Quorum met, but below pass threshold (20M Yes, 20M No = 50% < 66.67%)
            yes = 20_000_000
            no = 20_000_000
            abstain = 0
            veto = 0
        else:
            # Vetoed (20M Yes, 20M NoWithVeto = 50% >= 33.34%)
            yes = 20_000_000
            no = 0
            abstain = 0
            veto = 20_000_000

        participating = yes + no + abstain + veto
        non_abstain = yes + no + veto

        if participating > 0 and veto * BPS_DENOM >= participating * VETO_BPS:
            expected_status = "vetoed"
            earliest_exec_h = 0
            deposit_burned = 1
        elif participating * BPS_DENOM < total_stake * QUORUM_BPS:
            expected_status = "rejected_quorum"
            earliest_exec_h = 0
            deposit_burned = 0
        elif non_abstain == 0 or yes * BPS_DENOM < non_abstain * PASS_BPS:
            expected_status = "rejected_threshold"
            earliest_exec_h = 0
            deposit_burned = 0
        else:
            expected_status = "timelocked"
            earliest_exec_h = max(tally_h + TIMELOCK_DELAY, activation_h)
            deposit_burned = 0

        lines.append(
            f"case_id=b9_case_{idx:02d} total_stake={total_stake} yes={yes} no={no} "
            f"abstain={abstain} veto={veto} submit_h={submit_h} tally_h={tally_h} "
            f"activation_h={activation_h} paused={paused} expected_status={expected_status} "
            f"earliest_exec_h={earliest_exec_h} deposit_burned={deposit_burned}"
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="verify fixtures match without writing")
    args = parser.parse_args()

    baseline_text = build_baseline_fixture()
    gov_text = build_governance_vectors()

    if args.check:
        if BASELINE_FIXTURE.read_text(encoding="utf-8") != baseline_text:
            sys.stderr.write("b9_baseline.txt is out of date\n")
            return 1
        if GOV_FIXTURE.read_text(encoding="utf-8") != gov_text:
            sys.stderr.write("b9_governance.txt is out of date\n")
            return 1
        print(
            f"OK: {BASELINE_FIXTURE.relative_to(REPO)} and {GOV_FIXTURE.relative_to(REPO)} match (64 governance vectors)"
        )
        return 0

    BASELINE_FIXTURE.write_text(baseline_text, encoding="utf-8")
    GOV_FIXTURE.write_text(gov_text, encoding="utf-8")
    print(f"wrote {BASELINE_FIXTURE.relative_to(REPO)} ({len(baseline_text.splitlines())} lines)")
    print(f"wrote {GOV_FIXTURE.relative_to(REPO)} ({len(gov_text.splitlines())} lines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
