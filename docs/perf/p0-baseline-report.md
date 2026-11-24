# P0 — Baseline bench (phase report)

Date: 2026-11-23. Phase patch: `patches/0093-P0-Baseline-bench.patch`.
Report digest
`cfc469203166bce7439bf4bec51a9ba545e8b6c3df2edcb27cd8ff41dce220ec`.

## DoD and criteria (PERF_PLAN P0)

| Criterion | Status |
|---|---|
| **5 repeat runs: spread < 5%** | ✅ The deterministic plan runs 5 times byte-for-byte: spread **0%** (per-run 128 cost units, total 640, p50/p95/p99 = 128). The golden digest is pinned BEFORE the first run (methodology: the plan is fixed in advance) |
| **NC: substituted workload → different numbers** | ✅ Functional gate: a substituted workload (352 B instead of 350) yields **129 ≠ 128** units and a different digest; `validate` rejects it with the named message `P0: workload mismatch (…)`. Plus the `bad-workload` mutation in the NC suite |
| **Criterion + harness, hardware fixed** | ✅ `crates/aura-bench`: D-P1 machine (8 vCPU / 32 GiB / NVMe / 100 validators / 1 Gbit/s) and workload (350-byte transfer, 1 input/1 output) — constants with digests; substituting machine/workload is red |
| **first numbers (even bad ones)** | ✅ Honest wall-clock layer `--measured` (the real 350-byte pipeline sha256 + ed25519 verify, ns/tx) — evidence only, never a gate (D-P9) |

## The `crates/aura-bench` model

### Two layers, honestly separated (D-P2)

1. **Deterministic cost model — the gate.** Stage costs in
   relative cost units, fixed by the machine profile:
   `decode = 4 + bytes/16`, `verify = 38 × inputs`,
   `execute = 10 + 15 × outputs`, `commit = 40` → **128 units** per
   run for the reference workload. Percentiles are nearest-rank
   (p50/p95/p99), spread is integer `(max−min)·100/mean`.
   `BenchReport::build` is a pure function (metamorphism: two builds
   are byte-identical).
2. **Measured layer — honest numbers.** `measure_pipeline_ns` measures
   the real pipeline (sha256 of 350 B + ed25519 verify) on wall-clock;
   the `P0-MEASURED {p50_ns,…}` result goes to evidence only
   (`docs/evidence/p0/baseline.log`). "First numbers, even bad ones" —
   that is what these are; they never become gates.

### Gate control (`validate`, fixed order)

repeat count (5) → determinism (all runs equal) → spread (≤ 5%) →
workload digest → machine digest → published metrics (p50/p95/p99 ≠ 0,
D-P3: numbers are published together with the methodology). 6 named messages
(`P0: …`) — exact to_string asserts in the tests.

### Methodology (D-P3; sources below)

The bench plan (workload, machine, run count, metric set)
**is fixed before the run**; identical deterministic inputs for
every run; aggregates are p50/p95/p99 (never a bare average);
runs are reproducible and auditable (`sim/p0-bench.json`, logs);
negative results are published (the P6 convention).

## Gates and tests

`crates/aura-bench/tests/p0_bench.rs` — 8 tests: golden numbers
(the literal DoD), the functional NC "substituted workload → different
numbers", exact literals of the 6 NC targets, the validate-suite (every stage
turns red), metamorphisms (function purity, scaling of the run
count, percentiles/spread on a skewed sample, cost
breakdown). Example ↔ `tools/p0_bench_check.py` — output
`P0-RUNS-OK 5` / `P0-BENCH-OK {…}` byte-for-byte.

## Negative controls

`tools/nc_p0.py` — 6 mutations with named messages: `bad-workload`
(the DoD gate), `bad-machine`, `bad-spread`, `bad-runs`,
`bad-determinism`, `bad-metric`. **6/6 RED + GREEN.**
Cumulative: **614 NC / 32 phases**.

## Sources

- **Criterion.rs** (docs.rs criterion / rustfaq.org): the measurement
  protocol — warm-up → sample → statistics; deterministic inputs
  («random data introduces variance»); sample_size, noise threshold,
  confidence intervals, outlier detection; regressions against
  a stored baseline.
- **Consensus benchmarking** (doi 10.3390/app16199631): P50/P95
  inclusion latency, descriptive statistics (mean, CV, 95% CI).
- **OpenChainBench methodology** (openchainbench.com): identical
  inputs, honest aggregates (p50/p90/p99 + success rate; the average is
  not the headline), auditable runs, "no cherry-picking" (the plan
  before the run), "a number on its own is not a measurement".
- **Internal**: `docs/plans/PERF_PLAN.md` (D-P1..D-P3, risks),
  X7 determinism (a gate on every change), `aura-crypto`
  (ed25519 sign/verify for the measured layer), `aura-state::sha256_bytes`.

## Phase decisions

- **D-P9**: the deterministic gate proves the *harness* (identical
  inputs, fixed plan, percentile aggregation, spread
  control); wall-clock spread on shared hardware is reported but not
  gated (otherwise CI flakes).
- **D-P9b**: cost units are relative to the machine profile; P1
  optimizations change the cost model with before/after benches + NC + a
  determinism check (D-P2).
