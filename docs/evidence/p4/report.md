# P4 Report — Network

Date: 2026-11-24

**Phase**: P4 network (PERF, Stage 2) · **DoD**: vote batching, compression,
gossipsub parameters, mesh tuning; **100 nodes: traffic/validator within
budget** (the budget is published with its methodology — this document, D-P3). **Decision
D-P7**: the `aura-bench::p4` profile is a self-contained profiling artifact
(a byte-accurate model of the consensus-gossip path); landing the tuning in
`aura-p2p`/the node config — follow-up (like D-P6/D-P7 for P2/P3). The machine
fixture: **D-P1** (8 vCPU / 32 GiB / NVMe / **100 validators** / 1 Gbit/s,
`cc5b0e9e…f1a6ab2d`). The vocabulary is gossipsub v1.0 (D, D_low, D_high, heartbeat,
`GossipFactor`) + N2 (`vote`, `proposal`, `QC`).

## 1. Model and constants (pinned in `tools/p4_network_check.py`)

The stand: **100 validators, 1 view/s** (bytes per view == bytes/s). A view consists of:
100 votes + 1 proposal (8 KiB block + 256 B header) + 1 QC. Every
node relays each message to `D` mesh neighbors (first-hop accounting:
`per_validator = D × (messages + overhead)`), IHAVE/IWANT advertisement overhead =
`GossipFactor × bytes × 2 hops`.

| Constant | "Before" | "After" | Meaning |
|---|---|---|---|
| message envelope | 64 B | 64 B | topic + seqno + from + sig |
| vote (payload) | 128 B | 128 B | height+round+hash+sig |
| Vote batching | 1 envelope/vote | **k=8** (13 envelopes/100) | D-P2 №1 |
| Proposal | 8 512 B raw | **592 B** (RLE, runs×32) | D-P2 №2 |
| `GossipFactor` | **25%** (rust-libp2p default) | **10%** | D-P2 №3 |
| Mesh D / D_low / D_high | 8 / — / — | **5 / 3 / 8** (heartbeat 700 ms, mcache 5) | D-P2 №4 |
| Overhead (IHAVE/IWANT) | 13 952 B/view | **2 883 B/view** | pair №3 |
| **Traffic/validator** | **334 848 B/s** | **86 495 B/s** | **−74%** |

**Formulas**: `msg = votes_wire + proposal_wire + qc(192)`; 
`overhead = factor × msg × 2 / 100` (integer); `per_validator = D × (msg + overhead)`.

## 2. Budget (D-P3, methodology)

**Budget: consensus-gossip traffic ≤ 125 000 B/s per validator (1 Mbit/s
egress) at 1 view/s.** Rationale: 1 Mbit/s = 10% of a validator's share of the
D-P1 fabric (1 Gbit/s / 100 validators = 10 Mbit/s), leaving headroom for sync
and tx-gossip; the budget fits the Raspberry/phone validator class
(10 Mbit/s uplink, consensus share ≤ 10%). Scope: consensus-gossip egress
(topics `vote`/`proposal`/`QC`), not sync and not tx flooding.

| Metric | "Before" | "After" | Budget 125 000 |
|---|---|---|---|
| **Traffic/validator** | 334 848 B/s (2.68 Mbit/s) | **86 495 B/s (0.69 Mbit/s)** | Before: **FAIL** (×2.7) → After: **PASS** (31% headroom) |
| Votes (wire) | 19 200 | 13 632 | −29% |
| Proposal (wire) | 8 512 | 592 | −93% (RLE fixture) |
| Overhead | 13 952 | 2 883 | −79% |
| 5-run total | — | 432 475 | spread 0% |

**DoD met**: 100 nodes, traffic/validator 86 495 ≤ 125 000; the baseline honestly
fails the budget (asserted in the tests). **Sensitivity (honesty)**: if
real blocks compress only **2:1** (conservative deflate instead of the RLE-
fixture), the traffic is **108 860 B/s** — still within budget (asserted in the tests
`p4_sensitivity_two_to_one_fits`).

## 3. Methodology and honesty

All gates run on the deterministic engine (`crates/aura-bench/src/p4.rs`):
the RLE codec (lossless roundtrip — a functional gate), batching (unpack == the original
vote order), parameters (spec invariants: D_low ≤ D ≤ D_high,
heartbeat ≤ 1000 ms, mcache ≥ 3, factor_after < factor_before), mesh (ring +
chords as in N2 `GossipMesh`; BFS connectivity on 100 nodes). "Revert → red" —
`tools/nc_p4.py`, 6 mutations × 6 named messages `P4: …`; determinism —
double build of `P4Report` (X7), `sim/p4-network.json` is written by python only,
example ↔ python — IDENTICAL.

**Model vs reality (D-P9)**: the measured layer (`--measured`) measures the real
RLE and batching loops (encoding 8 KiB ≈ 7.6 µs) — evidence-only, non-gate.
The model counts only the first relay hop (every node sends to D neighbors);
real gossipsub with IHAVE/IWANT retransmissions yields more traffic — the model
approximates it with the overhead term (GossipFactor × 2 hops) and so the estimates
are conservative in the budget part for the declared terms only. The full
networking stack (libp2p handshake, multiplex, keepalive) is out of the profile scope.

## 4. Model limitations

1. bytes/view == bytes/s at 1 view/s; pipelining (P5) raises the view rate —
   the budget scales linearly (all terms are proportional to the view count).
2. The RLE fixture (runs × 32) compresses better than real blocks; we publish a
   sensitivity 2:1 (108 860 < 125 000).
3. Relay accounting is "every node → D neighbors"; echoes/duplicates are killed
   by the SeenCache (N2) and are not modeled.
4. Landing the parameters (`mesh_n=5`, `gossip_factor=0.10`, mcache/heartbeat) in
   `aura-p2p` — follow-up per D-P7.

## 5. Reproducibility

```bash
make test-p4        # 12 tests (deterministic gates + DoD + revert-red)
make nc-p4          # 6 NC mutations: all red + green restore
make examples-p4    # example ↔ python diff -> IDENTICAL
make check-p4 && make evidence-p4
```

Model: `tools/p4_network_check.py` (the only writer of `sim/p4-network.json`).
Goldens: `before=334848, after=86495, budget=125000, reduction=74%`,
pairs `votes 19200/13632`, `proposal 8512/592`, `overhead 13952/2883`, `mesh 8/5`,
sensitivity 2:1 = 108 860; 
`bench_digest=ab618903bda0b4dc15705a228a0cd9992e6f76ecf0c20bfe8bf8463281ce1108`.
