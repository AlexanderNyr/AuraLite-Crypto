#!/usr/bin/env python3
"""Independent oracle for T5 (ECONOMICS_PLAN T5).

Re-implements the slashing and insurance model in Python, derives the
constants from the Rust source, re-derives the pinned digest and
cross-runs the Rust example `t5_slashing_example`.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SLASHING_RS = os.path.join(ROOT, "crates", "aura-storage", "src", "slashing.rs")
SIM_JSON = os.path.join(ROOT, "sim", "t5-slashing.json")

GOLDEN_DIGEST = "4cc9ea0319374c3c2c74bc88db6d7747b23f0e21ea637ae2270bc4c5707c9cef"


def rust_const(name):
    paths = [SLASHING_RS, os.path.join(ROOT, "crates", "aura-storage", "src", "issuance.rs")]
    for source in paths:
        text = open(source, encoding="utf-8").read()
        match = re.search(r"pub const %s: (?:Amount|u64|u32) = ([0-9_]+);" % name, text)
        if match:
            return int(match.group(1).replace("_", ""))
        expr = re.search(r"pub const %s: (?:Amount|u64|u32) = ([A-Za-z0-9_ */]+);" % name, text)
        if expr:
            def value_of(factor):
                factor = factor.strip()
                if "/" in factor:
                    num, den = factor.split("/")
                    return value_of(num) // value_of(den)
                if re.fullmatch(r"[0-9_]+", factor):
                    return int(factor.replace("_", ""))
                if factor == "ALCRY":
                    return 10 ** 18
                return rust_const(factor)
            value = 1
            for factor in expr.group(1).split("*"):
                value *= value_of(factor)
            return value
    return None


class Fund:
    def __init__(self, bps, penalty_bps, share_bps, cap_bps, registered, seed):
        self.bps = bps
        self.penalty_bps = penalty_bps
        self.share_bps = share_bps
        self.cap = registered * cap_bps // bps
        self.balance = seed
        self.window = 0
        self.window_applied = 0
        self.pending = 0
        self.total_penalty = 0
        self.total_applied = 0
        self.total_covered = 0
        self.total_shortfall = 0
        self.records = []
        self.steps = []

    def step(self, kind, scenario):
        self.steps.append((len(self.steps), kind, scenario, self.balance, self.pending,
                           self.total_applied, self.total_penalty))

    def report(self, damage, scenario):
        penalty = damage * self.penalty_bps // self.bps
        assert penalty >= damage, "T5: the penalty must cover the damage"
        room = max(self.cap - self.window_applied, 0)
        applied = min(penalty, room)
        self.pending += penalty - applied
        self.window_applied += applied
        assert self.window_applied <= self.cap, "T5: the mass-slash cap must hold in every window"
        self.total_penalty += penalty
        self.total_applied += applied
        income = applied * self.share_bps // self.bps
        self.balance += income
        free = max(self.balance, 0)
        covered = min(damage, free)
        self.balance -= covered
        assert self.balance >= 0, "T5: the insurance fund must never go negative"
        self.total_covered += covered
        self.total_shortfall += damage - covered
        self.records.append((len(self.records), self.window, scenario, damage, penalty,
                             covered, applied, income, damage - covered))

    def advance(self, scenario):
        self.window += 1
        self.window_applied = 0
        taken = min(self.pending, self.cap)
        self.pending -= taken
        self.window_applied = taken
        self.total_applied += taken
        self.balance += taken * self.share_bps // self.bps
        self.step(1, scenario)


def main():
    BPS = rust_const("T5_BPS")
    PENALTY_BPS = rust_const("T5_PENALTY_BPS")
    SHARE_BPS = rust_const("T5_FUND_SHARE_BPS")
    CAP_BPS = rust_const("T5_MASS_SLASH_CAP_BPS")
    REGISTERED = rust_const("T5_REGISTERED_BONDED")
    SEED = rust_const("T5_FUND_SEED")
    ALCRY = 10 ** 18
    assert (BPS, PENALTY_BPS, SHARE_BPS, CAP_BPS, REGISTERED // ALCRY, SEED // ALCRY) == \
        (10_000, 15_000, 5_000, 500, 1_000_000, 200_000), \
        "T5: the reference simulation must agree with the pinned digest (constant drift)"

    f = Fund(BPS, PENALTY_BPS, SHARE_BPS, CAP_BPS, REGISTERED, SEED)
    f.report(100 * ALCRY, 1)
    f.step(0, 1)
    for _ in range(10):
        f.report(50 * ALCRY, 2)
        f.step(0, 2)
    for _ in range(20):
        f.report(5_000 * ALCRY, 3)
        f.step(0, 3)
    for _ in range(3):
        f.advance(3)
    for _ in range(3):
        f.report(60_000 * ALCRY, 4)
        f.step(0, 4)
    for _ in range(6):
        f.advance(4)
    f.report(ALCRY + 1, 5)
    f.step(0, 5)
    f.report(999_999_999_999_999_999, 5)
    f.step(0, 5)
    for _ in range(8):
        f.report(20_000 * ALCRY, 6)
        f.step(0, 6)
    for _ in range(6):
        f.advance(6)
    f.report(500_000 * ALCRY, 7)
    f.step(0, 7)
    f.report(250_000 * ALCRY, 7)
    f.step(0, 7)
    for _ in range(3):
        f.advance(7)
    f.report(1_000 * ALCRY, 7)
    f.step(0, 7)
    f.report(2_000 * ALCRY, 7)
    f.step(0, 7)
    for _ in range(20):
        f.advance(7)
    f.step(9, 7)

    lines = ["%d:%d:%d:%d:%d:%d:%d" % s for s in f.steps]
    digest = hashlib.sha256("".join(l + "\n" for l in lines).encode()).hexdigest()
    assert digest == GOLDEN_DIGEST, (
        "T5: the reference simulation must agree with the pinned digest "
        "(digest %s != %s)" % (digest, GOLDEN_DIGEST)
    )
    assert len(f.steps) == 87
    assert len(f.records) == 48
    assert f.pending == 0, "T5: deferred penalties must not be dropped"
    assert f.total_applied == f.total_penalty, "T5: deferred penalties must not be dropped"
    assert f.balance >= 0, "T5: the insurance fund must never go negative"

    scenario = json.load(open(SIM_JSON, encoding="utf-8"))
    assert scenario["golden_path_digest"] == GOLDEN_DIGEST
    totals = scenario["totals"]
    assert totals["final_balance"] == f.balance
    assert totals["pending"] == f.pending
    assert totals["total_penalty"] == f.total_penalty
    assert totals["total_applied"] == f.total_applied
    assert totals["total_covered"] == f.total_covered
    assert totals["total_shortfall"] == f.total_shortfall
    for sample in scenario["samples"]:
        step = f.steps[sample["after_step"]]
        assert step[3] == sample["balance"], (
            "T5: the reference simulation must agree with the pinned digest "
            "(sample %d)" % sample["after_step"]
        )
        assert step[4] == sample["pending"], (
            "T5: the reference simulation must agree with the pinned digest "
            "(sample %d)" % sample["after_step"]
        )

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage", "--example",
         "t5_slashing_example"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace",
    )
    output = proc.stdout or ""
    if proc.returncode != 0 or "T5-SIM-OK " not in output:
        print(output[-2000:])
        print("RESULT: T5 slashing check FAIL — rust example did not run")
        return 1
    payload = json.loads(output[output.index("T5-SIM-OK ") + len("T5-SIM-OK "):].strip())
    assert payload["steps"] == len(f.steps)
    assert payload["records"] == len(f.records)
    assert payload["digest"] == digest, (
        "T5: the reference simulation must agree with the pinned digest"
    )
    assert payload["final_balance"] == f.balance
    assert payload["pending"] == f.pending
    assert payload["total_penalty"] == f.total_penalty
    assert payload["total_applied"] == f.total_applied
    assert payload["total_covered"] == f.total_covered
    assert payload["total_shortfall"] == f.total_shortfall
    print("RESULT: T5 slashing check PASS — 7 scenarios, penalty >= damage, fund never "
          "negative (covered %d, shortfall %d, deferred drained to 0, digest %s)"
          % (f.total_covered, f.total_shortfall, digest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
