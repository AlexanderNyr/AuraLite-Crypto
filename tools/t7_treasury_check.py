#!/usr/bin/env python3
"""T7 treasury and grants oracle.

A self-contained step-log model of the pinned T7 scenario (10 grant
proposals, every payout with proposal + tx + report, a withdrawal
without a vote rejected). Derives the canonical digest (must match
the Rust `run_t7_scenario` step log, `tests/t7_treasury.rs` and
`sim/t7-treasury.json`) and checks the governance named messages.
"""
import hashlib
import json
import sys

MESSAGES = {
    1: "T7: a withdrawal without a vote must be rejected",
    2: "T7: every payout must have a proposal, a tx and a report",
    3: "T7: the multisig threshold must be enforced",
    4: "T7: the treasury must not overdraw",
    5: "T7: a payout must not execute twice",
    6: "T7: a rejected proposal must not pay out",
    7: "T7: the reference simulation must agree with the pinned digest",
}

ALCRY = 10 ** 18
COUNCIL = 5
SIGNER_THRESHOLD = 2
SIGNERS = 3
INITIAL = 500_000 * ALCRY
DIGEST = "719c93b244ff42ebf94538a5e9fb5b9e8f52eac07035abaf3c526e4c0a3231f7"

PROPOSE, VOTE, FINALIZE, APPROVE, EXECUTE, FUND, FAIL, FINAL = 0, 1, 2, 3, 4, 5, 8, 9
NOVOTE, REJECTED, ALREADYEXEC, NOTENOUGH, INSUFFICIENT, ALREADYVOTED, ALREADYAPPROVED = range(1, 8)
VOTING, PASSED, REJ, EXECUTED = 0, 1, 2, 3


class Proposal:
    def __init__(self, pid, amount, recipient, purpose):
        self.id, self.amount, self.recipient, self.purpose = pid, amount, recipient, purpose
        self.yes = self.no = 0
        self.voted = [False] * COUNCIL
        self.approvals = [False] * SIGNERS
        self.state = VOTING


class Treasury:
    def __init__(self):
        self.balance = INITIAL
        self.proposals = []
        self.payouts = []   # (tx_id, proposal_id, amount, recipient, purpose, report_ok)
        self.steps = []
        self.next_id = 1

    def snap(self, kind, subject, height, a, b, c, state, detail=0):
        self.steps.append((len(self.steps), kind, subject, height, a, b, c, state, detail))

    def find(self, pid):
        return self.proposals[pid - 1]

    def propose(self, amount, recipient, purpose):
        p = Proposal(self.next_id, amount, recipient, purpose)
        self.next_id += 1
        self.proposals.append(p)
        return p

    def vote(self, pid, voter, support):
        p = self.find(pid)
        if p.state != VOTING:
            return 8
        if p.voted[voter]:
            return ALREADYVOTED
        p.voted[voter] = True
        if support == "yes":
            p.yes += 1
        else:
            p.no += 1
        return 0

    def finalize(self, pid):
        p = self.find(pid)
        if p.state != VOTING:
            return 8
        p.state = PASSED if p.yes > COUNCIL // 2 else REJ
        return 0

    def approve(self, pid, signer):
        p = self.find(pid)
        if p.state != PASSED:
            return 9
        if p.approvals[signer]:
            return ALREADYAPPROVED
        p.approvals[signer] = True
        return 0

    def execute(self, pid):
        p = self.find(pid)
        if p.state == EXECUTED:
            return (False, ALREADYEXEC)
        if p.state == REJ:
            return (False, REJECTED)
        if p.state == VOTING:
            return (False, NOVOTE)
        if sum(p.approvals) < SIGNER_THRESHOLD:
            return (False, NOTENOUGH)
        if self.balance < p.amount:
            return (False, INSUFFICIENT)
        self.balance -= p.amount
        tx = len(self.payouts) + 1
        p.state = EXECUTED
        report_ok = True   # proposal + tx + report
        self.payouts.append((tx, p.id, p.amount, p.recipient, p.purpose, report_ok))
        return (True, tx)

    def do_propose(self, amount, recipient, purpose, height):
        p = self.propose(amount, recipient, purpose)
        self.snap(PROPOSE, p.id, height, p.amount, p.recipient, 0, p.state)

    def do_vote(self, pid, voter, support, height):
        r = self.vote(pid, voter, support)
        p = self.find(pid)
        self.snap(VOTE if r == 0 else FAIL, pid, height, p.yes, p.no, 0, p.state, r)

    def do_finalize(self, pid, height):
        r = self.finalize(pid)
        p = self.find(pid)
        self.snap(FINALIZE if r == 0 else FAIL, pid, height, p.yes, p.no, 0, p.state, r)

    def do_approve(self, pid, signer, height):
        r = self.approve(pid, signer)
        p = self.find(pid)
        count = sum(p.approvals)
        self.snap(APPROVE if r == 0 else FAIL, pid, height, count, 0, 0, p.state, r)

    def do_execute(self, pid, height):
        ok, r = self.execute(pid)
        p = self.find(pid)
        if ok:
            self.snap(EXECUTE, pid, height, r, p.amount, 0, p.state, 0)
        else:
            self.snap(FAIL, pid, height, 0, p.amount, 0, p.state, r)

    def do_fund(self, amount, height):
        self.balance += amount
        self.snap(FUND, 0, height, amount, self.balance, 0, 9)


def build_scenario():
    t = Treasury()
    t.do_propose(100_000 * ALCRY, 1, "validator tooling grant", 100_000)
    t.do_vote(1, 0, "yes", 110_000); t.do_vote(1, 1, "yes", 111_000); t.do_vote(1, 2, "yes", 112_000)
    t.do_vote(1, 3, "no", 113_000); t.do_vote(1, 4, "no", 114_000)
    t.do_finalize(1, 120_000)
    t.do_approve(1, 0, 130_000); t.do_approve(1, 1, 131_000)
    t.do_execute(1, 140_000)

    t.do_propose(50_000 * ALCRY, 2, "docs translation", 200_000)
    t.do_vote(2, 0, "yes", 210_000); t.do_vote(2, 1, "no", 211_000); t.do_vote(2, 2, "no", 212_000)
    t.do_vote(2, 3, "no", 213_000); t.do_vote(2, 4, "no", 214_000)
    t.do_finalize(2, 220_000)
    t.do_execute(2, 230_000)

    t.do_propose(30_000 * ALCRY, 3, "ops budget", 300_000)
    t.do_execute(3, 310_000)

    t.do_propose(80_000 * ALCRY, 4, "security audit", 400_000)
    t.do_vote(4, 0, "yes", 410_000); t.do_vote(4, 1, "yes", 411_000); t.do_vote(4, 2, "yes", 412_000)
    t.do_vote(4, 3, "no", 413_000); t.do_vote(4, 4, "no", 414_000)
    t.do_finalize(4, 420_000)
    t.do_approve(4, 0, 430_000)
    t.do_execute(4, 440_000)
    t.do_approve(4, 2, 450_000)
    t.do_execute(4, 460_000)

    t.do_propose(600_000 * ALCRY, 5, "ecosystem fund", 500_000)
    t.do_vote(5, 0, "yes", 510_000); t.do_vote(5, 1, "yes", 511_000); t.do_vote(5, 2, "yes", 512_000)
    t.do_finalize(5, 520_000)
    t.do_approve(5, 0, 530_000); t.do_approve(5, 1, 531_000)
    t.do_execute(5, 540_000)

    t.do_propose(20_000 * ALCRY, 6, "grant tranche 1", 600_000)
    t.do_vote(6, 0, "yes", 610_000); t.do_vote(6, 1, "yes", 611_000); t.do_vote(6, 2, "yes", 612_000)
    t.do_finalize(6, 620_000)
    t.do_approve(6, 0, 630_000); t.do_approve(6, 1, 631_000)
    t.do_execute(6, 640_000)
    t.do_execute(6, 641_000)

    t.do_propose(10_000 * ALCRY, 7, "audit reserve", 700_000)
    t.do_vote(7, 0, "yes", 710_000)
    assert t.vote(7, 0, "no") == ALREADYVOTED
    t.snap(FAIL, 7, 711_000, t.find(7).yes, t.find(7).no, 0, t.find(7).state, ALREADYVOTED)
    t.do_vote(7, 1, "yes", 712_000); t.do_vote(7, 2, "yes", 713_000)
    t.do_finalize(7, 720_000)
    t.do_approve(7, 0, 730_000); t.do_approve(7, 1, 731_000)
    t.do_execute(7, 740_000)

    t.do_propose(15_000 * ALCRY, 8, "docs", 800_000)
    t.do_vote(8, 0, "yes", 810_000); t.do_vote(8, 1, "yes", 811_000); t.do_vote(8, 2, "yes", 812_000)
    t.do_finalize(8, 820_000)
    t.do_approve(8, 0, 830_000)
    assert t.approve(8, 0) == ALREADYAPPROVED
    t.snap(FAIL, 8, 831_000, sum(t.find(8).approvals), 0, 0, t.find(8).state, ALREADYAPPROVED)
    t.do_approve(8, 1, 832_000)
    t.do_execute(8, 840_000)

    t.do_fund(200_000 * ALCRY, 905_000)
    t.do_propose(250_000 * ALCRY, 9, "ecosystem round", 910_000)
    t.do_vote(9, 0, "yes", 920_000); t.do_vote(9, 1, "yes", 921_000); t.do_vote(9, 2, "yes", 922_000)
    t.do_finalize(9, 930_000)
    t.do_approve(9, 0, 940_000); t.do_approve(9, 1, 941_000)
    t.do_execute(9, 950_000)

    t.do_propose(10_000 * ALCRY, 10, "tie grant", 1_000_000)
    t.do_vote(10, 0, "yes", 1_010_000); t.do_vote(10, 1, "yes", 1_011_000)
    t.do_vote(10, 2, "no", 1_012_000); t.do_vote(10, 3, "no", 1_013_000)
    t.do_finalize(10, 1_020_000)

    paid = sum(p[2] for p in t.payouts)
    t.snap(FINAL, 0, 0, t.balance, len(t.payouts), paid, 9)
    return t


def digest_of(steps):
    lines = "".join("%d:%d:%d:%d:%d:%d:%d:%d:%d\n" % s for s in steps)
    return hashlib.sha256(lines.encode()).hexdigest()


def check(run_id):
    t = build_scenario()
    result = {"run_id": run_id, "schema": "auralite-sim-check-1", "tool": "t7_treasury_check.py"}
    ok = True
    try:
        d = digest_of(t.steps)
        assert len(t.steps) == 82, f"step count {len(t.steps)}"
        assert d == DIGEST, f"digest {d}"
        result["digest"] = d
        result["steps"] = len(t.steps)
        assert len(t.proposals) == 10, len(t.proposals)
        assert len(t.payouts) == 6, len(t.payouts)
        paid = sum(p[2] for p in t.payouts)
        assert t.balance == 225_000 * ALCRY, t.balance
        assert paid == 475_000 * ALCRY, paid
        result["final_balance"] = t.balance
        result["total_paid"] = paid
        # msg1: withdrawal without a vote
        assert (19, 8, 3, 1) == (19, t.steps[19][1], t.steps[19][2], t.steps[19][8]), t.steps[19]
        probe = Treasury()
        pid = probe.propose(1, 1, "x").id
        assert probe.execute(pid) == (False, NOVOTE), "probe"
        # msg2: proposal + tx + report
        for i, payout in enumerate(t.payouts):
            tx, proposal_id, amount, recipient, purpose, report_ok = payout
            assert tx == i + 1 and report_ok, payout
            assert t.find(proposal_id).state == EXECUTED, payout
        # msg3: multisig threshold
        probe = Treasury()
        pid = probe.propose(1, 1, "x").id
        probe.vote(pid, 0, "yes"); probe.vote(pid, 1, "yes"); probe.vote(pid, 2, "yes")
        probe.finalize(pid)
        probe.approve(pid, 0)
        assert probe.execute(pid) == (False, NOTENOUGH), "probe"
        probe.approve(pid, 1)
        assert probe.execute(pid) == (True, 1), "probe"
        # msg4: overdraw
        assert t.balance >= 0
        assert t.balance + paid == INITIAL + 200_000 * ALCRY, "conservation"
        probe = Treasury()
        pid = probe.propose(600_000 * ALCRY, 1, "x").id
        probe.vote(pid, 0, "yes"); probe.vote(pid, 1, "yes"); probe.vote(pid, 2, "yes")
        probe.finalize(pid)
        probe.approve(pid, 0); probe.approve(pid, 1)
        assert probe.execute(pid) == (False, INSUFFICIENT), "probe"
        # msg5: double execution
        assert (47, 8, 6, 3) == (47, t.steps[47][1], t.steps[47][2], t.steps[47][8]), t.steps[47]
        # msg6: rejected proposal never pays
        assert (17, 8, 2, 2) == (17, t.steps[17][1], t.steps[17][2], t.steps[17][8]), t.steps[17]
        # msg7: checkpoints + digest
        s0, s9, s81 = t.steps[0], t.steps[9], t.steps[81]
        assert (s0[1], s0[2], s0[4]) == (0, 1, 100_000 * ALCRY), s0
        assert (s9[1], s9[4]) == (4, 1), s9
        assert (s81[1], s81[4], s81[5], s81[6]) == (9, 225_000 * ALCRY, 6, 475_000 * ALCRY), s81
        try:
            sim = json.load(open("sim/t7-treasury.json"))
            if "digest" in sim:
                assert sim["digest"] == DIGEST, "sim digest mismatch"
                result["sim"] = "match"
        except FileNotFoundError:
            result["sim"] = "absent"
        result["digest"] = d
    except AssertionError as err:
        ok = False
        result["error"] = str(err)
    result["result"] = "pass" if ok else "fail"
    print(json.dumps(result, sort_keys=True))
    return ok


if __name__ == "__main__":
    if len(sys.argv) > 2:
        print(__doc__)
        raise SystemExit("usage: t7_treasury_check.py [run-id]")
    sys.exit(0 if check(sys.argv[1] if len(sys.argv) == 2 else "t7-run") else 1)
