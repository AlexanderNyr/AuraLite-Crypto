# SLO dashboard AuraLite (PERF, P0–P6)

**Updated:** 2026-11-24 · **Status:** all phases closed (97 phases, 650 NC)

## North stars (confirmed 6/6)

| SLO | Value | Bound | Status | Report |
| --- | ---: | ---: | --- | --- |
| p0: spread of 5 runs | 0% | ≤ 5% | ✅ | [p0-baseline-report.md](p0-baseline-report.md) |
| p1: BLS batch verification (k=8) | 22 units | ≤ 38 units (serialized) | ✅ | [p1-crypto-report.md](p1-crypto-report.md) |
| p2: parallel engine (1000 tx) | 15 543 units | ≤ 99 000 (sequential) | ✅ | [p2-parallel-report.md](p2-parallel-report.md) |
| p3: p99 block commit (50k tx) | 12 300 units | ≤ 15 000 units | ✅ | [p3-storage-report.md](p3-storage-report.md) |
| p4: gossip traffic/validator | 86 495 B/s | ≤ 125 000 B/s | ✅ | [p4-network-report.md](p4-network-report.md) |
| p5: p99 finality @100 nodes | 586 ms | ≤ 2000 ms | ✅ | [p5-consensus-report.md](p5-consensus-report.md) |

## Methodology and logs

- Methodology: 6 sections (`pN-methodology`) — in
  [p6-slo-report.md](p6-slo-report.md).
- Run log: hash chain (tip
  `ac0f9067…e52a`), rigged run → `P6: rigged run detected`.
- Honest "did not work" items: 4 entries (`p2-measured-wall-clock`,
  `p3-measured-sandbox-fsync`, `p1-bls-model-only`, `p2-lcg-fixture`).

## Quality

| Measure | Value |
| --- | --- |
| Binaries in the benchmarks | 81 |
| Lines of code (rust+python+make+patch) | 10 212 |
| Automated tests | 163 (all green) |
| Negative controls | 650 (all red, named) |
| Flakes | 0 |

## Machine profiles

The reference profile (D-P1, `MachineProfile::REFERENCE`): 8 vCPU,
32 GiB, NVMe, 100 validators, 1 Gbit/s — for all numbers above.
Synthetic perf-containers are flagged in the reports as honest
limitations (`p0/p2/p3-measured-*`).
