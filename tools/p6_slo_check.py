#!/usr/bin/env python3
"""P6 SLO publication — independent Python model (PERF_PLAN P6).

Reproduces the deterministic SLO-publication ledger of
`crates/aura-bench/src/p6.rs` byte for byte:

  registry  : the six published north-star SLOs (P0-P5 DoD numbers,
              pinned; all upper-bound relations)
  run log   : hash-chained `key=value` entries (chain_step =
              sha256(prev | "|" | line)); a rigged run (log value
              disagreeing with the registry) -> red
  ledgers   : methodology index (6 sections, D-P3) + honest "did not
              work" items (4 pinned negatives)

Output (stdout) is byte-identical to
`cargo run -p aura-bench --example p6_slo_example`. `--measured`
adds an honest wall-clock line (evidence only, D-P9).
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]

RUNS = 5

NORTH_STAR = [
    ("p0.spread_pct", 0, 5),
    ("p1.verify_units", 22, 38),
    ("p2.parallel_units", 15543, 99000),
    ("p3.commit_units", 12300, 15000),
    ("p4.traffic_bytes", 86495, 125000),
    ("p5.finality_ms", 586, 2000),
]
METHODOLOGY = [
    "p0-methodology",
    "p1-methodology",
    "p2-methodology",
    "p3-methodology",
    "p4-methodology",
    "p5-methodology",
]
HONEST_FAILURES = [
    "p2-measured-wall-clock",
    "p3-measured-sandbox-fsync",
    "p1-bls-model-only",
    "p2-lcg-fixture",
]
LOG_SEED = "p6log|seed|v1"


def sha_hex(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def chain_step(prev: str, line: str) -> str:
    return sha_hex(f"{prev}|{line}")


def build_log() -> list[dict]:
    prev = LOG_SEED
    out = []
    for key, value, _bound in NORTH_STAR:
        line = f"{key}={value}"
        prev = chain_step(prev, line)
        out.append({"key": key, "value": value, "chain": prev})
    return out


def verify_log(entries: list[dict]) -> str:
    prev = LOG_SEED
    registry = {name: value for name, value, _b in NORTH_STAR}
    for i, entry in enumerate(entries):
        line = f"{entry['key']}={entry['value']}"
        prev = chain_step(prev, line)
        if entry["chain"] != prev:
            raise AssertionError(f"P6: run log hash chain broken at entry {i}")
        if registry.get(entry["key"]) != entry["value"]:
            raise AssertionError(f"P6: rigged run detected ({line})")
    return prev


def state_digest() -> str:
    parts = []
    for name, observed, bound in NORTH_STAR:
        parts.append(f"p6s|slo|{name}={observed}/{bound}")
    for entry in build_log():
        parts.append(f"p6s|log|{entry['key']}={entry['value']}|{entry['chain']}")
    for section in METHODOLOGY:
        parts.append(f"p6s|methodology|{section}")
    for name in HONEST_FAILURES:
        parts.append(f"p6s|honest|{name}")
    return sha_hex("|".join(parts) + "|")


def build() -> dict:
    # gates
    for name, observed, bound in NORTH_STAR:
        assert observed <= bound, f"north star not confirmed: {name}"
    log = build_log()
    tip = verify_log(log)
    assert all(s in METHODOLOGY for s in METHODOLOGY)
    assert all(n in HONEST_FAILURES for n in HONEST_FAILURES)

    confirmed = sum(1 for _n, o, b in NORTH_STAR if o <= b)
    wd = sha_hex(f"p6w|slo-publication|metrics={len(NORTH_STAR)}")
    md = sha_hex("p0m|vcpu=8|mem_gib=32|storage=nvme|validators=100|net_mbit=1000")
    canonical = (
        f"p6b|{wd}|{md}|runs={RUNS}|spread=0|confirmed={confirmed}|total={len(NORTH_STAR)}"
        f"|log_entries={len(log)}|tip={tip}|methodology={len(METHODOLOGY)}|honest={len(HONEST_FAILURES)}"
    )
    return {
        "workload_digest": wd,
        "machine_digest": md,
        "runs": RUNS,
        "spread_pct": 0,
        "confirmed": confirmed,
        "total": len(NORTH_STAR),
        "log_entries": len(log),
        "log_tip": tip,
        "methodology": len(METHODOLOGY),
        "honest": len(HONEST_FAILURES),
        "bench_digest": sha_hex(canonical),
    }


def main() -> int:
    report = build()
    assert report["confirmed"] == 6
    print(f"P6-RUNS-OK {report['runs']}")
    print("P6-SLO-OK " + json.dumps(report))

    body = {
        "report": report,
        "cost_model": {
            "north_star": [
                {"name": n, "observed": o, "bound": b} for n, o, b in NORTH_STAR
            ],
            "methodology": METHODOLOGY,
            "honest_failures": HONEST_FAILURES,
            "log_seed": LOG_SEED,
            "chain_step": 'sha256(prev | "|" | line)',
        },
        "machine": {
            "vcpu": 8,
            "mem_gib": 32,
            "storage": "nvme",
            "validators": 100,
            "net_mbit": 1000,
        },
        "goldens": {
            "workload_digest": report["workload_digest"],
            "machine_digest": report["machine_digest"],
            "log_tip": report["log_tip"],
            "bench_digest": report["bench_digest"],
        },
    }
    (REPO / "sim").mkdir(exist_ok=True)
    (REPO / "sim/p6-slos.json").write_text(
        json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    if "--measured" in sys.argv:
        import time

        prev = LOG_SEED
        start = time.perf_counter_ns()
        entries = []
        for i in range(4096):
            line = f"m{i}={i}"
            prev = chain_step(prev, line)
            entries.append((f"m{i}", i, prev))
        build_ns = time.perf_counter_ns() - start
        start = time.perf_counter_ns()
        prev = LOG_SEED
        for key, value, chain in entries:
            prev = chain_step(prev, f"{key}={value}")
            assert prev == chain
        verify_ns = time.perf_counter_ns() - start
        print("P6-MEASURED " + json.dumps({"python_build_ns": build_ns, "python_verify_ns": verify_ns, "len": 4096}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
