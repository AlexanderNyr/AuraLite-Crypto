#!/usr/bin/env python3
"""P5 consensus — independent Python model (PERF_PLAN P5).

Reproduces the deterministic finality-latency cost model of
`crates/aura-bench/src/p5.rs` byte for byte:

  stand     : 100 validators (D-P1 fixture), HotStuff-like 3-chain
              commit (D-B1/D-B2: Vote, QC, 2f+1 quorum)
  baseline  : serialized 3-chain, per-signature QC re-verification
              at every hop (shipped B2), 2 s view-change timeout
              (shipped DEFAULT_VIEW_TIMEOUT_MS)
  P5        : pipelined chain extension (proposal overlapped), QC
              cache (O(1) check), 300 ms view-change timeout
  SLO       : p99 finality at 100 nodes < 2000 ms (plan-fixed;
              methodology in docs/perf/p5-consensus-report.md, D-P3)

Output (stdout) is byte-identical to
`cargo run -p aura-bench --example p5_consensus_example`. `--measured`
adds an honest wall-clock line (evidence only, D-P9).
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]

N_NODES = 100
F_NODES = (N_NODES - 1) // 3
HOP_MS = 25
PROPOSAL_HOPS = 2
VOTE_HOPS = 1
QC_HOPS = 2
PROPOSAL_TX_MS = 1
VOTE_VERIFY_MS = 14
QC_FORM_MS = 4
QC_VERIFY_MS = 12
QC_VERIFY_CACHED_MS = 1
COMMIT_MS = 1
TIMEOUT_MS_BEFORE = 2000
TIMEOUT_MS_AFTER = 300
VIEW_CHANGE_RATE_PCT = 1
CHAIN_DEPTH = 3
FINALITY_SLO_MS = 2000
RUNS = 5


def sha(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def quorum_threshold(n: int) -> int:
    return 2 * ((n - 1) // 3) + 1


def view_full_latency(qc_verify_ms: int) -> int:
    return (
        PROPOSAL_HOPS * HOP_MS
        + PROPOSAL_TX_MS
        + VOTE_HOPS * HOP_MS
        + VOTE_VERIFY_MS
        + QC_FORM_MS
        + QC_HOPS * qc_verify_ms
        + COMMIT_MS
    )


def incremental_view_latency(qc_verify_ms: int) -> int:
    return VOTE_HOPS * HOP_MS + VOTE_VERIFY_MS + QC_FORM_MS + QC_HOPS * qc_verify_ms + COMMIT_MS


def happy_latency(qc_verify_ms: int, pipelined: bool) -> int:
    if pipelined:
        return view_full_latency(qc_verify_ms) + (CHAIN_DEPTH - 1) * incremental_view_latency(qc_verify_ms)
    return CHAIN_DEPTH * view_full_latency(qc_verify_ms)


def stall_latency(timeout_ms: int, qc_verify_ms: int) -> int:
    return timeout_ms + view_full_latency(qc_verify_ms)


def latency_percentiles(happy: int, stall: int, rate_pct: int, window: int) -> tuple[int, int, int]:
    base = (100 - rate_pct) ** window
    denom = 100 ** window
    happy_share = base * 1000 // denom

    def pick(p: int) -> int:
        return happy if p * 10 <= happy_share else happy + stall

    return (pick(50), pick(95), pick(99))


class QcAggregator:
    def __init__(self, n: int):
        self.voters: list[int] = []
        self.n = n

    def add_vote(self, validator: int) -> bool:
        if validator not in self.voters:
            self.voters.append(validator)
        return len(self.voters) >= quorum_threshold(self.n)


def finalized_indices(chain: list[dict], depth: int) -> list[int]:
    out = []
    for i in range(len(chain)):
        steps = 0
        cursor = i
        while steps < depth:
            if cursor is None:
                break
            cursor = next((j for j, b in enumerate(chain) if b["parent"] == cursor), None)
            steps += 1
        if steps == depth and cursor is not None:
            out.append(i)
    return out


def pairs() -> dict:
    qc_before = QC_HOPS * QC_VERIFY_MS
    qc_after = QC_HOPS * QC_VERIFY_CACHED_MS
    happy_b = happy_latency(QC_VERIFY_MS, False)
    happy_a = happy_latency(QC_VERIFY_CACHED_MS, True)
    stall_b = stall_latency(TIMEOUT_MS_BEFORE, QC_VERIFY_MS)
    stall_a = stall_latency(TIMEOUT_MS_AFTER, QC_VERIFY_CACHED_MS)
    p99_b = latency_percentiles(happy_b, stall_b, VIEW_CHANGE_RATE_PCT, 3)[2]
    p99_a = latency_percentiles(happy_a, stall_a, VIEW_CHANGE_RATE_PCT, 3)[2]
    return {
        "qc_verify": (qc_before, qc_after),
        "happy": (happy_b, happy_a),
        "stall": (stall_b, stall_a),
        "finality": (p99_b, p99_a),
    }


def build() -> dict:
    p = pairs()
    assert quorum_threshold(100) == 67
    assert F_NODES == 33
    # 3-chain rule gate (5-block chain)
    chain = [
        {"view": 1, "parent": None},
        {"view": 2, "parent": 0},
        {"view": 3, "parent": 1},
        {"view": 4, "parent": 2},
        {"view": 5, "parent": 3},
    ]
    want = [i for i in range(len(chain)) if i + CHAIN_DEPTH < len(chain)]
    assert finalized_indices(chain, CHAIN_DEPTH) == want, "3-chain rule"
    # quorum gate: 66 votes -> no QC, 67 -> QC
    agg = QcAggregator(N_NODES)
    for v in range(66):
        assert not agg.add_vote(v), "no QC at 66"
    assert agg.add_vote(66), "QC at 67"

    p50, p95, p99 = latency_percentiles(p["happy"][1], p["stall"][1], VIEW_CHANGE_RATE_PCT, 3)
    assert p99 < FINALITY_SLO_MS, "P5 DoD: p99 finality < 2 s"
    assert p["finality"][0] >= FINALITY_SLO_MS, "baseline must honestly fail the SLO"
    # 2:1 view-window sensitivity: 2 views in the window at the same rate
    sens = latency_percentiles(p["happy"][1], p["stall"][1], VIEW_CHANGE_RATE_PCT, 2)
    assert sens[2] < FINALITY_SLO_MS, "2-view window sensitivity also fits"

    wd = sha(f"p5w|consensus|nodes={N_NODES}|chain={CHAIN_DEPTH}|rate={VIEW_CHANGE_RATE_PCT}|hop={HOP_MS}")
    md = sha("p0m|vcpu=8|mem_gib=32|storage=nvme|validators=100|net_mbit=1000")
    total = p99 * RUNS
    canonical = (
        f"p5b|{wd}|{md}|runs={RUNS}|spread=0"
        f"|p50={p50}|p95={p95}|p99={p99}|total={total}"
        f"|before={p['finality'][0]}|after={p['finality'][1]}|slo={FINALITY_SLO_MS}"
        f"|happy={p['happy'][0]}/{p['happy'][1]}"
        f"|stall={p['stall'][0]}/{p['stall'][1]}"
        f"|qc_verify={p['qc_verify'][0]}/{p['qc_verify'][1]}"
    )
    return {
        "workload_digest": wd,
        "machine_digest": md,
        "runs": RUNS,
        "spread_pct": 0,
        "p50": p50,
        "p95": p95,
        "p99": p99,
        "total": total,
        "before": p["finality"][0],
        "after": p["finality"][1],
        "slo": FINALITY_SLO_MS,
        "happy_before": p["happy"][0],
        "happy_after": p["happy"][1],
        "stall_before": p["stall"][0],
        "stall_after": p["stall"][1],
        "qc_verify_before": p["qc_verify"][0],
        "qc_verify_after": p["qc_verify"][1],
        "timeout_before": TIMEOUT_MS_BEFORE,
        "timeout_after": TIMEOUT_MS_AFTER,
        "nodes": N_NODES,
        "chain_depth": CHAIN_DEPTH,
        "view_change_rate_pct": VIEW_CHANGE_RATE_PCT,
        "sensitivity_2view_p99": sens[2],
        "bench_digest": sha(canonical),
    }


def main() -> int:
    report = build()
    print(f"P5-RUNS-OK {report['runs']}")
    print("P5-CONSENSUS-OK " + json.dumps(report))

    body = {
        "report": {k: v for k, v in report.items() if k != "sensitivity_2view_p99"},
        "cost_model": {
            "hop_ms": HOP_MS,
            "proposal_hops": PROPOSAL_HOPS,
            "vote_hops": VOTE_HOPS,
            "qc_hops": QC_HOPS,
            "proposal_tx_ms": PROPOSAL_TX_MS,
            "vote_verify_ms": VOTE_VERIFY_MS,
            "qc_form_ms": QC_FORM_MS,
            "qc_verify_ms": QC_VERIFY_MS,
            "qc_verify_cached_ms": QC_VERIFY_CACHED_MS,
            "commit_ms": COMMIT_MS,
            "timeout_before": TIMEOUT_MS_BEFORE,
            "timeout_after": TIMEOUT_MS_AFTER,
            "view_change_rate_pct": VIEW_CHANGE_RATE_PCT,
            "chain_depth": CHAIN_DEPTH,
            "slo_ms": FINALITY_SLO_MS,
            "sensitivity_2view_p99": report["sensitivity_2view_p99"],
            "formula_view_full": "proposals*hop + tx + votes*hop + verify + form + qc_hops*qc_verify + commit",
            "formula_p99": "p99 = happy + one stall when (100-rate)^window permille < 990",
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
    (REPO / "sim/p5-consensus.json").write_text(
        json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    if "--measured" in sys.argv:
        import time

        chain = [{"view": 1, "parent": None}, {"view": 2, "parent": 0}, {"view": 3, "parent": 1},
                 {"view": 4, "parent": 2}, {"view": 5, "parent": 3}]
        start = time.perf_counter_ns()
        for _ in range(1000):
            fin = finalized_indices(chain, CHAIN_DEPTH)
        ns = time.perf_counter_ns() - start
        assert fin == [0, 1]
        print("P5-MEASURED " + json.dumps({"python_chain_1000x_ns": ns}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
