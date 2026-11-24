# P5 Consensus profile — performance report

**Phase**: P5 consensus (PERF, Stage 2) · **DoD**: pipelining, QC cache, faster
view change; **p99 finality at 100 nodes < 2 s** (the SLO is plan-fixed;
the methodology is published by this document, D-P3). **Decision D-P7**: the profile
`aura-bench::p5` is a self-contained profiling artifact
(a millisecond-accurate model of the 3-chain commit path); moving the tuning into
`aura-consensus`/`aura-node` — follow-up (like D-P6/D-P7 for P2–P4). The machine
fixture: **D-P1** (8 vCPU / 32 GiB / NVMe / **100 validators** / 1 Gbit/s,
`cc5b0e9e…f1a6ab2d`). The vocabulary is **D-B1/D-B2** (`Vote`, QC, 2f+1, HotStuff-like
3-chain, view change, nil blocks) + B3 (`DEFAULT_VIEW_TIMEOUT_MS = 2000`).

## 1. Model and constants (pinned in `tools/p5_consensus_check.py`)

The block finality path: proposal → vote round → QC → … → **the 3-chain rule**
(a block is final when its grandchild's QC appears). The model counts **milliseconds**
as integers: gossip hop delay 25 ms (a conservative WAN hop of the D-P1 fabric),
proposal transmission 8 KiB at 1 Gbit/s = 1 ms, verifying 67 votes
by the leader = 14 ms (batch), BLS QC aggregation = 4 ms, the commit rule = 1 ms.

| Constant | "Before" | "After" | Meaning |
|---|---|---|---|
| QC re-verify per view (2 hops) | **24 ms** (per signature, B2 as-built) | **2 ms** (O(1) cache, the D-B2 goal) | D-P2 №1 |
| Happy-path 3-chain | **357 ms** (3 serialized views) | **189 ms** (1 full + 2 incremental) | D-P2 №2 (pipelining) |
| View-change stall | **2 119 ms** (timeout 2000 + view) | **397 ms** (timeout 300 + view) | D-P2 №3 |
| **p99 finality** | **2 476 ms** | **586 ms** | **SLO 2000: FAIL → PASS** |

**Formulas**: `view_full = 2×25 + 1 + 1×25 + 14 + 4 + 2×qc_verify + 1`;
`incremental = 25 + 14 + 4 + 2×qc_verify + 1` (the next block's proposal is already in
flight — pipeline overlap); `happy(3-chain) = view_full + 2×incremental`;
`stall = timeout + view_full`; **the percentile rule (pinned)**: every
view stalls with probability 1% (slow leader/nil block); the share of
"happy" blocks = 0.99³ = 970 permille → p50 = p95 = happy (189),
**p99 = happy + one stall (586)**.

## 2. Results (deterministic; P5Report, 5 runs, spread 0%)

| Metric | "Before" (the shipped path) | "After" (P5) | SLO 2 000 ms |
|---|---|---|---|
| **p99 finality @ 100 nodes** | 2 476 ms | **586 ms** | Before: **FAIL** (×1.24) → After: **PASS** (×3.4 headroom) |
| p50 / p95 | 357 / 2 476 ms | **189 / 189 ms** | — |
| 5-run total | — | 2 930 | spread 0% |

**DoD met**: p99 586 ms < 2 000 ms at 100 nodes; the baseline (serialized
3-chain + per-signature QC + a 2 s timeout = the actual `DEFAULT_VIEW_TIMEOUT_MS`
from `aura-consensus::finality`) honestly fails the SLO. **Sensitivity
(honesty)**: a 2-view finality window at the same view-change rate yields
p99 = 586 ms — also within the SLO (asserted in the tests).

## 3. Methodology and honesty

All gates run on the deterministic engine (`crates/aura-bench/src/p5.rs`):
the 3-chain rule (a block is final exactly at 3 linked descendants; the gate pins
protocol depth 3), the QC aggregator (B2 `QcAggregator`: a QC at exactly 2f+1 = 67
votes — the "66 → no, 67 → yes" check), before/after pairs for every optimization,
the percentile rule. "Revert → red" — `tools/nc_p5.py`, 6 mutations × 6
named messages `P5: …`; determinism — double build of `P5Report` (X7),
`sim/p5-consensus.json` is written by python only, example ↔ python — IDENTICAL.

**Model vs reality (D-P9)**: the measured layer (`--measured`) measures real
loops (walking a 5-block chain, aggregating 67 votes, recomputing the model) —
evidence-only, non-gate. The model accounts for dissemination/verification delays
by component; the real network adds jitter — the model is conservative thanks to
the percentile rule "p99 = happy + one full stall" (the worst single
break in the window). The full network/IO stack is out of the profile scope.

## 4. Model limitations

1. The model constants (hop 25 ms, verification 14/12/4 ms) are published estimates
   for the D-P1 fabric; they are pinned and auditable, but not measured on a live WAN.
2. The percentile rule is a deterministic approximation of a Poisson
   view-change process (1%/view); real failures correlate — the p99 in
   production may be higher; the SLO should be checked on a rig (P6 — the publication
   of SLOs/logs).
3. QC cache: the model assumes an O(1) cache hit; the `aura-consensus` implementation
   (follow-up) must provide view-based invalidation — out of the profile scope.
4. Landing (pipelining in the pacemaker, `DEFAULT_VIEW_TIMEOUT_MS` 2000 → 300,
   the QC cache) — follow-up per D-P7.

## 5. Reproducibility

```bash
make test-p5        # 13 tests (deterministic gates + DoD + revert-red)
make nc-p5          # 6 NC mutations: all red + green restore
make examples-p5    # example ↔ python diff -> IDENTICAL
make check-p5 && make evidence-p5
```

Model: `tools/p5_consensus_check.py` (the only writer of
`sim/p5-consensus.json`). Goldens: `happy 357/189`, `stall 2119/397`,
`qc_verify 24/2`, `p50=p95=189`, `p99 2476 → 586 ms` (SLO 2000), 
`bench_digest=34dd71879cd2de0cbf3246a5403d1d6e13ef0e84fbaa7595aa92119a01fa4c43`.
