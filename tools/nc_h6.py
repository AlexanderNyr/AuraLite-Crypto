#!/usr/bin/env python3
"""Mutation controls for H6 (docs/h6-economic-plan.md §5).

Each control injects ONE bug (restored after) and requires the named H6
tests to go RED with the named message. NC-1 is the phase gate literally:
remove the pause -> the gate must turn red. Controls for the modelled
surfaces (oracle/bridge) inject the bug into the executable model — the
same guard a future native implementation must pass (plan §3).
"""

import hashlib
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GOV = os.path.join(ROOT, "crates/aura-consensus/src/governance.rs")
REWARDS = os.path.join(ROOT, "crates/aura-consensus/src/rewards.rs")
SLASHING = os.path.join(ROOT, "crates/aura-consensus/src/slashing.rs")
PROPOSER = os.path.join(ROOT, "crates/aura-consensus/src/proposer.rs")
MEMPOOL = os.path.join(ROOT, "crates/aura-mempool/src/lib.rs")
H6_TEST = os.path.join(ROOT, "crates/aura-consensus/tests/h6_economic.rs")

CONSENSUS_CMD = ["cargo", "test", "--locked", "-p", "aura-consensus",
                 "--test", "h6_economic"]
MEMPOOL_CMD = ["cargo", "test", "--locked", "-p", "aura-mempool",
               "--test", "h6_mev"]

# (name, path, anchor, replacement, test cmd, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> no pause must turn the gate red",
        GOV,
        "    ) -> Result<u64, GovernanceError> {\n"
        "        if self.emergency_pause.paused {\n",
        "    ) -> Result<u64, GovernanceError> {\n"
        "        if false {\n",
        CONSENSUS_CMD + ["h6_emergency_pause_halts_actions", "--", "--exact"],
        "H6: emergency pause must halt governance actions",
    ),
    (
        "NC-2 emission minted from nowhere escapes the ledger",
        REWARDS,
        "        self.total_minted += minted;\n",
        "        self.total_minted += minted;\n"
        "        *self.balances.entry(proposer).or_insert(0) += minted;\n",
        CONSENSUS_CMD + ["h6_emission_conserved_under_attack", "--", "--exact"],
        "H6: emission must stay conserved under attack",
    ),
    (
        "NC-3 slash receipt without burning stake",
        SLASHING,
        "        let stake_after = stake_before.saturating_sub(penalty_amount);\n",
        "        let stake_after = stake_before;\n",
        CONSENSUS_CMD + ["h6_stake_grinding_bounded", "--", "--exact"],
        "H6: attacker damage must be bounded by slashing",
    ),
    (
        "NC-4 proposer rotation pinned to one validator (permanent censorship)",
        PROPOSER,
        "        self.validators[round as usize % self.validators.len()]\n",
        "        self.validators[0]\n",
        CONSENSUS_CMD + ["h6_censorship_bounded_by_rotation", "--", "--exact"],
        "H6: censorship must be bounded by proposer rotation",
    ),
    (
        "NC-5 canonical order made discretionary (reversed)",
        MEMPOOL,
        "        self.transactions.values().cloned().collect()\n",
        "        self.transactions.values().rev().cloned().collect()\n",
        MEMPOOL_CMD + ["h6_canonical_ordering_deterministic", "--", "--exact"],
        "H6: MEV extraction must stay bounded by canonical ordering",
    ),
    (
        "NC-6 oracle median replaced by max (manipulation unbounded)",
        H6_TEST,
        "    expanded[expanded.len() / 2]\n",
        "    expanded[expanded.len() - 1]\n",
        CONSENSUS_CMD + ["h6_oracle_manipulation_bounded", "--", "--exact"],
        "H6: oracle manipulation must stay within report bounds",
    ),
    (
        "NC-7 bridge reserve bound disabled (drain allowed)",
        H6_TEST,
        "    if state.withdrawn.saturating_add(amount) > state.reserves {\n",
        "    if false {\n",
        CONSENSUS_CMD + ["h6_bridge_drain_bounded", "--", "--exact"],
        "H6: bridge withdrawals must stay within reserves",
    ),
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def run_control(name, path, anchor, replacement, cmd, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
    compile_failed = "error[" in output and "test result" not in output
    if compile_failed:
        return False, "mutation broke compilation"
    if "test result: ok" in output:
        return False, "mutated test still passes"
    if expected in output:
        return True, "red with named message"
    return False, "red without named message"


def main():
    passed = 0
    for name, path, anchor, replacement, cmd, expected in CONTROLS:
        ok, note = run_control(name, path, anchor, replacement, cmd, expected)
        line = f"[{'PASS' if ok else 'FAIL'}] {name}: {note}"
        print(line, flush=True)
        passed += bool(ok)
    print("RESULT: %d/%d negative controls PASS" % (passed, len(CONTROLS)),
          flush=True)
    print("control-anchors sha256: gov=%s rewards=%s"
          % (sha256_file(GOV)[:16], sha256_file(REWARDS)[:16]), flush=True)
    return 0 if passed == len(CONTROLS) else 1


if __name__ == "__main__":
    sys.exit(main())
