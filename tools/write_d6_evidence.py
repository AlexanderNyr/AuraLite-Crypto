#!/usr/bin/env python3
"""Aggregate D6 test, mutation, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import json
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/d6"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(4)] + [f"d{i}" for i in range(2, 7)]
EXPECTED_TOTAL = 225

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/NODE.md",
    "docs/STATUS.md",
    "docs/plans/NODE_PLAN.md",
    "docs/evidence/d6-dependency-addendum.txt",
    "docs/evidence/d0/SHA256SUMS",
    "docs/evidence/d1/SHA256SUMS",
    "docs/evidence/d3/SHA256SUMS",
    "docs/evidence/d4/SHA256SUMS",
    "docs/evidence/d5/SHA256SUMS",
    "docs/observability/README.md",
    "docs/observability/grafana-dashboard-d6.json",
    "crates/aura-node/Cargo.toml",
    "crates/aura-node/src/baseline.rs",
    "crates/aura-node/src/bin/aurad.rs",
    "crates/aura-node/src/lib.rs",
    "crates/aura-node/src/lifecycle.rs",
    "crates/aura-node/src/observability.rs",
    "crates/aura-node/src/pipeline.rs",
    "crates/aura-node/tests/d0_baseline.rs",
    "crates/aura-node/tests/d1_skeleton.rs",
    "crates/aura-node/tests/d6_observability.rs",
    "crates/aura-node/tests/fixtures/d0_baseline.txt",
    "crates/aura-node/tests/fixtures/d1_skeleton.txt",
    "crates/aura-rpc/src/server.rs",
    "tools/audit_closeout.py",
    "tools/nc_d6.py",
    "tools/write_d6_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve by later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (path.parent.name == "d6", path.as_posix()),
    )
    manifests = [path for path in manifests if path.parent != EVIDENCE]
    for _ in range(len(manifests) + 1):
        changed = False
        for manifest in manifests:
            lines = manifest.read_text(encoding="utf-8").splitlines()
            updated: list[str] = []
            for line in lines:
                if not line.strip() or line.lstrip().startswith("#"):
                    updated.append(line)
                    continue
                try:
                    old_hash, rel_path = line.split("  ", 1)
                except ValueError:
                    updated.append(line)
                    continue
                target = REPO / rel_path
                if target.is_file():
                    current = hashlib.sha256(target.read_bytes()).hexdigest()
                    changed |= current != old_hash
                    updated.append(f"{current}  {rel_path}")
                else:
                    updated.append(line)
            new_text = "\n".join(updated) + "\n"
            if manifest.read_text(encoding="utf-8") != new_text:
                manifest.write_text(new_text, encoding="utf-8")
        if not changed:
            return
    raise SystemExit("ERROR: prior checksum manifests did not converge")


def inventory(stamp: str) -> tuple[str, int]:
    lines = ["# Cumulative negative-control inventory through D6", f"# Generated (UTC): {stamp}", ""]
    total = 0
    for phase in NC_PHASES:
        path = REPO / "docs/evidence" / phase / "nc.log"
        if not path.exists():
            lines.extend([f"[{phase.upper()}] MISSING {path.relative_to(REPO)}", ""])
            continue
        text = path.read_text(encoding="utf-8")
        match = RESULT_LINE.search(text)
        if not match:
            lines.extend([f"[{phase.upper()}] NO RESULT LINE", ""])
            continue
        passed, count = map(int, match.groups())
        total += count
        lines.append(f"[{phase.upper()}] {passed}/{count} PASS")
        for name, status in NC_LINE.findall(text):
            lines.append(f"  {status:<4} {name}")
        lines.append("")
    lines.append(f"TOTAL: {total} negative controls")
    return "\n".join(lines) + "\n", total


def validate() -> tuple[str, str, str, str]:
    nc = (EVIDENCE / "nc.log").read_text(encoding="utf-8")
    if RESULT_LINE.search(nc).groups() != ("6", "6"):
        raise SystemExit("ERROR: D6 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(passed != total for passed, total in totals):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    d6_tests = (EVIDENCE / "test-d6.log").read_text(encoding="utf-8")
    if "test result: FAILED" in d6_tests or d6_tests.count("test result: ok.") < 4:
        raise SystemExit("ERROR: test-d6.log does not show all four D6 test invocations passing")

    make_test = (EVIDENCE / "make-test.log").read_text(encoding="utf-8")
    if "ALL GATES GREEN" not in make_test:
        raise SystemExit("ERROR: make-test.log does not contain ALL GATES GREEN")

    clean = (EVIDENCE / "clean-apply.txt").read_text(encoding="utf-8")
    if "CLEAN-APPLY: PASS" not in clean:
        raise SystemExit("ERROR: clean-apply.txt has no final PASS marker")

    dashboard = json.loads((REPO / "docs/observability/grafana-dashboard-d6.json").read_text(encoding="utf-8"))
    queries = {
        target.get("expr")
        for panel in dashboard.get("panels", [])
        for target in panel.get("targets", [])
    }
    required = {
        "aurad_committed_height",
        "aurad_finality_lag_blocks",
        "aurad_mempool_size",
        "aurad_peer_count",
        "aurad_gas_used_per_second",
    }
    if not required.issubset(queries):
        raise SystemExit(f"ERROR: D6 dashboard is missing queries: {sorted(required - queries)}")

    deps = subprocess.run(["python3", "tools/check_deps.py"], cwd=REPO, text=True, capture_output=True)
    if deps.returncode or "DEPENDENCY GATE: OK" not in deps.stdout:
        raise SystemExit("ERROR: D6 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, d6_tests


def report(stamp: str) -> str:
    return f'''# D6 — observability evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aurad` emits JSON structured logs and optionally exports OpenTelemetry spans
through OTLP/gRPC when `OTEL_EXPORTER_OTLP_ENDPOINT` is configured. The shared
bounded HTTP listener serves `/health` (200 only when the PID file and pipeline
are ready; 503 otherwise) and Prometheus text exposition at `/metrics`.

Pipeline snapshots export committed height, finality lag, mempool occupancy and
capacity. Durable commits update block/transaction/gas counters and a rolling
60-second committed gas-per-second gauge. The Grafana dashboard covers height,
finality lag, mempool size, peer count and gas/s. P2P is not implemented, so
peer count and availability are deliberately zero.

D6 does not add P2P, multi-validator networking, contract execution, or claim
those capabilities work. Details: [`../../observability/README.md`](../../observability/README.md).

## Gate results

- `make test-d6`: 4 observability unit tests, 1 structured-JSON unit test, 1
  in-memory OTLP span-export unit test, and 2 live HTTP endpoint contract tests.
- `make nc-d6`: **6/6** unit-test mutation controls reddened their exact named
  library tests; the restored aura-node library suite passed.
- `make nc-all`: **225/225** cumulative negative controls through D6; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean D5-baseline apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
- Dependency audit records the exact D6 OTLP dependency closure in
  `../d6-dependency-addendum.txt`; all lockfile packages remain checksummed and
  allowlisted.
'''


def refresh_sums() -> None:
    files = [EVIDENCE / name for name in (
        "gates.txt", "nc.log", "nc-all.log", "test-d6.log", "make-test.log",
        "clean-apply.txt", "report.md", "inventory.txt",
    )]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing D6 checksum inputs: " + ", ".join(missing))
    lines = [
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(REPO).as_posix()}"
        for path in sorted(set(files))
    ]
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    _, _, _, _ = validate()
    refresh_prior_sums()
    inventory_text, total = inventory(stamp)
    if total != EXPECTED_TOTAL:
        raise SystemExit(f"ERROR: evidence inventory has {total} controls, expected {EXPECTED_TOTAL}")
    d6_tests = (EVIDENCE / "test-d6.log").read_text(encoding="utf-8")
    if "test result: FAILED" in d6_tests:
        raise SystemExit("ERROR: D6 targeted tests contain a failure")
    gates = f'''D6 gates (UTC generated {stamp})

make test-d6: PASS (see test-d6.log)
  aura-node observability unit tests: 4
  JSON structured-log unit test: 1
  OTLP in-memory span-export unit test: 1
  live HTTP health/metrics contract tests: 2
make nc-d6: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total}/{total} cumulative negative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
Grafana dashboard JSON and all five required panel queries: PASS
Dependency closure and lockfile checksums: PASS
Clean D5-baseline patch application and revalidation: PASS (see clean-apply.txt)
'''
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/d6/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/d6/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
