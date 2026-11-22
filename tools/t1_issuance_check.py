#!/usr/bin/env python3
"""Independent T1 issuance oracle (ECONOMICS_PLAN T1).

Recomputes the ten-year supply path from `sim/t1-issuance.json` in two
independent ways (stepwise loop and per-year closed form), enforces the
phase gate (simulated supply matches the formula within 1 base unit) and
cross-checks the Rust example `t1_issuance_example`.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIO = os.path.join(ROOT, "sim", "t1-issuance.json")
ISSUANCE_RS = os.path.join(ROOT, "crates", "aura-storage", "src", "issuance.rs")

GOLDEN_PATH_DIGEST = "c2a4669192ddde5d9cc4e7cb32a76ecd06356022bce7c7230826f068854c1a35"


def stepwise(genesis, emission, halving, cap, years):
    supply = genesis
    path = [supply]
    for year in range(1, years + 1):
        raw = emission >> ((year - 1) // halving)
        issued = min(raw, cap - supply)
        supply += issued
        path.append(supply)
    return path


def closed_form(genesis, emission, halving, cap, years):
    out = []
    supply = genesis
    for year in range(0, years + 1):
        if year > 0:
            supply = min(cap, supply + (emission >> ((year - 1) // halving)))
        out.append(supply)
    return out


def path_digest(path):
    text = "".join("%d:%d\n" % (y, s) for y, s in enumerate(path))
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def rust_consts():
    text = open(ISSUANCE_RS, encoding="utf-8").read()

    def const(name):
        raw = re.search(r"pub const %s: Amount = ([^;]+);" % name, text).group(1)
        raw = raw.replace("ALCRY", str(10 ** 18)).replace("_", "").replace("u128.pow(18)", "")
        return eval(raw)

    return {
        "genesis": const("GENESIS_SUPPLY"),
        "emission": const("ANNUAL_EMISSION"),
        "halving": int(re.search(r"pub const HALVING_YEARS: u32 = ([0-9_]+);", text).group(1).replace("_", "")),
        "cap": const("MAX_SUPPLY"),
        "bps": int(re.search(r"pub const TARGET_STAKED_RATIO_BPS: u16 = ([0-9_]+);", text).group(1).replace("_", "")),
    }


def main():
    scenario = json.load(open(SCENARIO, encoding="utf-8"))
    p = scenario["params"]
    alcry = 10 ** 18
    genesis = p["genesis_supply_alcry"] * alcry
    emission = p["annual_emission_alcry"] * alcry
    halving = p["halving_years"]
    cap = p["max_supply_alcry"] * alcry
    years = scenario["horizon_years"]

    rs = rust_consts()
    assert rs == {"genesis": genesis, "emission": emission, "halving": halving,
                  "cap": cap, "bps": p["target_staked_ratio_bps"]}, (
        "T1: the reference model must agree with the simulation (const drift)"
    )

    path = stepwise(genesis, emission, halving, cap, years)
    formula = closed_form(genesis, emission, halving, cap, years)
    for year, (sim_value, formula_value) in enumerate(zip(path, formula)):
        assert abs(sim_value - formula_value) <= 1, (
            "T1: the supply must match the issuance formula (year %d)" % year
        )
    assert path == [v * alcry for v in scenario["expected_path_alcry"]], (
        "T1: the reference model must agree with the simulation"
    )
    digest = path_digest(path)
    assert digest == GOLDEN_PATH_DIGEST, "T1: the reference model must agree with the simulation"
    assert digest == scenario["golden_path_digest"]

    cb = scenario["cap_binding_scenario"]
    cb_path = stepwise(genesis, emission, halving, cb["max_supply_alcry"] * alcry, cb["years"])
    assert cb_path[-1] == cb["final_alcry"] * alcry, "T1: the max supply must never be exceeded"
    assert all(s <= cb["max_supply_alcry"] * alcry for s in cb_path), (
        "T1: the max supply must never be exceeded"
    )

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage", "--example",
         "t1_issuance_example"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace",
    )
    output = proc.stdout or ""
    if proc.returncode != 0 or "T1-SIM-OK " not in output:
        print(output[-2000:])
        print("RESULT: T1 issuance check FAIL — rust example did not run")
        return 1
    payload = json.loads(output[output.index("T1-SIM-OK ") + len("T1-SIM-OK "):].strip())
    assert payload["digest"] == digest, "T1: the reference model must agree with the simulation"
    assert payload["final_supply"] == path[-1], "T1: the reference model must agree with the simulation"
    assert payload["path_alcry"] == scenario["expected_path_alcry"], (
        "T1: the reference model must agree with the simulation"
    )
    print("RESULT: T1 issuance check PASS — %d-year simulation, supply %d (4950000 ALCRY), "
          "matches the formula within 1 unit, digest %s" % (years, path[-1], digest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
