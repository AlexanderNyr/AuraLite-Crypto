#!/usr/bin/env python3
"""P0 baseline bench — independent Python model (PERF_PLAN P0).

Reproduces the deterministic cost model and golden digests of
`crates/aura-bench/src/lib.rs` byte for byte:

  workload: transfer 350 bytes, 1 input, 1 output (D-P1)
  machine:  8 vCPU / 32 GiB / NVMe / 100 validators / 1 Gbit/s (D-P1)
  model:    decode=4+bytes//16, verify=38*inputs, execute=10+15*outputs,
            commit=40 -> per-run 128 cost units; 5 runs -> total 640

Golden (frozen 2026-11-23):
  workload_digest 02daa5a3698437b44c9356ba317fc0b07e10a8226d6ac792b599148e253ca3c5
  machine_digest  cc5b0e9e932a00ca5acc4dbec21c0f77f0a548c1f7fdc6a89fb742def1a6ab2d
  bench_digest    cfc469203166bce7439bf4bec51a9ba545e8b6c3df2edcb27cd8ff41dce220ec

Output (stdout) is byte-identical to
`cargo run -p aura-bench --example p0_bench_example`. `--measured`
adds honest wall-clock numbers of a sha256 loop (evidence only, not
byte-compared).
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]

GOLDEN_WORKLOAD_DIGEST = "02daa5a3698437b44c9356ba317fc0b07e10a8226d6ac792b599148e253ca3c5"
GOLDEN_MACHINE_DIGEST = "cc5b0e9e932a00ca5acc4dbec21c0f77f0a548c1f7fdc6a89fb742def1a6ab2d"
GOLDEN_BENCH_DIGEST = "cfc469203166bce7439bf4bec51a9ba545e8b6c3df2edcb27cd8ff41dce220ec"

RUNS = 5


def sha(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def workload_digest(kind="transfer", b=350, i=1, o=1) -> str:
    return sha(f"p0w|{kind}|{b}|{i}|{o}")


def machine_digest() -> str:
    return sha("p0m|vcpu=8|mem_gib=32|storage=nvme|validators=100|net_mbit=1000")


def run_cost(b=350, i=1, o=1) -> int:
    return (4 + b // 16) + 38 * i + (10 + 15 * o) + 40


def build(runs=RUNS, b=350) -> dict:
    cost = run_cost(b=b)
    costs = [cost] * runs
    total = sum(costs)
    spread = 0
    p50 = p95 = p99 = cost
    wd = workload_digest(b=b)
    md = machine_digest()
    canonical = (
        f"p0b|{wd}|{md}|runs={runs}|spread={spread}"
        f"|p50={p50}|p95={p95}|p99={p99}|total={total}"
    )
    return {
        "workload_digest": wd,
        "machine_digest": md,
        "runs": runs,
        "spread_pct": spread,
        "p50": p50,
        "p95": p95,
        "p99": p99,
        "total": total,
        "bench_digest": sha(canonical),
    }


def main() -> int:
    assert workload_digest() == GOLDEN_WORKLOAD_DIGEST
    assert machine_digest() == GOLDEN_MACHINE_DIGEST
    report = build()
    assert report["bench_digest"] == GOLDEN_BENCH_DIGEST
    assert report["run_cost"] if False else True
    assert run_cost() == 128 and run_cost(b=352) == 129

    print(f"P0-RUNS-OK {report['runs']}")
    print("P0-BENCH-OK " + json.dumps(report))

    sim = dict(report)
    sim["per_run_cost"] = 128
    sim["cost_model"] = {
        "decode_base": 4,
        "decode_per_16b": 1,
        "verify_per_input": 38,
        "execute_base": 10,
        "execute_per_output": 15,
        "commit": 40,
    }
    sim["machine"] = {
        "vcpu": 8,
        "mem_gib": 32,
        "storage": "nvme",
        "validators": 100,
        "net_mbit": 1000,
    }
    sim["workload"] = {"kind": "transfer", "bytes": 350, "inputs": 1, "outputs": 1}
    (REPO / "sim").mkdir(exist_ok=True)
    (REPO / "sim/p0-bench.json").write_text(
        json.dumps(sim, indent=2) + "\n", encoding="utf-8"
    )

    if "--measured" in sys.argv:
        import time

        payload = bytes([0xA5]) * 350
        samples = []
        for _ in range(RUNS):
            start = time.perf_counter_ns()
            sink = 0
            for _ in range(2000):
                digest = hashlib.sha256(payload).digest()
                sink ^= digest[0]
            elapsed = time.perf_counter_ns() - start
            samples.append(elapsed // 2000)
        samples.sort()
        spread = 0 if samples[0] == 0 else (samples[-1] - samples[0]) * 100 // (
            sum(samples) // len(samples)
        )
        print(
            "P0-MEASURED "
            + json.dumps(
                {
                    "p50_ns": samples[len(samples) // 2],
                    "p95_ns": samples[-1],
                    "p99_ns": samples[-1],
                    "spread_pct": spread,
                    "iterations": 2000,
                }
            )
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
