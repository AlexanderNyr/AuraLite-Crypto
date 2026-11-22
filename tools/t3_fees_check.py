#!/usr/bin/env python3
"""Independent T3 fees oracle (ECONOMICS_PLAN T3).

Re-derives the EIP-1559-like fee market over the pinned 10 000
congested blocks: the base fee must rise and fall **per the formula** at
every step, the base part must be burned and only the tip goes to the
validator. Cross-checks the Rust example `t3_fees_example` and the
constants in `crates/aura-storage/src/fees.rs`.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIO = os.path.join(ROOT, "sim", "t3-fees.json")
FEES_RS = os.path.join(ROOT, "crates", "aura-storage", "src", "fees.rs")

GOLDEN_FEE_DIGEST = "412f1f9e120ec829fb5d2ae65e4590da69771cd7b49bef83f29440d0d93bc124"
INITIAL = 10 ** 9
TARGET = 15_000_000
DENOM = 8
LIMIT = 30_000_000
TIP = 10 ** 9
BLOCKS = 10_000


def demand(i):
    segment = i // 250
    kind = segment % 5
    if kind == 0:
        return TARGET + TARGET // 4
    if kind == 1:
        return TARGET - TARGET // 4
    if kind == 2:
        return TARGET + TARGET // 2
    if kind == 3:
        return TARGET - TARGET // 2
    return TARGET


def next_base(parent, gas):
    if gas == TARGET:
        return parent
    if gas > TARGET:
        delta = max(parent * (gas - TARGET) // TARGET // DENOM, 1)
        return parent + delta
    delta = max(parent * (TARGET - gas) // TARGET // DENOM, 1)
    return max(parent - delta, 0)


def rust_const(name):
    text = open(FEES_RS, encoding="utf-8").read()
    match = re.search(r"pub const %s: (?:Amount|u64|u32) = ([0-9_]+);" % name, text)
    if match:
        return int(match.group(1).replace("_", ""))
    # expression form: T3_GAS_LIMIT = T3_GAS_TARGET * T3_ELASTICITY
    expr = re.search(r"pub const %s: u64 = ([A-Z0-9_ *]+);" % name, text)
    if expr:
        value = 1
        for factor in expr.group(1).split("*"):
            value *= rust_const(factor.strip())
        return value
    return None


def main():
    scenario = json.load(open(SCENARIO, encoding="utf-8"))
    assert scenario["blocks"] == BLOCKS
    assert scenario["initial_base_fee"] == INITIAL
    assert scenario["gas_target"] == TARGET
    assert scenario["gas_limit"] == LIMIT
    assert scenario["base_fee_max_change_denominator"] == DENOM
    assert scenario["tip_per_gas"] == TIP

    # Rust constants must mirror the scenario (reference check).
    for name, value in [
        ("T3_INITIAL_BASE_FEE", INITIAL),
        ("T3_GAS_TARGET", TARGET),
        ("T3_GAS_LIMIT", LIMIT),
        ("T3_BASE_FEE_MAX_CHANGE_DENOMINATOR", DENOM),
        ("T3_TIP_PER_GAS", TIP),
        ("T3_BLOCKS", BLOCKS),
    ]:
        got = rust_const(name)
        assert got == value, (
            "T3: the reference simulation must agree with the pinned digest "
            "(const %s drift: %r != %r)" % (name, got, value)
        )

    base = INITIAL
    total_burned = 0
    total_tips = 0
    peak = 0
    text = ""
    for i in range(BLOCKS):
        gas = demand(i)
        assert gas <= LIMIT, "T3: a block must not exceed the elastic gas limit"
        burned = gas * base
        tips = gas * TIP
        total_burned += burned
        total_tips += tips
        after = next_base(base, gas)
        text += "%d:%d:%d:%d:%d:%d\n" % (i, base, gas, after, burned, tips)
        segment = (i // 250) % 5
        if segment in (0, 2):
            assert after > base, (
                "T3: congestion must raise the base fee and slack must lower it"
            )
        elif segment in (1, 3):
            assert after < base, (
                "T3: congestion must raise the base fee and slack must lower it"
            )
        else:
            assert after == base, (
                "T3: congestion must raise the base fee and slack must lower it"
            )
        base = after
        peak = max(peak, base)

    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    assert digest == GOLDEN_FEE_DIGEST, (
        "T3: the reference simulation must agree with the pinned digest"
    )
    assert digest == scenario["golden_path_digest"]
    assert total_burned == scenario["totals"]["total_burned"], (
        "T3: the base part must be burned"
    )
    assert total_tips == scenario["totals"]["total_tips"], (
        "T3: the tip must go to the validator"
    )
    assert peak == scenario["totals"]["peak_base_after"]

    sample_base = INITIAL
    sample_map = {s["after_block"]: s["base_after"] for s in scenario["samples"]}
    for i in range(max(sample_map) + 1):
        sample_base = next_base(sample_base, demand(i))
        if i in sample_map:
            assert sample_base == sample_map[i], (
                "T3: the reference simulation must agree with the pinned digest "
                "(sample %d)" % i
            )

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage", "--example",
         "t3_fees_example"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace",
    )
    output = proc.stdout or ""
    if proc.returncode != 0 or "T3-SIM-OK " not in output:
        print(output[-2000:])
        print("RESULT: T3 fees check FAIL — rust example did not run")
        return 1
    payload = json.loads(output[output.index("T3-SIM-OK ") + len("T3-SIM-OK "):].strip())
    assert payload["blocks"] == BLOCKS
    assert payload["digest"] == digest, (
        "T3: the reference simulation must agree with the pinned digest"
    )
    assert payload["final_base"] == base, (
        "T3: the reference simulation must agree with the pinned digest"
    )
    assert payload["peak_base"] == peak
    assert payload["total_burned"] == total_burned, "T3: the base part must be burned"
    assert payload["total_tips"] == total_tips, "T3: the tip must go to the validator"
    print("RESULT: T3 fees check PASS — 10000 congested blocks follow the formula "
          "(base 1e9 -> %d, peak %d), burned %d, tips %d, digest %s"
          % (base, peak, total_burned, total_tips, digest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
