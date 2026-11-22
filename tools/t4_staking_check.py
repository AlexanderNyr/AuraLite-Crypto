#!/usr/bin/env python3
"""Independent T4 staking oracle (ECONOMICS_PLAN T4).

Re-implements the share-based staking model from scratch and re-runs the
pinned 1000-delegator scenario: the delegator shares must sum to exactly
one pool at every step, and a withdrawal without unbonding must be
rejected. Cross-checks the Rust example `t4_staking_example` and the
constants in `crates/aura-storage/src/staking.rs`.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIO = os.path.join(ROOT, "sim", "t4-staking.json")
STAKING_RS = os.path.join(ROOT, "crates", "aura-storage", "src", "staking.rs")

GOLDEN_STAKING_DIGEST = "e80662c82c74e1d1a0a5b0624beb4cf08af88c24f8091751c277cc7ea6887f07"
ALCRY = 10 ** 18
MIN_STAKE = 1000 * ALCRY
COMMISSION_BPS = 1000
BPS = 10_000
UNBONDING_BLOCKS = 21 * 86_400 // 5
BLOCK_REWARD = 2 * ALCRY


class Pool:
    def __init__(self):
        self.bonded = 0
        self.total_shares = 0
        self.pending_commission = 0
        self.delegations = {}
        self.unbonding = []

    def shares_sum(self):
        return sum(self.delegations.values())

    def delegate(self, owner, amount):
        if amount < MIN_STAKE:
            raise ValueError("BelowMinStake")
        minted = amount if self.total_shares == 0 else amount * self.total_shares // self.bonded
        self.delegations[owner] = self.delegations.get(owner, 0) + minted
        self.total_shares += minted
        self.bonded += amount
        return minted

    def undelegate(self, owner, shares, now):
        amount = shares * self.bonded // self.total_shares
        self.delegations[owner] -= shares
        self.total_shares -= shares
        self.bonded -= amount
        self.unbonding.append([owner, amount, now + UNBONDING_BLOCKS])
        return amount

    def withdraw(self, owner, now):
        claimed = 0
        kept = []
        for entry in self.unbonding:
            if entry[0] == owner and entry[2] <= now:
                claimed += entry[1]
            else:
                kept.append(entry)
        self.unbonding = kept
        if claimed == 0:
            raise ValueError("NoUnbonding/StillUnbonding")
        return claimed

    def accrue(self, reward):
        commission = reward * COMMISSION_BPS // BPS
        self.pending_commission += commission
        self.bonded += reward - commission

    def slash(self, rate_bps):
        cut = self.bonded * rate_bps // BPS
        self.bonded -= cut
        for entry in self.unbonding:
            entry[1] -= entry[1] * rate_bps // BPS


def rust_const(name):
    paths = [STAKING_RS, os.path.join(ROOT, "crates", "aura-storage", "src", "issuance.rs")]
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


def main():
    scenario = json.load(open(SCENARIO, encoding="utf-8"))
    assert scenario["unbonding_blocks"] == UNBONDING_BLOCKS
    assert scenario["min_stake_alcry"] * ALCRY == MIN_STAKE
    assert scenario["commission_bps"] == COMMISSION_BPS

    for name, value in [
        ("T4_MIN_STAKE", MIN_STAKE),
        ("T4_COMMISSION_BPS", COMMISSION_BPS),
        ("T4_BPS", BPS),
        ("T4_UNBONDING_DAYS", 21),
        ("T4_UNBONDING_BLOCKS", UNBONDING_BLOCKS),
        ("T4_BLOCK_REWARD", BLOCK_REWARD),
    ]:
        got = rust_const(name)
        assert got == value, (
            "T4: the reference simulation must agree with the pinned digest "
            "(const %s drift: %r != %r)" % (name, got, value)
        )

    steps = []
    pool = Pool()

    def record(kind, amount):
        steps.append((len(steps), kind, amount, pool.bonded,
                      pool.total_shares, pool.shares_sum()))

    for i in range(1000):
        amount = MIN_STAKE + i * MIN_STAKE // 1000
        record(1, pool.delegate(i, amount))
    for _ in range(200):
        pool.accrue(BLOCK_REWARD)
        record(2, BLOCK_REWARD)
    pool.slash(100)
    record(3, 100)
    for _ in range(200):
        pool.accrue(BLOCK_REWARD)
        record(2, BLOCK_REWARD)
    for i in range(100):
        record(4, pool.undelegate(i, pool.delegations[i] // 2, 10_000))
    for i in range(100, 200):
        record(4, pool.undelegate(i, pool.delegations[i] // 3, 20_000))
    pool.slash(500)
    record(3, 500)
    for _ in range(200):
        pool.accrue(BLOCK_REWARD)
        record(2, BLOCK_REWARD)
    try:
        early = pool.withdraw(0, 10_000 + UNBONDING_BLOCKS - 1)
    except ValueError:
        early = 0
    record(5, early)
    for i in range(100):
        try:
            claimed = pool.withdraw(i, 10_000 + UNBONDING_BLOCKS)
        except ValueError:
            claimed = 0
        record(6, claimed)
    for i in range(100, 200):
        try:
            claimed = pool.withdraw(i, 20_000 + UNBONDING_BLOCKS)
        except ValueError:
            claimed = 0
        record(6, claimed)
    record(9, 0)

    # The gate: shares sum == 1 at every step; early attempt rejected.
    for step in steps:
        assert step[4] == step[5], "T4: the delegator shares must sum to exactly one"
    assert steps[1802][1] == 5 and steps[1802][2] == 0, (
        "T4: a withdrawal without unbonding must be rejected"
    )

    text = "".join("%d:%d:%d:%d:%d:%d\n" % step for step in steps)
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    assert digest == GOLDEN_STAKING_DIGEST, (
        "T4: the reference simulation must agree with the pinned digest"
    )
    assert digest == scenario["golden_path_digest"]

    totals = scenario["totals"]
    withdrawn = sum(step[2] for step in steps if step[1] == 6)
    assert withdrawn == totals["withdrawn_total"], (
        "T4: the reference simulation must agree with the pinned digest"
    )
    assert pool.bonded == totals["final_bonded"], (
        "T4: the reference simulation must agree with the pinned digest"
    )
    assert pool.total_shares == totals["final_total_shares"]
    assert pool.shares_sum() == totals["final_shares_sum"], (
        "T4: the delegator shares must sum to exactly one"
    )
    assert pool.pending_commission == totals["pending_commission"], (
        "T4: the commission must be taken from rewards only"
    )

    for sample in scenario["samples"]:
        step = steps[sample["after_step"]]
        if "bonded" in sample:
            assert step[3] == sample["bonded"], (
                "T4: the reference simulation must agree with the pinned digest "
                "(sample %d)" % sample["after_step"]
            )
        if "amount" in sample:
            assert step[2] == sample["amount"], (
                "T4: a withdrawal without unbonding must be rejected (sample %d)"
                % sample["after_step"]
            )

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage", "--example",
         "t4_staking_example"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace",
    )
    output = proc.stdout or ""
    if proc.returncode != 0 or "T4-SIM-OK " not in output:
        print(output[-2000:])
        print("RESULT: T4 staking check FAIL — rust example did not run")
        return 1
    payload = json.loads(output[output.index("T4-SIM-OK ") + len("T4-SIM-OK "):].strip())
    assert payload["steps"] == len(steps)
    assert payload["digest"] == digest, (
        "T4: the reference simulation must agree with the pinned digest"
    )
    assert payload["final_bonded"] == pool.bonded
    assert payload["final_total_shares"] == pool.total_shares
    assert payload["final_shares_sum"] == pool.shares_sum(), (
        "T4: the delegator shares must sum to exactly one"
    )
    assert payload["withdrawn_total"] == withdrawn
    print("RESULT: T4 staking check PASS — 1000 delegators, shares sum to exactly one "
          "(final shares %d), unbonding %d blocks, withdrawn %d, commission %d, digest %s"
          % (pool.total_shares, UNBONDING_BLOCKS, withdrawn, pool.pending_commission, digest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
