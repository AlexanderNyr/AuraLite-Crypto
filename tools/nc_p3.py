#!/usr/bin/env python3
"""P3 storage negative-control mutations (PERF_PLAN P3 DoD:
"p99 commit latency < SLO; NC: fsync in the wrong place -> S7 red").

Kept OUTSIDE the repo source (tools-only) so nothing can compile
against the mutated text. Each mutation **reverts one P3 optimization
aspect** (six mutations, one per named message target) in exactly one
anchored source block, runs `cargo test --locked -p aura-bench --test
p3_storage` and requires a non-zero exit, then restores the exact
original bytes in a `finally` block.

The suite must fail on every mutation (red) and pass after restore
(green). The named messages asserted by the tests:

  P3: p99 commit exceeds SLO ({p99}/{slo})            <- bad-slo
  P3: fsync missing in commit path (batch {n})        <- bad-fsync (S7!)
  P3: bloom false negative (key {n})                  <- bad-bloom
  P3: bloom miss cost regression ({before}/{after})   <- bad-bloom-cost
  P3: compaction lost data                            <- bad-compaction
  P3: determinism violated (run {run})                <- bad-determinism
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MODEL = ROOT / "crates" / "aura-bench" / "src" / "p3.rs"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p3_storage"]

MUTATIONS = [
    # Revert batch writes: the block commits write by write again (an
    # fsync per write) -> p99 back above the published SLO.
    (
        "bad-slo",
        MODEL,
        "    let batch = BLOCK_WRITES as u64 * (wal_put(VALUE_BYTES) + MEMTABLE_PUT);",
        "    let batch = BLOCK_WRITES as u64 * (wal_put(VALUE_BYTES) + FSYNC + MEMTABLE_PUT);",
    ),
    # Move fsync out of the commit path (the S7 contract): a crash
    # after a "committed" batch loses it.
    (
        "bad-fsync",
        MODEL,
        "        self.sync_wal();",
        "        let _ = &self.wal;",
    ),
    # Revert the full bloom insert: only k-1 of k bits are set ->
    # false negatives for inserted keys.
    (
        "bad-bloom",
        MODEL,
        "        for p in self.probes(key) {",
        "        for p in self.probes(key).into_iter().take(self.k - 1) {",
    ),
    # Revert the bloom win on the lookup path: the probe costs as much
    # as the SSTable read it should avoid.
    (
        "bad-bloom-cost",
        MODEL,
        "pub const BLOOM_CHECK: u64 = 1;",
        "pub const BLOOM_CHECK: u64 = 41;",
    ),
    # Revert the compaction merge: run data is dropped when merging
    # multiple runs.
    (
        "bad-compaction",
        MODEL,
        "            merged.extend(run.iter().map(|(k, v)| (*k, v.clone())));\n        }\n        // The memtable is newer than every run.",
        "            if self.runs.len() < 2 { merged.extend(run.iter().map(|(k, v)| (*k, v.clone()))); }\n        }\n        // The memtable is newer than every run.",
    ),
    # Break determinism: one repeat run differs.
    (
        "bad-determinism",
        MODEL,
        "        let run_costs = vec![after; runs];",
        "        let mut run_costs = vec![after; runs]; run_costs[2] += 1;",
    ),
]


def run(name: str, path: pathlib.Path, old: str, new: str) -> bool:
    original = path.read_text(encoding="utf-8")
    if original.count(old) != 1:
        print(f"NC-RED [{name}] anchor not unique in {path.name}: FAIL")
        return False
    try:
        path.write_text(original.replace(old, new), encoding="utf-8")
        proc = subprocess.run(TEST_CMD, capture_output=True, text=True, cwd=ROOT)
        red = proc.returncode != 0
        print(f"NC-RED [{name}] {'ok' if red else 'FAIL (tests still green!)'}")
        return red
    finally:
        path.write_text(original, encoding="utf-8")


AUDIT_LINES = {
    "bad-slo": "NC-1 batch writes reverted (fsync per write): red with named message (bad-slo)",
    "bad-fsync": "NC-2 fsync moved out of the commit path: red with named message (bad-fsync)",
    "bad-bloom": "NC-3 bloom insert reverted to k-1 bits: red with named message (bad-bloom)",
    "bad-bloom-cost": "NC-4 bloom lookup win reverted: red with named message (bad-bloom-cost)",
    "bad-compaction": "NC-5 compaction merge reverted (data dropped): red with named message (bad-compaction)",
    "bad-determinism": "NC-6 determinism broken: red with named message (bad-determinism)",
}


def main() -> int:
    print("[evidence_p3] cmd=tools/nc_p3.py")
    failures = 0
    red_names = []
    for name, path, old, new in MUTATIONS:
        if run(name, path, old, new):
            red_names.append(name)
        else:
            failures += 1
    proc = subprocess.run(TEST_CMD, capture_output=True, text=True, cwd=ROOT)
    green = proc.returncode == 0
    print(f"NC-GREEN restore ok ({len(MUTATIONS) - failures}/{len(MUTATIONS)} red as required)"
          if green else "NC-GREEN restore: FAIL (tests still red)")
    for name, _, _, _ in MUTATIONS:
        if name in red_names:
            print(f"[PASS] {AUDIT_LINES[name]}")
        else:
            print(f"[FAIL] {AUDIT_LINES[name]} (tests stayed green)")
    if failures == 0 and green:
        print(f"RESULT: {len(MUTATIONS)}/{len(MUTATIONS)} negative controls PASS")
    return 0 if failures == 0 and green else 1


if __name__ == "__main__":
    sys.exit(main())
