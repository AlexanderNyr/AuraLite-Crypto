#!/usr/bin/env python3
"""Aggregate completed D4 gates into a reproducible evidence receipt."""
from __future__ import annotations

import hashlib
import re
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/d4"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += ["b0", "b1", "b2", "b3", "d2", "d3", "d4"]
EXPECTED_TOTAL = 213


def inventory(stamp: str) -> tuple[str, int]:
    lines = ["# D4 cumulative negative-control inventory", f"# Generated (UTC): {stamp}", ""]
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


def report(stamp: str) -> str:
    return f'''# D4 — JSON-RPC 2.0 evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-rpc` implements JSON-RPC 2.0 request, batch, notification and error
semantics over bounded HTTP (`POST /rpc`, also `/`) and RFC 6455 WebSocket
(`/ws`). The pipeline-backed adapter exposes transaction submission, balance
and transaction queries, recent committed blocks, gas estimation, and durable
commit subscriptions. WebSocket subscriptions unregister on drop, including
when no later commit arrives.

Only native transfers execute. Contract calls and unsupported transaction
execution return explicit errors; D4 does not add P2P, multiple validators,
smart-contract execution or checkpoint recovery. HTTP bodies and WebSocket
messages are capped at 1 MiB, batches at 32 requests, concurrent connections
at 128, and incomplete HTTP requests time out after 15 seconds.

## Gate results

- `make test-d4`: RPC library **11/11**, 50 golden vectors **1/1**, node-backed
  contract tests **2/2**, managed-subscription lifecycle unit test **1/1**.
- `make nc-d4`: **6/6** unit-test-based mutation controls.
- `make nc-all`: **213/213** cumulative negative controls across D0–D4 and
  earlier phases; per-phase receipts are summarized in `inventory.txt`.
- Clean-copy `make test`: passed, including workspace formatting, workspace
  clippy with warnings denied, workspace tests, docs links, dependency checks,
  vector inventory, rig wiring and the close-out audit. Raw output:
  `make-test.log`.
- `fuzz_rpc`: 60-second libFuzzer shard across HTTP and WebSocket parser entry
  points; **733,119 executions, 0 crashes**, peak RSS 469 MiB. Raw output:
  `fuzz-rpc.log`. This is not the full H3 campaign.
- Clean-baseline apply and revalidation: see `clean-apply.txt`.

## Remaining H3 scope

The full 24-hour campaign and planned p2p-frame, WASM-verifier and wallet-
keystore targets remain H3 work; D4 claims only its RPC parser shard.

Protocol: [`../../spec/json-rpc.md`](../../spec/json-rpc.md).
'''


def refresh_sums() -> None:
    paths = [
        EVIDENCE / name
        for name in ("gates.txt", "nc.log", "nc-all.log", "make-test.log", "fuzz-rpc.log", "clean-apply.txt", "report.md", "inventory.txt")
    ]
    paths += [
        REPO / relative
        for relative in (
            "docs/spec/json-rpc.md",
            "docs/spec/FORMAT_INDEX.md",
            "docs/plans/NODE_PLAN.md",
            "docs/NODE.md",
            "docs/STATUS.md",
            "CHANGELOG.md",
            "Makefile",
            "Cargo.toml",
            "Cargo.lock",
            "crates/aura-rpc/src/lib.rs",
            "crates/aura-rpc/src/server.rs",
            "crates/aura-rpc/tests/d4_vectors.rs",
            "crates/aura-rpc/tests/fixtures/d4_json_rpc.txt",
            "crates/aura-node/src/pipeline.rs",
            "crates/aura-node/src/rpc.rs",
            "crates/aura-node/src/bin/aurad.rs",
            "crates/aura-node/tests/d4_rpc.rs",
            "crates/aura-node/tests/fixtures/d0_baseline.txt",
            "crates/aura-node/tests/fixtures/d1_skeleton.txt",
            "fuzz/Cargo.toml",
            "fuzz/Cargo.lock",
            "fuzz/README.md",
            "fuzz/fuzz_targets/fuzz_rpc.rs",
            "tools/audit_closeout.py",
            "tools/gen_d4_vectors.py",
            "tools/nc_d4.py",
            "tools/write_d4_evidence.py",
            "docs/evidence/d4-dependency-addendum.txt",
        )
    ]
    files = sorted({path for path in paths if path.is_file()})
    lines = [f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(REPO)}" for path in files]
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    required = [
        EVIDENCE / "gates.txt",
        EVIDENCE / "nc.log",
        EVIDENCE / "nc-all.log",
        EVIDENCE / "make-test.log",
        EVIDENCE / "fuzz-rpc.log",
        EVIDENCE / "clean-apply.txt",
    ]
    missing = [str(path.relative_to(REPO)) for path in required if not path.exists()]
    if missing:
        print("ERROR: missing D4 evidence inputs: " + ", ".join(missing))
        return 1
    if "ALL GATES GREEN" not in (EVIDENCE / "make-test.log").read_text(encoding="utf-8"):
        print("ERROR: make-test.log does not end with ALL GATES GREEN")
        return 1
    nc_text = (EVIDENCE / "nc.log").read_text(encoding="utf-8")
    match = RESULT_LINE.search(nc_text)
    if not match or match.groups() != ("6", "6"):
        print("ERROR: D4 NC log is not 6/6")
        return 1
    inventory_text, total = inventory(stamp)
    if total != EXPECTED_TOTAL:
        print(f"ERROR: cumulative NC inventory is {total}, expected {EXPECTED_TOTAL}")
        return 1
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote {EVIDENCE.relative_to(REPO)}/inventory.txt ({total} controls)")
    print(f"wrote {EVIDENCE.relative_to(REPO)}/report.md")
    print(f"wrote {EVIDENCE.relative_to(REPO)}/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
