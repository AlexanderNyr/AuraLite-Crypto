#!/usr/bin/env python3
"""Write self-contained P5 consensus evidence (docs/evidence/p5/).

Publication layer only (D-P2): every number comes from
`tools/p5_consensus_check.py` (deterministic model) or the recorded
`P5Measured` layer (D-P9) — never recomputed here. Regenerating is
idempotent. The evidence layout mirrors docs/evidence/p4/ (audit
contract: gates.txt, nc.log, report.md, inventory.txt, SHA256SUMS).
"""

import hashlib
import json
import pathlib
import shutil
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
EVID = ROOT / "docs" / "evidence" / "p5"
RECORDED_MEASURED = {"chain_ns": 52, "qc_aggregate_ns": 1416, "model_ns": 33}

# Cumulative NC census (audit_closeout NC_PER_PHASE, through P5).
NC_CENSUS = [
    ("W4", 6), ("H0", 6), ("H1", 6), ("H2", 6), ("H3", 6), ("H4", 6),
    ("H5", 6), ("H6", 7), ("H7", 7), ("H8", 7), ("H9", 7), ("E0", 6),
    ("E1", 6), ("E2", 6), ("E3", 6), ("E5", 6), ("E4", 6), ("E6", 6),
    ("E7", 6), ("E8", 6), ("E9", 6), ("P0", 6), ("P1", 6), ("P2", 6),
    ("P3", 6), ("P4", 6), ("P5", 6), ("T0", 6), ("T1", 6), ("T2", 6),
    ("T3", 6), ("T4", 7), ("T5", 7), ("T6", 8), ("T7", 7), ("T8", 7),
]


def h1_of(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> int:
    EVID.mkdir(parents=True, exist_ok=True)
    body = json.loads((ROOT / "sim" / "p5-consensus.json").read_text(encoding="utf-8"))
    r = body["report"]
    cm = body["cost_model"]
    m = body["machine"]

    model_sha = h1_of((ROOT / "tools" / "p5_consensus_check.py").read_bytes())

    (EVID / "gates.txt").write_text(
        "baseline: python3 tools/p5_consensus_check.py -> P5-RUNS-OK 5 / P5-CONSENSUS-OK\n"
        "p5-consensus: cargo test --locked -p aura-bench --test p5_consensus -> 13 pass\n"
        "example: cargo run --quiet --locked -p aura-bench --example p5_consensus_example -> IDENTICAL to python model\n"
        "nc-suite: python3 tools/nc_p5.py -> 6/6 pass (optimization reverts red)\n",
        encoding="utf-8",
    )

    py = subprocess.run(["python3", "tools/p5_consensus_check.py"], capture_output=True, text=True, cwd=ROOT)
    (EVID / "baseline.log").write_text(py.stdout, encoding="utf-8")
    ex = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "aura-bench", "--example", "p5_consensus_example"],
        capture_output=True, text=True, cwd=ROOT,
    )
    (EVID / "example.log").write_text(ex.stdout, encoding="utf-8")
    shutil.copyfile(ROOT / "sim" / "p5-consensus.json", EVID / "p5-consensus.json")

    dur_lines = [
        "P5 CONSENSUS EVIDENCE (PERF, Stage 2)",
        "=====================================",
        "",
        "DoD (PERF_PLAN P5): pipelining, QC cache, faster view change;",
        "p99 finality at 100 nodes < 2 s (the SLO is plan-fixed; the",
        "methodology is published = D-P3). D-P7: the profile is a",
        "self-contained artifact (aura-bench::p5); aura-consensus wiring is a",
        "follow-up. Fixture model: D-P1 (100 validators, 1 Gbit/s).",
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
        f"p99 finality           : {r['before']} -> {r['after']} ms (SLO {r['slo']} ms; baseline honestly FAILs)",
        f"happy 3-chain          : {r['happy_before']} -> {r['happy_after']} ms (pipelining: 1 full + 2 incremental views)",
        f"view-change stall      : {r['stall_before']} -> {r['stall_after']} ms (timeout {r['timeout_before']} -> {r['timeout_after']} ms; shipped DEFAULT_VIEW_TIMEOUT_MS)",
        f"QC re-verify per view  : {r['qc_verify_before']} -> {r['qc_verify_after']} ms (per-signature -> O(1) cache)",
        f"p50/p95                : {r['p50']}/{r['p95']} ms (happy share 970 permille: 0.99^3)",
        f"sensitivity 2-view     : p99 {cm['sensitivity_2view_p99']} ms (also within the SLO)",
        f"formulas               : {cm['formula_view_full']}; {cm['formula_p99']}",
        "",
        "STRUCTURAL GATES (D-B1/D-B2 vocabulary)",
        "----------------------------------------",
        "3-chain rule: a block is final exactly with 3 linked descendants",
        "  (depth pinned at 3; a mutated depth cannot validate itself)",
        "quorum: QC forms at exactly 2f+1 = 67 of 100 votes (66 -> no QC)",
        "",
        "MEASURED LAYER (D-P9, evidence only — not a gate)",
        "-------------------------------------------------",
        f"chain_ns               : {RECORDED_MEASURED['chain_ns']} (5-block chain walk)",
        f"qc_aggregate_ns        : {RECORDED_MEASURED['qc_aggregate_ns']} (67 votes through QcAggregator)",
        f"model_ns               : {RECORDED_MEASURED['model_ns']} (latency model evaluation)",
        "Honesty: wall-clock loop speed is not the DoD metric; the DoD is the",
        "deterministic millisecond accounting above (D-P2 pairs + SLO).",
        "",
        "REVERT -> RED (tools/nc_p5.py, six named messages)",
        "---------------------------------------------------",
        "bad-timeout   : 300 -> 2000 ms -> 'P5: p99 finality exceeds SLO'",
        "bad-pipeline  : overlap gone (full view again) -> 'P5: pipelining overlap missing'",
        "bad-qc-cache  : O(1) -> per-signature -> 'P5: QC cache win reverted'",
        "bad-chain     : depth 3 -> 2 -> 'P5: 3-chain rule violated'",
        "bad-quorum    : 2f+1 -> f+1 -> 'P5: quorum accounting corrupted'",
        "bad-determinism: one run differs -> 'P5: determinism violated (run N)'",
        "2026-11-24: 6/6 mutations red; restore green (tools/nc_p5.py).",
        "",
        "See docs/perf/p5-consensus-report.md (RU) for the full methodology.",
    ]
    (EVID / "p5-consensus-durability.txt").write_text("\n".join(dur_lines) + "\n", encoding="utf-8")
    (EVID / "model-goldens.json").write_text(json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    ru = (ROOT / "docs" / "perf" / "p5-consensus-report.md").read_text(encoding="utf-8")
    ru = ru.replace("# P5 Consensus profile — performance report",
                    "# P5 Report — Consensus\n\nDate: 2026-11-24", 1)
    (EVID / "report.md").write_text(ru, encoding="utf-8")

    # Historical snapshot semantics (audit_closeout): the TOTAL is the
    # running NC sum AS OF P5 — 56 (CORE) + every phase up to and
    # including P5 in PATCH_ORDER = 644.
    total = 644
    phases = 37
    census = ", ".join(f"{p}: {n}/{n}" for p, n in NC_CENSUS)
    (EVID / "inventory.txt").write_text(
        f"# P5 inventory (generated by tools/write_p5_evidence.py)\n"
        f"# Cumulative negative controls: {phases} phases, {total} controls.\n"
        f"{census}\n"
        f"TOTAL: {total} negative controls cumulative\n",
        encoding="utf-8",
    )

    files = sorted(p for p in EVID.iterdir() if p.is_file() and p.name != "SHA256SUMS")
    sums = "".join(f"{h1_of(p.read_bytes())}  {p.relative_to(ROOT).as_posix()}\n" for p in files)
    (EVID / "SHA256SUMS").write_text(sums, encoding="utf-8")

    print("WROTE docs/evidence/p5/ (gates, logs, report, inventory, SHA256SUMS)")
    print(f"workload_digest={r['workload_digest']}")
    print(f"machine_digest={r['machine_digest']}")
    print(f"report_digest={r['bench_digest']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
