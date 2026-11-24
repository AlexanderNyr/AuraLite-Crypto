# P0 Report — Baseline bench

Date: 2026-11-23

The result: **ALL P0 GATES GREEN**

The DoD is met literally: **5 repeat runs: spread < 5%** —
the deterministic plan runs five times byte-for-byte (spread 0%,
per-run 128 cost units, total 640, p50/p95/p99 = 128), the report digest
`cfc46920…20ec` is pinned before the first run. **NC: substituted
workload -> different numbers** — a functional gate: a 352-byte
substituted workload yields 129 != 128 units and a different digest,
`validate` rejects it with a named message; the `bad-workload` mutation in
the NC suite is red too.

The `crates/aura-bench` model (a new crate): D-P1 fixed hardware
(8 vCPU / 32 GiB / NVMe / 100 validators / 1 Gbit/s) and a workload
(350-byte transfer, 1 input/1 output) with canonical digests;
a baseline cost model (decode 4+bytes/16, verify 38/input, execute
10+15/output, commit 40); nearest-rank p50/p95/p99; an integer
spread; `validate` — the gate check in a fixed order
(runs -> determinism -> spread -> workload -> machine -> published
metrics). 8 tests with metamorphic properties; example <-> `tools/p0_bench_check.py`
byte-for-byte (`P0-RUNS-OK 5` / `P0-BENCH-OK {…}`). The honest
wall-clock layer `--measured` (a real sha256 + ed25519 pipeline
verify, ns/tx) — only in the evidence (D-P9), never in the gate:
"the first numbers, even bad ones". Negative controls `tools/nc_p0.py`
— 6 controls (`nc.log`, RESULT: 6/6 negative controls PASS).
Sources: criterion.rs (warm-up -> sample -> statistics,
deterministic inputs), doi 10.3390/app16199631 (P50/P95
inclusion latency), openchainbench.com (identical inputs, honest
aggregates, auditable runs, no cherry-picking) — see
`docs/perf/p0-baseline-report.md`. Decisions D-P9/D-P9b.
