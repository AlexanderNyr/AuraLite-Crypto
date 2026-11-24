#!/usr/bin/env python3
"""P4 network — independent Python model (PERF_PLAN P4).

Reproduces the deterministic gossip-traffic cost model of
`crates/aura-bench/src/p4.rs` byte for byte:

  stand      : 100 validators (D-P1 fixture), 1 view/s (bytes/view == bytes/s)
  baseline   : one envelope per vote (100 x 192 B), raw proposal
               (8512 B), gossip_factor 25% over 2 advert hops, D=8
  P4         : vote batching k=8 (13 envelopes), RLE-compressed
               proposal (runs of 32), gossip_factor 10%, D=5
  budget     : 125 000 B/s per validator (1 Mbit/s; methodology in
               docs/perf/p4-network-report.md, D-P3)

Output (stdout) is byte-identical to
`cargo run -p aura-bench --example p4_network_example`. `--measured`
adds an honest wall-clock line (evidence only, D-P9).
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]

N_NODES = 100
VIEW_RATE = 1
VOTE_PAYLOAD = 128
ENV_BYTES = 64
PROPOSAL_BODY = 8192
PROPOSAL_HDR = 256
QC_PAYLOAD = 128
BATCH_K = 8
RLE_HDR = 16
D_BEFORE = 8
D_AFTER = 5
D_LO = 3
D_HI = 8
HEARTBEAT_MS = 700
MCACHE = 5
GOSSIP_FACTOR_BEFORE = 25
GOSSIP_FACTOR_AFTER = 10
ADV_HOPS = 2
BUDGET_BYTES = 125_000
RUNS = 5


def sha(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def proposal_body() -> list[int]:
    return [((i // 32) % 251) for i in range(PROPOSAL_BODY)]


def rle_encode(body: list[int]) -> list[tuple[int, int]]:
    out = []
    if not body:
        return out
    value, count = body[0], 1
    for byte in body[1:]:
        if byte == value and count < 255:
            count += 1
        else:
            out.append((value, count))
            value, count = byte, 1
    out.append((value, count))
    return out


def rle_decode(pairs: list[tuple[int, int]]) -> list[int]:
    out = []
    for value, count in pairs:
        out.extend([value] * count)
    return out


def votes_wire_before(votes: int) -> int:
    return votes * (VOTE_PAYLOAD + ENV_BYTES)


def votes_wire_after(votes: int, batch_k: int) -> int:
    k = max(1, batch_k)
    full, rest = divmod(votes, k)
    total = full * (k * VOTE_PAYLOAD + ENV_BYTES)
    if rest:
        total += rest * VOTE_PAYLOAD + ENV_BYTES
    return total


def proposal_wire_before() -> int:
    return PROPOSAL_BODY + PROPOSAL_HDR + ENV_BYTES


def proposal_wire_after() -> int:
    pairs = rle_encode(proposal_body())
    return RLE_HDR + len(pairs) * 2 + ENV_BYTES


def qc_wire_bytes() -> int:
    return QC_PAYLOAD + ENV_BYTES


def msg_bytes(votes_wire: int, proposal_wire: int) -> int:
    return votes_wire + proposal_wire + qc_wire_bytes()


def overhead_bytes(factor_pct: int, msg: int) -> int:
    return factor_pct * msg * ADV_HOPS // 100


def per_validator_bytes(degree: int, msg: int, overhead: int) -> int:
    return degree * (msg + overhead)


def build_mesh(n: int, degree: int) -> list[list[int]]:
    adj = []
    for i in range(n):
        peers = sorted(
            {(i + 1) % n, (i + n - 1) % n, (i + 2) % n, (i + n - 2) % n, (i + n // 2) % n}
        )
        adj.append(peers[:degree])
    return adj


def mesh_connected(adj: list[list[int]]) -> bool:
    if not adj:
        return True
    seen = [False] * len(adj)
    queue = [0]
    seen[0] = True
    while queue:
        node = queue.pop()
        for peer in adj[node]:
            if not seen[peer]:
                seen[peer] = True
                queue.append(peer)
    return all(seen)


def ledger_lines() -> list[str]:
    votes = votes_wire_after(N_NODES, BATCH_K)
    proposal = proposal_wire_after()
    msg = msg_bytes(votes, proposal)
    over = overhead_bytes(GOSSIP_FACTOR_AFTER, msg)
    per_val = per_validator_bytes(D_AFTER, msg, over)
    lines = [
        f"p4s|ledger|votes={votes}",
        f"p4s|ledger|proposal={proposal}",
        f"p4s|ledger|qc={qc_wire_bytes()}",
        f"p4s|ledger|overhead={over}",
        f"p4s|ledger|per_val={per_val}",
    ]
    for i, peers in enumerate(build_mesh(N_NODES, D_AFTER)):
        joined = ",".join(str(p) for p in peers)
        lines.append(f"p4s|mesh|{i}:{joined}")
    return lines


def state_digest() -> str:
    joined = "|".join(ledger_lines()) + "|"
    return hashlib.sha256(joined.encode()).hexdigest()


def pairs() -> dict:
    votes_b = votes_wire_before(N_NODES)
    votes_a = votes_wire_after(N_NODES, BATCH_K)
    prop_b = proposal_wire_before()
    prop_a = proposal_wire_after()
    msg_b = msg_bytes(votes_b, prop_b)
    msg_a = msg_bytes(votes_a, prop_a)
    over_b = overhead_bytes(GOSSIP_FACTOR_BEFORE, msg_b)
    over_a = overhead_bytes(GOSSIP_FACTOR_AFTER, msg_a)
    traffic_b = per_validator_bytes(D_BEFORE, msg_b, over_b)
    traffic_a = per_validator_bytes(D_AFTER, msg_a, over_a)
    return {
        "votes": (votes_b, votes_a),
        "proposal": (prop_b, prop_a),
        "overhead": (over_b, over_a),
        "mesh": (D_BEFORE, D_AFTER),
        "traffic": (traffic_b, traffic_a),
    }


def build() -> dict:
    p = pairs()
    before, after = p["traffic"]
    assert after < BUDGET_BYTES, "P4 DoD: traffic/validator within budget"
    assert before > BUDGET_BYTES, "baseline must honestly fail the budget"
    assert mesh_connected(build_mesh(N_NODES, D_AFTER)), "mesh must connect"
    # lossless codec gate
    body = proposal_body()
    assert rle_decode(rle_encode(body)) == body, "RLE roundtrip"
    # conservative 2:1 sensitivity (report line): still in budget
    conservative = per_validator_bytes(
        D_AFTER,
        msg_bytes(votes_wire_after(N_NODES, BATCH_K), (PROPOSAL_BODY + PROPOSAL_HDR) // 2 + 32 + ENV_BYTES),
        overhead_bytes(
            GOSSIP_FACTOR_AFTER,
            msg_bytes(votes_wire_after(N_NODES, BATCH_K), (PROPOSAL_BODY + PROPOSAL_HDR) // 2 + 32 + ENV_BYTES),
        ),
    )
    assert conservative < BUDGET_BYTES, "2:1 sensitivity must fit the budget"

    wd = sha(
        f"p4w|net-gossip|nodes={N_NODES}|views={VIEW_RATE}|vote={VOTE_PAYLOAD}"
        f"|body={PROPOSAL_BODY}|k={BATCH_K}"
    )
    md = sha("p0m|vcpu=8|mem_gib=32|storage=nvme|validators=100|net_mbit=1000")
    run_costs = [after] * RUNS
    spread = 0
    reduction = (before - after) * 100 // before
    canonical = (
        f"p4b|{wd}|{md}|runs={RUNS}|spread={spread}"
        f"|p50={after}|p95={after}|p99={after}|total={after * RUNS}"
        f"|before={before}|after={after}|budget={BUDGET_BYTES}"
        f"|reduction_pct={reduction}"
        f"|votes={p['votes'][0]}/{p['votes'][1]}"
        f"|proposal={p['proposal'][0]}/{p['proposal'][1]}"
        f"|overhead={p['overhead'][0]}/{p['overhead'][1]}"
        f"|mesh={p['mesh'][0]}/{p['mesh'][1]}"
    )
    return {
        "workload_digest": wd,
        "machine_digest": md,
        "runs": RUNS,
        "spread_pct": spread,
        "p50": after,
        "p95": after,
        "p99": after,
        "total": after * RUNS,
        "before": before,
        "after": after,
        "budget": BUDGET_BYTES,
        "reduction_pct": reduction,
        "votes_before": p["votes"][0],
        "votes_after": p["votes"][1],
        "proposal_before": p["proposal"][0],
        "proposal_after": p["proposal"][1],
        "overhead_before": p["overhead"][0],
        "overhead_after": p["overhead"][1],
        "mesh_before": p["mesh"][0],
        "mesh_after": p["mesh"][1],
        "nodes": N_NODES,
        "view_rate": VIEW_RATE,
        "sensitivity_2to1_bytes": conservative,
        "bench_digest": sha(canonical),
    }


def main() -> int:
    report = build()
    print(f"P4-RUNS-OK {report['runs']}")
    print("P4-NETWORK-OK " + json.dumps(report))

    body = {
        "report": {k: v for k, v in report.items() if k != "sensitivity_2to1_bytes"},
        "cost_model": {
            "vote_payload": VOTE_PAYLOAD,
            "env_bytes": ENV_BYTES,
            "proposal_body": PROPOSAL_BODY,
            "proposal_hdr": PROPOSAL_HDR,
            "qc_payload": QC_PAYLOAD,
            "batch_k": BATCH_K,
            "rle_hdr": RLE_HDR,
            "d_before": D_BEFORE,
            "d_after": D_AFTER,
            "d_lo": D_LO,
            "d_hi": D_HI,
            "heartbeat_ms": HEARTBEAT_MS,
            "mcache": MCACHE,
            "gossip_factor_before": GOSSIP_FACTOR_BEFORE,
            "gossip_factor_after": GOSSIP_FACTOR_AFTER,
            "adv_hops": ADV_HOPS,
            "budget_bytes": BUDGET_BYTES,
            "sensitivity_2to1_bytes": report["sensitivity_2to1_bytes"],
            "formula_per_val": "degree * (msg + overhead); overhead = factor*msg*hops/100",
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
    (REPO / "sim/p4-network.json").write_text(
        json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    if "--measured" in sys.argv:
        import time

        body_l = proposal_body()
        start = time.perf_counter_ns()
        sink = rle_encode(body_l)
        enc = time.perf_counter_ns() - start
        start = time.perf_counter_ns()
        sink2 = rle_decode(sink)
        dec = time.perf_counter_ns() - start
        assert sink2 == body_l
        print(
            "P4-MEASURED "
            + json.dumps({"python_rle_encode_ns": enc, "python_rle_decode_ns": dec})
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
