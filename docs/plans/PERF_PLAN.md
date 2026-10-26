# PERF_PLAN — honest measurements and SLOs

**Status:** 📋 PLAN (P0–P6)  
**Stage:** Stage 1: P0–P3. ⏸ Stage 2: P4–P6 (network, consensus, published SLOs).  
**Deliverable:** `patches/0096-P0-...` … `patches/0102-P6-...`
**Dependencies:** CORE (bench rig from C9), NODE D6 (metrics), NET N8 (stand).

## Problem

"10 000 TPS" in a README usually means "on a laptop, with empty txs, one node,
no consensus". Here a number appears only as the result of a measured run with
fixed hardware, workload and logs. Like PERF P4 in AuraLite-Emulator, a
negative result is published too (⛔ NO-GO).

## Decisions

- **D-P1.** One workload: 350-byte transfer, 1 input/1 output, 8 vCPU/32 GiB, NVMe, 100 validators, 1 Gbit/s, p50/p95/p99.
- **D-P2.** Profile before optimizing; each optimization = bench before/after + NC + determinism check (X7).
- **D-P3.** SLOs are published together with the methodology, not as a bare number.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| **P0** | Baseline bench | Criterion + harness, hardware fixed, first numbers (even bad ones) | 5 repeat runs: spread < 5%; NC: substituted workload → different numbers |
| **P1** | Codec/crypto | batch signature verification, BLS aggregation, zero-copy decode | +X% verify/s with the same determinism; NC: revert → red |
| **P2** | Parallel execution (stretch) | optimistic MVCC / Block-STM-like scheme for independent txs | 1000 txs: parallel result == sequential; conflicts roll back honestly |
| **P3** | Storage | RocksDB tuning, bloom filters, compaction, batch writes | p99 commit latency < SLO; NC: fsync in the wrong place → S7 red |
| ⏸ **P4** | Network | vote batching, compression, gossipsub parameters, mesh tuning | 100 nodes: traffic/validator within budget |
| ⏸ **P5** | Consensus | pipelining, QC cache, faster view change | p99 finality at 100 nodes < 2 s |
| ⏸ **P6** | Publish SLOs | methodology, logs, dashboards, honest "did not work" items | north star metrics confirmed; NC: rigged run → log check red |

## Risks

- Optimization breaking determinism (P2) — the X7 gate on every change.
- Benchmark on an unrepresentative workload — D-P1, D-P3.
- Chasing TPS at the cost of decentralization (validator hardware requirements grow) — a separate "minimum validator hardware" metric in O6.

## Non-goals

Marketing comparisons with other networks, "theoretical maximums".

## Closure criteria

Stage 1: a reproducible bench rig and P0–P3 profiles with honest raw numbers.
Stage 2 (⏸): P4–P6 — published TPS/finality/latency SLOs with methodology,
hardware and logs; failed experiments are marked ⛔ NO-GO.
