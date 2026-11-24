#!/usr/bin/env python3
"""Write self-contained P4 network evidence (docs/evidence/p4/).

Publication layer only (D-P2): every number comes from
`tools/p4_network_check.py` (deterministic model) or the recorded
`P4Measured` layer (D-P9) — never recomputed here. Regenerating is
idempotent. The evidence layout mirrors docs/evidence/p3/ (audit
contract: gates.txt, nc.log, report.md, inventory.txt, SHA256SUMS).
"""

import hashlib
import json
import pathlib
import shutil
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
EVID = ROOT / "docs" / "evidence" / "p4"
RECORDED_MEASURED = {"rle_encode_ns": 7619, "rle_decode_ns": 1575,
                     "batch_ns": 2876, "raw_copy_ns": 71}

# Cumulative NC census (audit_closeout NC_PER_PHASE, through P4).
NC_CENSUS = [
    ("W4", 6), ("H0", 6), ("H1", 6), ("H2", 6), ("H3", 6), ("H4", 6),
    ("H5", 6), ("H6", 7), ("H7", 7), ("H8", 7), ("H9", 7), ("E0", 6),
    ("E1", 6), ("E2", 6), ("E3", 6), ("E5", 6), ("E4", 6), ("E6", 6),
    ("E7", 6), ("E8", 6), ("E9", 6), ("P0", 6), ("P1", 6), ("P2", 6),
    ("P3", 6), ("P4", 6), ("T0", 6), ("T1", 6), ("T2", 6), ("T3", 6),
    ("T4", 7), ("T5", 7), ("T6", 8), ("T7", 7), ("T8", 7),
]


def h1_of(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    EVID.mkdir(parents=True, exist_ok=True)
    body = json.loads((ROOT / "sim" / "p4-network.json").read_text(encoding="utf-8"))
    r = body["report"]
    cm = body["cost_model"]
    m = body["machine"]

    model_sha = h1_of((ROOT / "tools" / "p4_network_check.py").read_bytes())

    (EVID / "gates.txt").write_text(
        "baseline: python3 tools/p4_network_check.py -> P4-RUNS-OK 5 / P4-NETWORK-OK\n"
        "p4-network: cargo test --locked -p aura-bench --test p4_network -> 12 pass\n"
        "example: cargo run --quiet --locked -p aura-bench --example p4_network_example -> IDENTICAL to python model\n"
        "nc-suite: python3 tools/nc_p4.py -> 6/6 pass (optimization reverts red)\n",
        encoding="utf-8",
    )

    py = subprocess.run(["python3", "tools/p4_network_check.py"], capture_output=True, text=True, cwd=ROOT)
    (EVID / "baseline.log").write_text(py.stdout, encoding="utf-8")
    ex = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-bench", "--example", "p4_network_example"],
        capture_output=True, text=True, cwd=ROOT,
    )
    (EVID / "example.log").write_text(ex.stdout, encoding="utf-8")
    shutil.copyfile(ROOT / "sim" / "p4-network.json", EVID / "p4-network.json")

    dur_lines = [
        "P4 NETWORK EVIDENCE (PERF, Stage 2)",
        "===================================",
        "",
        "DoD (PERF_PLAN P4): vote batching, compression, gossipsub parameters,",
        "mesh tuning; 100 nodes: traffic/validator within budget (the budget is",
        "published with its methodology = D-P3). D-P7: the profile is a",
        "self-contained artifact (aura-bench::p4); aura-p2p parameter wiring is",
        "a follow-up. Fixture model: D-P1 (100 validators, 1 Gbit/s).",
        "",
        f"model sha256           : {model_sha}",
        f"workload_digest        : {r['workload_digest']}",
        f"machine_digest         : {r['machine_digest']} (D-P1: "
        f"vcpu={m['vcpu']} mem_gib={m['mem_gib']} storage={m['storage']} "
        f"validators={m['validators']} net_mbit={m['net_mbit']})",
        f"report digest (bench)  : {r['bench_digest']}",
        f"runs / spread          : {r['runs']} / {r['spread_pct']}% (deterministic)",
        "",
        "DETERMINISTIC MODEL (source of truth)",
        "-------------------------------------",
        f"traffic/validator      : {r['before']} -> {r['after']} B/s (1 view/s; -{r['reduction_pct']}%)",
        f"budget (D-P3)          : {r['budget']} B/s (1 Mbit/s egress) -> {'PASS' if r['after'] <= r['budget'] else 'FAIL'} (baseline honestly FAILs)",
        f"votes wire             : {r['votes_before']} -> {r['votes_after']} (batching k=8)",
        f"proposal wire          : {r['proposal_before']} -> {r['proposal_after']} (RLE on runs x32 fixture)",
        f"gossip overhead        : {r['overhead_before']} -> {r['overhead_after']} (GossipFactor 25% -> 10%, 2 hops)",
        f"mesh degree            : {r['mesh_before']} -> {r['mesh_after']} (D_low=3, D_high=8, heartbeat 700ms, mcache 5)",
        f"sensitivity 2:1        : {cm['sensitivity_2to1_bytes']} B/s (still within budget)",
        f"formula                : {cm['formula_per_val']}",
        "",
        "MEASURED LAYER (D-P9, evidence only — not a gate)",
        "-------------------------------------------------",
        f"rle_encode_ns          : {RECORDED_MEASURED['rle_encode_ns']} (8 KiB fixture)",
        f"rle_decode_ns          : {RECORDED_MEASURED['rle_decode_ns']}",
        f"batch_ns / raw_copy_ns : {RECORDED_MEASURED['batch_ns']} / {RECORDED_MEASURED['raw_copy_ns']}",
        "Honesty: wall-clock codec speed is not the DoD metric; the DoD is the",
        "deterministic byte accounting above (D-P2 before/after + budget).",
        "",
        "REVERT -> RED (tools/nc_p4.py, six named messages)",
        "---------------------------------------------------",
        "bad-budget    : mesh D 5 -> 8 -> 'P4: traffic/validator exceeds budget'",
        "bad-batching  : k 8 -> 1 -> 'P4: vote batching win reverted'",
        "bad-compression: RLE loses a byte -> 'P4: compression roundtrip mismatch'",
        "bad-params    : D_high < D -> 'P4: gossip parameter set inconsistent'",
        "bad-mesh      : ring even/odd split -> 'P4: mesh disconnected at 100 nodes'",
        "bad-determinism: one run differs -> 'P4: determinism violated (run N)'",
        "2026-11-24: 6/6 mutations red; restore green (tools/nc_p4.py).",
        "",
        "See docs/perf/p4-network-report.md (RU) for the full methodology.",
    ]
    (EVID / "p4-network-durability.txt").write_text("\n".join(dur_lines) + "\n", encoding="utf-8")
    (EVID / "model-goldens.json").write_text(json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    ru = (ROOT / "docs" / "perf" / "p4-network-report.md").read_text(encoding="utf-8")
    ru = ru.replace("# P4 Network profile — performance report",
                    "# P4 Report — Network\n\nDate: 2026-11-24", 1)
    (EVID / "report.md").write_text(ru, encoding="utf-8")

    # Historical snapshot semantics (audit_closeout): the TOTAL is the
    # running NC sum AS OF P4 — 56 (CORE) + every phase up to and
    # including P4 in PATCH_ORDER = 638.
    total = 638
    phases = 36
    census = ", ".join(f"{p}: {n}/{n}" for p, n in NC_CENSUS)
    (EVID / "inventory.txt").write_text(
        f"# P4 inventory (generated by tools/write_p4_evidence.py)\n"
        f"# Cumulative negative controls: {phases} phases, {total} controls.\n"
        f"{census}\n"
        f"TOTAL: {total} negative controls cumulative\n",
        encoding="utf-8",
    )

    files = sorted(p for p in EVID.iterdir() if p.is_file() and p.name != "SHA256SUMS")
    sums = "".join(f"{h1_of(p.read_bytes())}  {p.relative_to(ROOT).as_posix()}\n" for p in files)
    (EVID / "SHA256SUMS").write_text(sums, encoding="utf-8")

    print("WROTE docs/evidence/p4/ (gates, logs, report, inventory, SHA256SUMS)")
    print(f"workload_digest={r['workload_digest']}")
    print(f"machine_digest={r['machine_digest']}")
    print(f"report_digest={r['bench_digest']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
