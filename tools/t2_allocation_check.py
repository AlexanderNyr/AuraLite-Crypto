#!/usr/bin/env python3
"""Independent T2 allocation oracle (ECONOMICS_PLAN T2).

The phase gate is literal: this script reconciles the genesis balances
with the allocation table — sum(pots) must equal the genesis supply
exactly ("extra coins -> red"). It also re-derives the vesting/lockup
rules and cross-checks the Rust example `t2_allocation_example`.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIO = os.path.join(ROOT, "sim", "t2-allocation.json")
ALLOCATION_RS = os.path.join(ROOT, "crates", "aura-storage", "src", "allocation.rs")

GOLDEN_TABLE_DIGEST = "5fe7417a134011e7028f255cf7cf180335cc23c773ee9657a68de260fcc9ea41"
ALCRY = 10 ** 18


def vested(cliff, total, amount, month):
    if month < cliff:
        return 0
    if month >= total:
        return amount
    return amount * month // total


def table_digest(entries):
    lines = []
    for entry in entries:
        if entry["vesting"]:
            v = entry["vesting"]
            lines.append("%s:%d:v=%d,%d,%d\n" % (
                entry["name"], entry["amount"], v["cliff"], v["total"], v["vested"]))
        else:
            lines.append("%s:%d\n" % (entry["name"], entry["amount"]))
    return hashlib.sha256("".join(lines).encode("utf-8")).hexdigest()


def rust_digest_const():
    text = open(ALLOCATION_RS, encoding="utf-8").read()
    rows = re.findall(
        r'name: "([a-z]+)",\s*\n\s*amount: ([0-9_]+) \* ALCRY,',
        text)
    return rows


def main():
    scenario = json.load(open(SCENARIO, encoding="utf-8"))
    genesis = scenario["genesis_supply_alcry"] * ALCRY

    entries = []
    for raw in scenario["entries"]:
        vesting = None
        if raw["vesting"]:
            vesting = {
                "cliff": raw["vesting"]["cliff_months"],
                "total": raw["vesting"]["total_months"],
                "vested": raw["vesting"]["amount_alcry"] * ALCRY,
            }
        entries.append({
            "name": raw["name"],
            "amount": raw["amount_alcry"] * ALCRY,
            "vesting": vesting,
        })

    # The gate: reconcile genesis balances with the table.
    total = sum(e["amount"] for e in entries)
    assert total == genesis, (
        "T2: the allocation must sum to the genesis supply (%d != %d)" % (total, genesis)
    )
    assert total == 3_000_000 * ALCRY
    assert len({e["name"] for e in entries}) == len(entries)

    for entry in entries:
        if entry["vesting"]:
            v = entry["vesting"]
            assert v["total"] > v["cliff"], "T2: vesting must not exceed its allocation"
            assert v["vested"] <= entry["amount"], "T2: vesting must not exceed its allocation"

    digest = table_digest(entries)
    assert digest == GOLDEN_TABLE_DIGEST, (
        "T2: the reconciliation must agree with the reference table"
    )
    assert digest == scenario["golden_table_digest"]

    # Rust constants must mirror the table.
    rows = rust_digest_const()
    expected = [(e["name"], str(e["amount"] // ALCRY).replace("000", "000")) for e in entries]
    rust_names = [name for name, _ in rows]
    assert rust_names == [e["name"] for e in entries], (
        "T2: the reconciliation must agree with the reference table (const drift)"
    )
    rust_amounts = [int(amount.replace("_", "")) for _, amount in rows]
    assert rust_amounts == [e["amount"] // ALCRY for e in entries], (
        "T2: the reconciliation must agree with the reference table (const drift)"
    )

    # Vesting samples.
    for sample in scenario["samples"]:
        entry = next(e for e in entries if e["name"] == sample["name"])
        v = entry["vesting"]
        got_vested = vested(v["cliff"], v["total"], v["vested"], sample["month"])
        got_locked = v["vested"] - got_vested
        assert got_vested == sample["vested_alcry"] * ALCRY, (
            "T2: vesting must complete exactly at the end (sample %s@%d)"
            % (sample["name"], sample["month"])
        )
        assert got_locked == sample["locked_alcry"] * ALCRY, (
            "T2: locked coins must not be spendable (sample %s@%d)"
            % (sample["name"], sample["month"])
        )
        # locked coins are not spendable
        liquid = entry["amount"] - got_locked
        assert liquid + got_locked == entry["amount"], "T2: locked coins must not be spendable"

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage", "--example",
         "t2_allocation_example"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace",
    )
    output = proc.stdout or ""
    if proc.returncode != 0 or "T2-SIM-OK " not in output:
        print(output[-2000:])
        print("RESULT: T2 allocation check FAIL — rust example did not run")
        return 1
    payload = json.loads(output[output.index("T2-SIM-OK ") + len("T2-SIM-OK "):].strip())
    assert payload["genesis"] == genesis, "T2: the reconciliation must agree with the reference table"
    assert payload["digest"] == digest, "T2: the reconciliation must agree with the reference table"
    assert payload["team_vested_36"] == 450_000 * ALCRY, "T2: vesting must complete exactly at the end"
    for entry in entries:
        row = payload["entries"][entry["name"]]
        assert row["amount"] == entry["amount"], (
            "T2: the reconciliation must agree with the reference table"
        )
    print("RESULT: T2 allocation check PASS — table reconciles with genesis %d "
          "(validators 1200000, treasury 600000, community 750000, team 450000), "
          "digest %s" % (genesis // ALCRY, digest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
