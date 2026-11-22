#!/usr/bin/env python3
"""Independent T0 economics oracle (ECONOMICS_PLAN T0).

Re-implements the deterministic baseline simulation from
`sim/t0-baseline.json` (xorshift64*, cap-at-balance rule) in Python and
cross-checks it against the Rust example `t0_sim`. The phase gate is the
literal gate: "emission from nowhere" must be impossible — the total
supply is verified constant at every one of the 1000 steps.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCENARIO = os.path.join(ROOT, "sim", "t0-baseline.json")
ECON_RS = os.path.join(ROOT, "crates", "aura-storage", "src", "econ.rs")

MASK = (1 << 64) - 1
GOLDEN_DIGEST = "aec12b64eada1b850e7c73ac1e2afce62fb398df0d27365c7af300c295c7a51a"


class XorShift64:
    def __init__(self, seed):
        self.state = seed if seed else 0x9E3779B97F4A7C15

    def next_u64(self):
        s = self.state
        s ^= (s << 13) & MASK
        s ^= s >> 7
        s ^= (s << 17) & MASK
        self.state = s
        return s


def simulate(scenario):
    agents = [(a["name"], a["genesis"]) for a in scenario["agents"]]
    names = [name for name, _ in agents]
    n = len(agents)
    rng = XorShift64(scenario["seed"])
    bal = dict(agents)
    total = sum(bal.values())
    schedule = []
    for _ in range(scenario["steps"]):
        sender = names[rng.next_u64() % n]
        recipient = names[rng.next_u64() % n]
        if recipient == sender:
            idx = rng.next_u64() % n
            recipient = names[idx]
            if recipient == sender:
                recipient = names[(idx + 1) % n]
        amount = 1 + rng.next_u64() % 997
        schedule.append((sender, recipient, amount))
    for step, (sender, recipient, amount) in enumerate(schedule):
        capped = min(amount, bal[sender])
        bal[sender] -= capped
        bal[recipient] += capped
        assert sum(bal.values()) == total, (
            "step %d: emission from nowhere (sum %d != supply %d)"
            % (step, sum(bal.values()), total)
        )
    canon = "".join("%s:%d\n" % (k, bal[k]) for k in sorted(bal))
    digest = hashlib.sha256(canon.encode("utf-8")).hexdigest()
    return bal, total, digest, schedule


def rust_consts():
    text = open(ECON_RS, encoding="utf-8").read()
    seed = int(re.search(r"pub const T0_SEED: u64 = ([0-9_]+);", text).group(1).replace("_", ""))
    steps = int(re.search(r"pub const T0_STEPS: usize = ([0-9_]+);", text).group(1).replace("_", ""))
    agents = re.findall(r'\("([a-z]+)", ([0-9_]+)\)', text)
    agents = [(name, int(value.replace("_", ""))) for name, value in agents]
    return seed, steps, agents


def main():
    scenario = json.load(open(SCENARIO, encoding="utf-8"))
    seed, steps, agents = rust_consts()
    assert seed == scenario["seed"], "T0: the simulation must be deterministic (seed drift)"
    assert steps == scenario["steps"], "T0: the simulation must be deterministic (steps drift)"
    assert agents == [(a["name"], a["genesis"]) for a in scenario["agents"]], (
        "T0: the baseline properties must cover scarcity, security and accessibility (genesis drift)"
    )
    assert scenario["max_send_bps"] == 250

    bal, total, digest, schedule = simulate(scenario)
    assert total == 3_000_000, "T0: transfers must conserve the total supply"
    assert sum(bal.values()) == total, "T0: transfers must conserve the total supply"
    assert digest == GOLDEN_DIGEST, "T0: the simulation must be deterministic"

    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage", "--example", "t0_sim"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, errors="replace",
    )
    output = proc.stdout or ""
    if proc.returncode != 0 or "T0-SIM-OK " not in output:
        print(output[-2000:])
        print("RESULT: T0 economics check FAIL — rust example did not run")
        return 1
    payload = json.loads(output[output.index("T0-SIM-OK ") + len("T0-SIM-OK "):].strip())
    assert payload["digest"] == digest, "T0: the reference model must agree with the simulation"
    assert payload["supply"] == total, "T0: the reference model must agree with the simulation"
    assert payload["sum"] == total, "T0: the reference model must agree with the simulation"
    assert payload["steps"] == scenario["steps"], "T0: the reference model must agree with the simulation"
    for name, value in bal.items():
        assert payload["balances"][name] == value, (
            "T0: the reference model must agree with the simulation"
        )
    print("RESULT: T0 economics check PASS — %d steps, supply constant %d, digest %s"
          % (scenario["steps"], total, digest))
    return 0


if __name__ == "__main__":
    sys.exit(main())
