#!/usr/bin/env python3
"""Mutation controls for E3 (ECOSYSTEM_PLAN E3).

Each control injects ONE bug (restored after) and requires the named E3
message to appear in the red run. NC-1 is the phase gate literally:
'1000 requests: 1 payout/address/day' must go red when the same-day
repeat check is removed.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FAUCET = os.path.join(ROOT, "crates", "aura-storage/src/faucet.rs")

TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage",
            "--test", "e3_faucet"]

# (name, path, anchor, replacement, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> 1 payout/address/day must go red",
        FAUCET,
        "        if self.paid.contains(&key) {",
        "        if false {",
        "E3: faucet must pay 1 payout per address per day",
    ),
    (
        "NC-2 testnet-only guard removed",
        FAUCET,
        "        if network != self.network {",
        "        if false {",
        "E3: faucet must refuse non-testnet requests",
    ),
    (
        "NC-3 out-of-funds stop removed",
        FAUCET,
        "        if self.funds < self.config.payout {",
        "        if false {",
        "E3: faucet must stop when out of funds",
    ),
    (
        "NC-4 daily window never rolls over",
        FAUCET,
        "        let day = now / 86_400;",
        "        let day = 0;",
        "E3: the daily window must roll over",
    ),
    (
        "NC-5 HTTP route bypasses the policy",
        FAUCET,
        "                let outcome = self.request(now, network, &address);",
        "                let outcome = FaucetOutcome::Paid { day: 0, payout: self.config.payout };",
        "E3: the faucet HTTP API must apply the same policy as the library",
    ),
    (
        "NC-6 address validation removed",
        FAUCET,
        "        if address.is_empty() || address.len() > MAX_ADDRESS_LEN {",
        "        if false {",
        "E3: the faucet must validate addresses",
    ),
]

PASS_MARKERS = ("test result: ok.",)


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
    passed = 0
    for name, path, anchor, replacement, expected in CONTROLS:
        ok, why = run_control(name, path, anchor, replacement, expected)
        print("[%s] %s: %s" % ("PASS" if ok else "FAIL", name, why))
        if ok:
            passed += 1
    total = len(CONTROLS)
    print("RESULT: %d/%d negative controls PASS" % (passed, total))
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
