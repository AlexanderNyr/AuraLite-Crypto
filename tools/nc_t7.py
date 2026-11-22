#!/usr/bin/env python3
"""T7 negative-control runner (7 mutations of crates/aura-storage/src/treasury.rs)."""
import os
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "crates/aura-storage/src/treasury.rs"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t7_treasury"]
PASS_MARKERS = ("test result: ok.",)
MUTATIONS = [
    ("if proposal.state == ProposalState::Voting {",
     "if false {",
     "T7: a withdrawal without a vote must be rejected"),
    ("report: Some(report),",
     "report: None,",
     "T7: every payout must have a proposal, a tx and a report"),
    ("if approvals < T7_SIGNER_THRESHOLD as usize {",
     "if approvals < T7_SIGNER_THRESHOLD as usize - 1 {",
     "T7: the multisig threshold must be enforced"),
    ("if self.balance < proposal.amount as i128 {",
     "if false {",
     "T7: the treasury must not overdraw"),
    ("if proposal.state == ProposalState::Executed {",
     "if false {",
     "T7: a payout must not execute twice"),
    ("if proposal.state == ProposalState::Rejected {",
     "if false {",
     "T7: a rejected proposal must not pay out"),
    ("pub const T7_SIGNER_THRESHOLD: u32 = 2;",
     "pub const T7_SIGNER_THRESHOLD: u32 = 3;",
     "T7: the reference simulation must agree with the pinned digest"),
]


def run_control(name, path, anchor, replacement, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(TEST_CMD, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
    compile_failed = "error[" in output and "test result" not in output
    if compile_failed:
        return False, "mutation broke compilation"
    if any(marker in output for marker in PASS_MARKERS):
        return False, "mutated gate still passes"
    if expected in output:
        return True, "red with named message"
    return False, "red without named message"


def main():
    print("running negative controls against", TARGET)
    names = [
        "NC-1 withdrawal without vote executed",
        "NC-2 payout report dropped",
        "NC-3 multisig threshold weakened",
        "NC-4 overdraw guard removed",
        "NC-5 double execution allowed",
        "NC-6 rejected proposal pays out",
        "NC-7 pinned parameters drift",
    ]
    for i, (anchor, replacement, expected) in enumerate(MUTATIONS):
        ok, note = run_control(names[i], TARGET, anchor, replacement, expected)
        print("%s: %s" % (names[i].split()[0], note))
        if not ok:
            print("negative control %d failed" % (i + 1))
            raise SystemExit(1)
    print("RESULT: 7/7 negative controls PASS")


if __name__ == "__main__":
    main()
