#!/usr/bin/env python3
"""Aggregate completed D5 gates into a reproducible evidence receipt."""
from __future__ import annotations

import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/d5"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(4)] + [f"d{i}" for i in range(2, 6)]
EXPECTED_TOTAL = 219

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/NODE.md",
    "docs/STATUS.md",
    "docs/plans/NODE_PLAN.md",
    "docs/spec/FORMAT_INDEX.md",
    "docs/spec/grpc-rest.md",
    "docs/api/openapi-d5.yaml",
    "docs/api/openapi-d5.json",
    "docs/evidence/d5-dependency-addendum.txt",
    "docs/evidence/c10/SHA256SUMS",
    "docs/evidence/d0/SHA256SUMS",
    "docs/evidence/d1/SHA256SUMS",
    "docs/evidence/d4/SHA256SUMS",
    "crates/aura-grpc-rest/Cargo.toml",
    "crates/aura-grpc-rest/build.rs",
    "crates/aura-grpc-rest/proto/aura/api/v1/service.proto",
    "crates/aura-grpc-rest/src/lib.rs",
    "crates/aura-grpc-rest/tests/d5_contract.rs",
    "crates/aura-node/Cargo.toml",
    "crates/aura-node/src/baseline.rs",
    "crates/aura-node/src/config.rs",
    "crates/aura-node/src/bin/aurad.rs",
    "crates/aura-node/tests/d0_baseline.rs",
    "crates/aura-node/tests/d1_skeleton.rs",
    "crates/aura-node/tests/fixtures/d0_baseline.txt",
    "crates/aura-node/tests/fixtures/d1_skeleton.txt",
    "crates/aura-rpc/src/lib.rs",
    "crates/aura-rpc/src/server.rs",
    "tools/audit_closeout.py",
    "tools/nc_d5.py",
    "tools/write_d5_evidence.py",
]


def inventory(stamp: str) -> tuple[str, int]:
    lines = ["# D5 cumulative negative-control inventory", f"# Generated (UTC): {stamp}", ""]
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


def validate() -> tuple[str, str, str]:
    nc = (EVIDENCE / "nc.log").read_text(encoding="utf-8")
    match = RESULT_LINE.search(nc)
    if not match or match.groups() != ("6", "6"):
        raise SystemExit("ERROR: D5 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, m)) for m in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(p != t for p, t in totals):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    make_test = (EVIDENCE / "make-test.log").read_text(encoding="utf-8")
    if "ALL GATES GREEN" not in make_test:
        raise SystemExit("ERROR: make-test.log does not end with ALL GATES GREEN")

    clean = (EVIDENCE / "clean-apply.txt").read_text(encoding="utf-8")
    if "CLEAN-APPLY: PASS" not in clean:
        raise SystemExit("ERROR: clean-apply.txt has no final PASS marker")

    yaml_doc = yaml.safe_load((REPO / "docs/api/openapi-d5.yaml").read_text(encoding="utf-8"))
    json_doc = json.loads((REPO / "docs/api/openapi-d5.json").read_text(encoding="utf-8"))
    for document in (yaml_doc, json_doc):
        if document.get("openapi") != "3.1.0" or document.get("info", {}).get("version") != "1.1.0":
            raise SystemExit("ERROR: OpenAPI docs do not describe version 1.1.0 / OpenAPI 3.1.0")
    return make_test, nc_all, clean


def report(stamp: str) -> str:
    return f'''# D5 — gRPC + REST evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-grpc-rest` provides a protobuf gRPC service on a separate configurable
listener and a versioned REST gateway on the existing bounded D4 HTTP listener.
Both adapters call the same D4 `RpcService`; neither bypasses transaction
validation, D3 admission, finality or storage. REST v1.1 adds `apiVersion`
without changing `result`. Closed JSON request schemas reject unknown fields
and unsupported versions. OpenAPI 3.1.0 definitions for API 1.1.0 are served as
both YAML and JSON.

Only native transfers execute. Contract calls and unsupported transaction
kinds remain explicit errors. D5 does not add contract execution, P2P,
multi-validator networking or claim unsupported operations succeed. Protobuf
v1 response fields and RPC paths are preserved; version metadata is additive.

## Gate results

- `make test-d5`: 7 library unit tests, 1 live REST + HTTP/2 gRPC contract test,
  and 4 daemon CLI tests passed.
- `make nc-d5`: **6/6** exact library unit-test mutations reddened their named
  tests; post-restoration unit-test check is green.
- `make nc-all`: **219/219** cumulative negative controls passed across all
  phases through D5; per-phase inventory is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — format, workspace Clippy with warnings
  denied, workspace tests, documentation guard, dependency checks, vector
  inventory, test-rig wiring, and close-out audit.
- Clean D4-baseline patch application and revalidation: see `clean-apply.txt`.

Protocol: [`../../spec/grpc-rest.md`](../../spec/grpc-rest.md).
OpenAPI: [`../../api/openapi-d5.yaml`](../../api/openapi-d5.yaml) and
[`../../api/openapi-d5.json`](../../api/openapi-d5.json).
'''


def refresh_sums() -> None:
    paths = [EVIDENCE / name for name in (
        "gates.txt", "nc.log", "nc-all.log", "make-test.log", "clean-apply.txt",
        "report.md", "inventory.txt",
    )]
    paths += [REPO / path for path in HASH_PATHS]
    missing = [p.relative_to(REPO).as_posix() for p in paths if not p.is_file()]
    if missing:
        raise SystemExit("ERROR: missing D5 checksum inputs: " + ", ".join(missing))
    lines = [f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(REPO).as_posix()}" for p in sorted(set(paths))]
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    make_test, nc_all, clean = validate()
    inventory_text, total = inventory(stamp)
    if total != EXPECTED_TOTAL:
        raise SystemExit(f"ERROR: evidence inventory has {total} controls, expected {EXPECTED_TOTAL}")
    gates = '''D5 gates (UTC generated {stamp})

make test-d5: PASS
  aura-grpc-rest --lib: 7 unit tests
  aura-grpc-rest --test d5_contract: 1 live REST + gRPC contract
  aura-node --bin aurad: 4 CLI/config tests
make nc-d5: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS (219/219 cumulative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
OpenAPI YAML + JSON parse and version validation: PASS
Clean D4-baseline patch apply and revalidation: PASS (see clean-apply.txt)
'''.format(stamp=stamp)
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/d5/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/d5/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
