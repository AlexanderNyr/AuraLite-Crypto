#!/usr/bin/env python3
"""Deterministic oracle for E3 (ECOSYSTEM_PLAN E3).

Gate: "1000 requests: 1 payout/address/day; SDK examples pass in CI".
Re-simulates the 1000-request script with an independent Python model
of the faucet policy and checks the totals pinned in
tools/fixtures/e3_faucet_script.json, the named gate messages in the
E3 test, and the report.
"""

import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURE = os.path.join(ROOT, "tools", "fixtures", "e3_faucet_script.json")
TEST = os.path.join(ROOT, "crates", "aura-storage", "tests", "e3_faucet.rs")
REPORT = os.path.join(ROOT, "docs", "ecosystem", "e3-faucet-sdk-report.md")

DAY = 86_400
NAMED = [
    "E3: faucet must pay 1 payout per address per day",
    "E3: faucet must refuse non-testnet requests",
    "E3: faucet must stop when out of funds",
    "E3: the daily window must roll over",
    "E3: the faucet HTTP API must apply the same policy as the library",
    "E3: the faucet must validate addresses",
]


class PolicyModel:
    """Independent E3 faucet policy."""

    def __init__(self, payout, funding):
        self.payout = payout
        self.funds = funding
        self.paid = set()
        self.payouts = 0

    def request(self, now, network, address):
        if not address or len(address) > 64:
            return "denied_bad_address"
        if network != "testnet":
            return "denied_wrong_network"
        key = (address, now // DAY)
        if key in self.paid:
            return "denied_rate_limit"
        if self.funds < self.payout:
            return "denied_out_of_funds"
        self.funds -= self.payout
        self.paid.add(key)
        self.payouts += 1
        return "paid"


def main():
    with open(FIXTURE, encoding="utf-8") as f:
        fixture = json.load(f)
    with open(TEST, encoding="utf-8") as f:
        test_src = f.read()
    with open(REPORT, encoding="utf-8") as f:
        report = f.read()

    problems = []

    # --- independent re-simulation of the 1000-request script ------
    model = PolicyModel(fixture["payout"], fixture["funding"])
    paid = 0
    rate_denied = 0
    per_key = {}
    for day in range(fixture["days"]):
        for i in range(fixture["addresses"]):
            address = "addr-%03d" % i
            if model.request(day * DAY + 3600, "testnet", address) == "paid":
                paid += 1
                per_key[(address, day)] = per_key.get((address, day), 0) + 1
    for i in range(fixture["repeats"]):
        address = "addr-%03d" % i
        if model.request(3600, "testnet", address) == "denied_rate_limit":
            rate_denied += 1

    requests = fixture["days"] * fixture["addresses"] + fixture["repeats"]
    if requests != fixture["requests"]:
        problems.append("fixture request count must be consistent")
    if paid != fixture["expected_payouts"]:
        problems.append("independent model must pay 1 payout per address per day")
    if rate_denied != fixture["expected_rate_denied"]:
        problems.append("independent model must deny same-day repeats")
    if any(count != 1 for count in per_key.values()):
        problems.append("independent model must pay 1 payout per address per day")

    # --- named gate messages asserted in the test ------------------
    for message in NAMED:
        if message not in test_src:
            problems.append("every named gate message must be asserted in the E3 test")
            break

    # --- report ----------------------------------------------------
    if "1000" not in report or "1 payout" not in report:
        problems.append("report must state the 1000-request gate")
    if "SDK examples pass in CI" not in report:
        problems.append("report must state the SDK examples gate")
    for message in NAMED:
        if message not in report:
            problems.append("report must cover the named gate messages")
            break

    for p in problems:
        print("FAIL:", p)
    if problems:
        print("RESULT: E3 faucet check FAIL")
        return 1
    print("RESULT: E3 faucet check PASS — %d requests, %d payouts, 1 payout per address per day"
          % (requests, paid))
    return 0


if __name__ == "__main__":
    sys.exit(main())
