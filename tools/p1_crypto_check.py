#!/usr/bin/env python3
"""P1 codec/crypto — independent Python model (PERF_PLAN P1).

Reproduces the deterministic cost model and golden digests of
`crates/aura-bench/src/p1.rs` byte for byte:

  workload: transfer 350 bytes, 1 input, 1 output (D-P1, unchanged)
  model:    zero-copy decode = 2 + bytes/32      (was 4 + bytes/16)
            batch verify k=8: (16 + 20k)/k = 22  (was 38 single)
            execute = 10 + 15*out, commit = 40   (unchanged)
            -> per-run 99 cost units; 5 runs -> total 495
  BLS model (D-P4, MODEL ONLY): FastAggregateVerify(k) = 2*22 + 2*k
            -> 60 for k=8 vs 352 individual (draft-irtf-cfrg-bls-signature)

Golden (frozen 2026-11-24):
  workload_digest 02daa5a3698437b44c9356ba317fc0b07e10a8226d6ac792b599148e253ca3c5
  machine_digest  cc5b0e9e932a00ca5acc4dbec21c0f77f0a548c1f7fdc6a89fb742def1a6ab2d
  p1_digest       340a884d17a5ca58111c00fcd55229f7cfce4175fcfb8c97cf69fe2f52c80241

Output (stdout) is byte-identical to
`cargo run -p aura-bench --example p1_crypto_example`. `--measured`
adds honest wall-clock numbers (evidence only, not byte-compared).
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]

GOLDEN_WORKLOAD_DIGEST = "02daa5a3698437b44c9356ba317fc0b07e10a8226d6ac792b599148e253ca3c5"
GOLDEN_MACHINE_DIGEST = "cc5b0e9e932a00ca5acc4dbec21c0f77f0a548c1f7fdc6a89fb742def1a6ab2d"
GOLDEN_P1_DIGEST = "340a884d17a5ca58111c00fcd55229f7cfce4175fcfb8c97cf69fe2f52c80241"

RUNS = 5
BATCH_K = 8


def sha(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def workload_digest() -> str:
    return sha("p0w|transfer|350|1|1")


def machine_digest() -> str:
    return sha("p0m|vcpu=8|mem_gib=32|storage=nvme|validators=100|net_mbit=1000")


def verify_after() -> int:
    return (16 + 20 * BATCH_K) // BATCH_K


def run_cost() -> int:
    decode = 2 + 350 // 32
    verify = verify_after()
    execute = 10 + 15 * 1
    return decode + verify + execute + 40


def build() -> dict:
    cost = run_cost()
    wd = workload_digest()
    md = machine_digest()
    verify_before = 38
    decode_before = 4 + 350 // 16
    decode_after = 2 + 350 // 32
    agg_cost = 2 * 22 + 2 * BATCH_K
    canonical = (
        f"p1b|{wd}|{md}|runs={RUNS}|spread=0"
        f"|p50={cost}|p95={cost}|p99={cost}|total={cost * RUNS}"
        f"|verify_before={verify_before}|verify_after={verify_after()}"
        f"|decode_before={decode_before}|decode_after={decode_after}"
        f"|agg_k={BATCH_K}|agg_cost={agg_cost}"
    )
    return {
        "workload_digest": wd,
        "machine_digest": md,
        "runs": RUNS,
        "spread_pct": 0,
        "p50": cost,
        "p95": cost,
        "p99": cost,
        "total": cost * RUNS,
        "verify_before": verify_before,
        "verify_after": verify_after(),
        "decode_before": decode_before,
        "decode_after": decode_after,
        "agg_k": BATCH_K,
        "agg_cost": agg_cost,
        "bench_digest": sha(canonical),
    }


def main() -> int:
    assert workload_digest() == GOLDEN_WORKLOAD_DIGEST
    assert machine_digest() == GOLDEN_MACHINE_DIGEST
    report = build()
    assert report["bench_digest"] == GOLDEN_P1_DIGEST
    assert run_cost() == 99 and report["total"] == 495
    assert verify_after() == 22
    assert report["agg_cost"] == 60

    speedup = report["verify_before"] * 100 // report["verify_after"] - 100
    decode_cut = (
        (report["decode_before"] - report["decode_after"])
        * 100
        // report["decode_before"]
    )
    agg_individual = 2 * 22 * BATCH_K
    assert speedup == 72 and decode_cut == 52 and agg_individual == 352

    print(f"P1-RUNS-OK {report['runs']}")
    base = json.dumps(report)
    base = base[:-1]
    print(
        "P1-CRYPTO-OK "
        + f'{base}, "verify_speedup_pct": {speedup}, "decode_cut_pct": {decode_cut}, '
        + f'"agg_individual": {agg_individual}}}'
    )

    cost_model = {
        "decode_base": 2,
        "decode_div": 32,
        "decode_base_before": 4,
        "decode_div_before": 16,
        "verify_batch_base": 16,
        "verify_batch_per": 20,
        "verify_before": 38,
        "execute_base": 10,
        "execute_per_output": 15,
        "commit": 40,
        "batch_k": BATCH_K,
        "agg_pairing": 22,
        "agg_add": 2,
    }
    machine = {
        "vcpu": 8,
        "mem_gib": 32,
        "storage": "nvme",
        "validators": 100,
        "net_mbit": 1000,
    }
    workload = {"kind": "transfer", "bytes": 350, "inputs": 1, "outputs": 1}
    goldens = {
        "workload_digest": report["workload_digest"],
        "machine_digest": report["machine_digest"],
        "p1_digest": report["bench_digest"],
    }
    body = {
        "report": report,
        "cost_model": cost_model,
        "machine": machine,
        "workload": workload,
        "verify_speedup_pct": speedup,
        "decode_cut_pct": decode_cut,
        "agg_individual": agg_individual,
        "goldens": goldens,
    }
    (REPO / "sim").mkdir(exist_ok=True)
    (REPO / "sim/p1-crypto.json").write_text(
        json.dumps(body, indent=2) + "\n", encoding="utf-8"
    )

    if "--measured" in sys.argv:
        import time

        payload = bytes([0x5A]) * 350
        samples = []
        for _ in range(RUNS):
            start = time.perf_counter_ns()
            sink = 0
            for _ in range(2000):
                digest = hashlib.sha256(payload).digest()
                sink ^= digest[0]
            samples.append(time.perf_counter_ns() - start)
        samples.sort()
        print(
            "P1-MEASURED "
            + json.dumps(
                {
                    "sha256_ns_per_op": samples[len(samples) // 2] // 2000,
                    "iterations": 2000,
                }
            )
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
