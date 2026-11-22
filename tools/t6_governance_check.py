#!/usr/bin/env python3
"""Independent oracle for T6 (ECONOMICS_PLAN T6).

Re-implements the governance model in Python, derives the constants
from the Rust source, re-derives the pinned digest and cross-runs the
Rust example `t6_governance_example`.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GOV_RS = os.path.join(ROOT, "crates", "aura-storage", "src", "governance.rs")
SIM_JSON = os.path.join(ROOT, "sim", "t6-governance.json")

GOLDEN_DIGEST = "828b13cf7ca3abcaea65386f7c62e1e155f4d207cb000aceccd86397b5c729aa"
ALCRY = 10 ** 18

BELOW, CLOSED, NOPOWER, DUPVOTE, STILLVOTING, DUPFINAL, TIMelock, NOTPASS, DUPEXEC, SUNSETErr = range(1, 11)
VOTING, PASSED, REJECTED, EXECUTED, VETOED, NA = 0, 1, 2, 3, 4, 9


def rust_const(name):
    paths = [GOV_RS, os.path.join(ROOT, "crates", "aura-storage", "src", "issuance.rs")]
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


class Proposal:
    def __init__(self, pid, proposer, created, snapshot):
        self.id, self.proposer, self.created = pid, proposer, created
        self.snapshot = list(snapshot)
        self.yes = self.no = self.abstain = 0
        self.voted = [False] * len(snapshot)
        self.state = VOTING


class Gov:
    def __init__(self, stakes, bps, threshold, quorum_bps, period, timelock, sunset):
        self.stakes = list(stakes)
        self.bps, self.threshold = bps, threshold
        self.quorum_bps, self.period, self.timelock, self.sunset = quorum_bps, period, timelock, sunset
        self.proposals = []
        self.steps = []
        self.next_id = 1

    def snap(self, kind, subject, height, a, b, c, state, detail=0):
        self.steps.append((len(self.steps), kind, subject, height, a, b, c, state, detail))

    def find(self, pid):
        return self.proposals[pid - 1]

    def propose(self, proposer, height):
        if self.stakes[proposer] < self.threshold:
            return BELOW
        p = Proposal(self.next_id, proposer, height, self.stakes)
        self.next_id += 1
        self.proposals.append(p)
        return p

    def vote(self, pid, voter, support, height):
        p = self.find(pid)
        if height >= p.created + self.period:
            return CLOSED
        power = p.snapshot[voter]
        if power == 0:
            return NOPOWER
        if p.voted[voter]:
            return DUPVOTE
        p.voted[voter] = True
        if support == "yes":
            p.yes += power
        elif support == "no":
            p.no += power
        else:
            p.abstain += power
        return 0

    def finalize(self, pid, height):
        p = self.find(pid)
        if height < p.created + self.period:
            return STILLVOTING
        if p.state != VOTING:
            return DUPFINAL
        voted = p.yes + p.no + p.abstain
        quorum = sum(p.snapshot) * self.quorum_bps // self.bps
        p.state = PASSED if (voted >= quorum and p.yes > p.no) else REJECTED
        return 0

    def execute(self, pid, height):
        p = self.find(pid)
        if p.state == EXECUTED:
            return DUPEXEC
        if p.state != PASSED:
            return NOTPASS
        if height < p.created + self.period + self.timelock:
            return TIMelock
        p.state = EXECUTED
        return 0

    def veto(self, pid, height):
        p = self.find(pid)
        if p.state != PASSED:
            return NOTPASS
        if height >= self.sunset:
            return SUNSETErr
        p.state = VETOED
        return 0

    def do_propose(self, proposer, height):
        r = self.propose(proposer, height)
        if isinstance(r, Proposal):
            p = self.find(r.id)
            self.snap(0, r.id, height, p.yes, p.no, p.abstain, p.state)
        else:
            self.snap(8, 0, height, 0, 0, 0, NA, r)
        return r

    def do_vote(self, pid, voter, support, height):
        r = self.vote(pid, voter, support, height)
        p = self.find(pid)
        self.snap(1 if r == 0 else 8, pid, height, p.yes, p.no, p.abstain, p.state, r)
        return r

    def do_finalize(self, pid, height):
        r = self.finalize(pid, height)
        p = self.find(pid)
        self.snap(2 if r == 0 else 8, pid, height, p.yes, p.no, p.abstain, p.state, r)
        return r

    def do_execute(self, pid, height):
        r = self.execute(pid, height)
        p = self.find(pid)
        self.snap(3 if r == 0 else 8, pid, height, p.yes, p.no, p.abstain, p.state, r)
        return r

    def do_veto(self, pid, height):
        r = self.veto(pid, height)
        p = self.find(pid)
        self.snap(4 if r == 0 else 8, pid, height, p.yes, p.no, p.abstain, p.state, r)
        return r

    def do_stake(self, voter, amount, height):
        self.stakes[voter] += amount
        self.snap(5, voter, height, self.stakes[voter], 0, 0, NA)

    def do_unstake(self, voter, amount, height):
        self.stakes[voter] -= amount
        self.snap(6, voter, height, self.stakes[voter], 0, 0, NA)


def build_scenario(g):
    g.do_propose(0, 200_000)
    g.do_vote(1, 0, "yes", 210_000); g.do_vote(1, 1, "yes", 211_000); g.do_vote(1, 2, "yes", 212_000)
    g.do_finalize(1, 321_000); g.do_execute(1, 355_520)
    g.do_propose(0, 400_000)
    g.do_vote(2, 2, "yes", 410_000); g.do_vote(2, 0, "no", 411_000)
    g.do_finalize(2, 521_000); g.do_execute(2, 522_000)
    g.do_propose(0, 600_000)
    g.do_vote(3, 3, "yes", 610_000)
    g.do_finalize(3, 721_000)
    assert g.do_propose(5, 800_000) == BELOW
    g.do_propose(0, 800_001)
    g.do_vote(4, 0, "yes", 810_000); g.do_vote(4, 1, "yes", 811_000)
    g.do_finalize(4, 921_000); g.do_execute(4, 955_560)
    g.do_propose(0, 1_000_000)
    g.do_vote(5, 0, "yes", 1_010_000); g.do_vote(5, 1, "yes", 1_020_000)
    assert g.do_vote(5, 2, "yes", 1_120_960) == CLOSED
    g.do_finalize(5, 1_120_961); g.do_execute(5, 1_155_520)
    g.do_propose(0, 1_200_000)
    g.do_vote(6, 0, "yes", 1_210_000); g.do_vote(6, 1, "yes", 1_220_000)
    g.do_finalize(6, 1_320_961)
    assert g.do_execute(6, 1_355_519) == TIMelock
    g.do_execute(6, 1_355_520)
    g.do_propose(0, 1_400_000)
    g.do_vote(7, 0, "yes", 1_410_000); g.do_vote(7, 1, "yes", 1_420_000)
    g.do_finalize(7, 1_520_961)
    g.do_veto(7, 1_520_999)
    g.do_execute(7, 1_555_521)
    g.do_propose(0, 1_600_000)
    g.do_vote(8, 0, "yes", 1_610_000); g.do_vote(8, 1, "yes", 1_620_000)
    g.do_finalize(8, 1_720_961)
    assert g.do_veto(8, 1_721_000) == SUNSETErr
    g.do_execute(8, 1_755_520)
    g.do_propose(0, 1_800_000)
    g.do_stake(5, 1_000_000 * ALCRY, 1_850_000)
    assert g.do_vote(9, 5, "yes", 1_850_001) == NOPOWER
    g.do_vote(9, 0, "yes", 1_860_000); g.do_vote(9, 1, "yes", 1_861_000); g.do_vote(9, 2, "yes", 1_862_000)
    g.do_finalize(9, 1_920_961); g.do_execute(9, 1_955_520)
    g.do_unstake(5, 1_000_000 * ALCRY, 1_960_000)
    g.do_propose(0, 2_000_000)
    g.do_vote(10, 0, "yes", 2_050_000)
    assert g.do_vote(10, 0, "no", 2_050_001) == DUPVOTE
    g.do_vote(10, 1, "yes", 2_060_000)
    g.do_finalize(10, 2_120_961); g.do_execute(10, 2_155_520)
    g.do_propose(0, 2_200_000)
    g.do_vote(11, 0, "yes", 2_210_000); g.do_vote(11, 1, "abstain", 2_211_000); g.do_vote(11, 2, "abstain", 2_212_000)
    g.do_finalize(11, 2_320_961); g.do_execute(11, 2_355_520)
    g.do_propose(0, 2_400_000)
    g.do_vote(12, 2, "yes", 2_410_000)
    g.do_finalize(12, 2_520_961); g.do_execute(12, 2_555_520)
    g.do_propose(0, 2_600_000)
    g.do_vote(13, 0, "yes", 2_610_000); g.do_vote(13, 1, "yes", 2_611_000)
    g.do_finalize(13, 2_720_961); g.do_execute(13, 2_755_520)
    g.do_propose(0, 2_800_000)
    g.do_stake(3, 20_000 * ALCRY, 2_850_000)
    g.do_vote(14, 3, "yes", 2_850_001)
    g.do_vote(14, 0, "yes", 2_860_000)
    g.do_finalize(14, 2_920_961); g.do_execute(14, 2_955_520)
    g.do_propose(0, 3_000_000)
    g.do_vote(15, 0, "yes", 3_010_000); g.do_vote(15, 4, "yes", 3_011_000)
    g.do_vote(15, 1, "no", 3_012_000); g.do_vote(15, 2, "no", 3_013_000)
    g.do_finalize(15, 3_120_961)
    g.do_propose(4, 3_200_000)
    g.do_vote(16, 0, "yes", 3_210_000)
    g.do_finalize(16, 3_320_961); g.do_execute(16, 3_355_520)
    g.do_propose(0, 3_400_000)
    g.do_finalize(17, 3_520_961)
    g.do_propose(0, 3_600_000)
    g.do_vote(18, 0, "yes", 3_610_000); g.do_vote(18, 1, "yes", 3_611_000)
    g.do_finalize(18, 3_720_961)
    assert g.do_finalize(18, 3_720_962) == DUPFINAL
    g.do_execute(18, 3_755_520)
    g.do_propose(0, 3_800_000)
    g.do_vote(19, 0, "yes", 3_810_000); g.do_vote(19, 1, "yes", 3_811_000)
    g.do_finalize(19, 3_920_961); g.do_execute(19, 3_955_520)
    assert g.do_execute(19, 3_955_521) == DUPEXEC
    g.do_propose(1, 4_000_000)
    g.do_vote(20, 0, "yes", 4_010_000); g.do_vote(20, 1, "yes", 4_011_000); g.do_vote(20, 2, "yes", 4_012_000)
    g.do_vote(20, 3, "no", 4_013_000); g.do_vote(20, 4, "abstain", 4_014_000)
    g.do_finalize(20, 4_120_961); g.do_execute(20, 4_155_520)
    states = [p.state for p in g.proposals]
    g.snap(9, 0, 0, states.count(EXECUTED), states.count(REJECTED), states.count(VETOED), NA)


def main():
    ALCRY = 10 ** 18
    BPS = rust_const("T6_BPS")
    THRESHOLD = rust_const("T6_PROPOSAL_THRESHOLD")
    QUORUM_BPS = rust_const("T6_QUORUM_BPS")
    PERIOD = rust_const("T6_VOTING_PERIOD_BLOCKS")
    TIMELOCK = rust_const("T6_TIMELOCK_BLOCKS")
    SUNSET = rust_const("T6_VETO_SUNSET_BLOCK")
    assert (BPS, THRESHOLD // ALCRY, QUORUM_BPS, PERIOD, TIMELOCK, SUNSET) == \
        (10_000, 10_000, 2_000, 120_960, 34_560, 1_521_000), \
        "T6: the reference simulation must agree with the pinned digest (constant drift)"

    stakes = [80_000 * ALCRY, 50_000 * ALCRY, 40_000 * ALCRY,
              20_000 * ALCRY, 10_000 * ALCRY, 0]
    g = Gov(stakes, BPS, THRESHOLD, QUORUM_BPS, PERIOD, TIMELOCK, SUNSET)
    build_scenario(g)

    lines = ["%d:%d:%d:%d:%d:%d:%d:%d:%d" % s for s in g.steps]
    digest = hashlib.sha256("".join(l + "\n" for l in lines).encode()).hexdigest()
    assert digest == GOLDEN_DIGEST, (
        "T6: the reference simulation must agree with the pinned digest "
        "(digest %s != %s)" % (digest, GOLDEN_DIGEST)
    )
    assert len(g.steps) == 113
    assert len(g.proposals) == 20
    states = [p.state for p in g.proposals]
    assert states == [3, 2, 2, 3, 3, 3, 4, 3, 3, 3, 3, 3, 3, 3, 2, 3, 2, 3, 3, 3], \
        "T6: a valid proposal must pass"
    executed = states.count(EXECUTED)
    rejected = states.count(REJECTED)
    vetoed = states.count(VETOED)
    assert (executed, rejected, vetoed) == (15, 4, 1), "T6: a valid proposal must pass"

    scenario = json.load(open(SIM_JSON, encoding="utf-8"))
    assert scenario["golden_path_digest"] == GOLDEN_DIGEST
    totals = scenario["totals"]
    assert totals["steps"] == len(g.steps)
    assert totals["proposals"] == len(g.proposals)
    assert totals["executed"] == executed
    assert totals["rejected"] == rejected
    assert totals["vetoed"] == vetoed
    for sample in scenario["samples"]:
        step = g.steps[sample["after_step"]]
        for key, pos in (("a", 4), ("b", 5), ("c", 6), ("state", 7), ("detail", 8)):
            if key in sample:
                assert step[pos] == sample[key], (
                    "T6: the reference simulation must agree with the pinned digest "
                    "(sample %d key %s)" % (sample["after_step"], key)
                )

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage", "--example",
         "t6_governance_example"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace",
    )
    output = proc.stdout or ""
    if proc.returncode != 0 or "T6-SIM-OK " not in output:
        print(output[-2000:])
        print("RESULT: T6 governance check FAIL — rust example did not run")
        return 1
    payload = json.loads(output[output.index("T6-SIM-OK ") + len("T6-SIM-OK "):].strip())
    assert payload["steps"] == len(g.steps)
    assert payload["proposals"] == len(g.proposals)
    assert payload["digest"] == digest, (
        "T6: the reference simulation must agree with the pinned digest"
    )
    assert payload["executed"] == executed
    assert payload["rejected"] == rejected
    assert payload["vetoed"] == vetoed
    print("RESULT: T6 governance check PASS — 20 proposals, valid ones pass (15 executed), "
          "flash-stake attack rejected, veto sunset at %d, digest %s" % (SUNSET, digest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
