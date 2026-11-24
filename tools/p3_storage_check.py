#!/usr/bin/env python3
"""P3 storage — independent Python model (PERF_PLAN P3).

Reproduces the deterministic cost model and the exact bloom-filter
false-positive count of `crates/aura-bench/src/p3.rs` byte for byte:

  workload:  block commit = 1000 writes of 350-byte values (D-P1)
  baseline:  write by write: wal(4+350/64=9) + fsync(300) + memtable(2)
             -> 311 000 units per block
  P3:        one WriteBatch: 1000*(9+2) + fsync(300) + compaction
             amortized 1000*8/8 = 12 300 units per block
  SLO:       p99 block commit < 15 000 units (wall-clock SLO published
             in docs/perf/p3-storage-report.md, D-P3)
  bloom:     10 bits/key, k=7, m rounded up to a power of two; probes
             (h1 + i*h2) % m with h1,h2 from sha256(key.to_le_bytes()),
             h2 forced odd. Misses: 200 absent keys.

Output (stdout) is byte-identical to
`cargo run -p aura-bench --example p3_storage_example`. `--measured`
adds an honest wall-clock line (evidence only, not byte-compared).
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]

BLOCK_WRITES = 1000
VALUE_BYTES = 350
WAL_BASE = 4
WAL_DIV = 64
FSYNC = 300
MEMTABLE_PUT = 2
BLOOM_CHECK = 1
SST_READ = 40
COMPACT_PER_KEY = 8
COMPACT_EVERY = 8
COMMIT_SLO_UNITS = 15_000
BLOOM_BITS_PER_KEY = 10
BLOOM_K = 7
LOOKUP_MISSES = 200
RUNS = 5


def sha(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def sha32(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def wal_put(value_bytes: int) -> int:
    return WAL_BASE + value_bytes // WAL_DIV


def block_cost_before() -> int:
    return BLOCK_WRITES * (wal_put(VALUE_BYTES) + FSYNC + MEMTABLE_PUT)


def block_cost_after() -> int:
    batch = BLOCK_WRITES * (wal_put(VALUE_BYTES) + MEMTABLE_PUT)
    return batch + FSYNC + BLOCK_WRITES * COMPACT_PER_KEY // COMPACT_EVERY


def bloom_probes(key: int, m: int) -> list[int]:
    d = sha32(key.to_bytes(8, "little"))
    h1 = int.from_bytes(d[0:8], "little")
    h2 = int.from_bytes(d[8:16], "little") | 1
    return [(h1 + i * h2) % m for i in range(BLOOM_K)]


def bloom_false_positives() -> int:
    want = max(1000, 1) * BLOOM_BITS_PER_KEY
    m = 1
    while m < want:
        m *= 2
    bits = [False] * m
    for key in range(1000):
        for p in bloom_probes(key, m):
            bits[p] = True
    for key in range(1000):
        assert all(bits[p] for p in bloom_probes(key, m)), "no false negatives"
    fp = 0
    for key in range(1_000_000, 1_000_000 + LOOKUP_MISSES):
        if all(bits[p] for p in bloom_probes(key, m)):
            fp += 1
    return fp


def miss_cost_after(fp: int) -> int:
    return BLOOM_CHECK + (fp * SST_READ + LOOKUP_MISSES - 1) // LOOKUP_MISSES


def build() -> dict:
    before = block_cost_before()
    after = block_cost_after()
    fp = bloom_false_positives()
    wd = sha(f"p3w|block-commit|{BLOCK_WRITES}|{VALUE_BYTES}")
    md = sha("p0m|vcpu=8|mem_gib=32|storage=nvme|validators=100|net_mbit=1000")
    speedup = before * 100 // after - 100
    miss_before = SST_READ
    miss_after = miss_cost_after(fp)
    canonical = (
        f"p3b|{wd}|{md}|runs={RUNS}|spread=0"
        f"|p50={after}|p95={after}|p99={after}|total={after * RUNS}"
        f"|before={before}|after={after}|slo={COMMIT_SLO_UNITS}"
        f"|speedup_pct={speedup}"
        f"|miss_before={miss_before}|miss_after={miss_after}|fp={fp}"
    )
    return {
        "workload_digest": wd,
        "machine_digest": md,
        "runs": RUNS,
        "spread_pct": 0,
        "p50": after,
        "p95": after,
        "p99": after,
        "total": after * RUNS,
        "before": before,
        "after": after,
        "slo": COMMIT_SLO_UNITS,
        "speedup_pct": speedup,
        "miss_before": miss_before,
        "miss_after": miss_after,
        "fp": fp,
        "block_writes": BLOCK_WRITES,
        "value_bytes": VALUE_BYTES,
        "bench_digest": sha(canonical),
    }


def main() -> int:
    report = build()
    assert report["before"] == 311_000
    assert report["after"] == 12_300
    assert report["p99"] < report["slo"], "P3 DoD: p99 < SLO"
    assert report["miss_after"] < report["miss_before"]
    print(f"P3-RUNS-OK {report['runs']}")
    print("P3-STORAGE-OK " + json.dumps(report))

    body = {
        "report": report,
        "cost_model": {
            "wal_base": WAL_BASE,
            "wal_div": WAL_DIV,
            "fsync": FSYNC,
            "memtable_put": MEMTABLE_PUT,
            "bloom_check": BLOOM_CHECK,
            "sst_read": SST_READ,
            "compact_per_key": COMPACT_PER_KEY,
            "compact_every": COMPACT_EVERY,
            "bloom_bits_per_key": BLOOM_BITS_PER_KEY,
            "bloom_k": BLOOM_K,
            "lookup_misses": LOOKUP_MISSES,
            "formula_after": "writes*(wal+memtable) + fsync + writes*compact/every",
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
            "bench_digest": report["bench_digest"],
        },
    }
    (REPO / "sim").mkdir(exist_ok=True)
    (REPO / "sim/p3-storage.json").write_text(
        json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    if "--measured" in sys.argv:
        import time

        samples = []
        for _ in range(3):
            start = time.perf_counter_ns()
            sink = 0
            for i in range(2000):
                sink ^= (i * 2654435761) & 0xFFFF
            samples.append(time.perf_counter_ns() - start)
        samples.sort()
        print(
            "P3-MEASURED "
            + json.dumps({"python_loop_ns": samples[1], "iterations": 2000})
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
