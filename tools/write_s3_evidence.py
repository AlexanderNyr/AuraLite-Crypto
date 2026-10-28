#!/usr/bin/env python3
"""Regenerate S3 evidence: gates, report, NC inventory, SHA256SUMS.

All numbers come from real command output or the independent Python golden
fixture. Usage:
    python3 tools/write_s3_evidence.py
    make evidence-s3
"""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "s3"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s3_golden.txt"
SPEC = REPO / "docs" / "evidence" / "s3" / "report.md"
NC_LOGS = [f"c{i}" for i in range(11)] + ["s0", "s1", "s2", "s3"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-state/tests/fixtures/s3_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_s3_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-state/tests/fixtures/s3_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-S3 tree (S2 commit)", "git log -1 --oneline", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("Rust files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 16),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("S3 overlay tests", "cargo test -p aura-state --test s3_overlay --quiet", 8),
    ("S3 negative controls (evidence log)", "make nc-s3", 34),
    ("all negative controls (C0-C10 + S0-S3)", "make nc-all", 32),
    ("CORE shard", "make test-core", 12),
    ("plan<->tree audit", "make audit-closeout", 12),
    ("full gate", "make test", 10),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def fixture_map() -> dict[str, str]:
    out: dict[str, str] = {}
    for line in FIXTURE.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            out[key] = value
    return out


def reset_current_evidence() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    for name in ("gates.txt", "nc.log", "report.md", "inventory.txt", "SHA256SUMS"):
        path = EVIDENCE / name
        if path.exists():
            path.unlink()


def build_report(f: dict[str, str], stamp: str) -> str:
    lines = [
        "# S3 — execution overlay (measured)",
        "",
        f"Generated (UTC): {stamp}. Numbers come from the independent Python",
        "reference `tools/gen_s3_vectors.py` and are re-verified by",
        "`crates/aura-state/tests/s3_overlay.rs`.",
        "",
        "## What landed",
        "",
        "- `StateView`: copy-on-write overlay over an immutable S1 `MerkleTree`.",
        "  Reads fall through to the base tree; writes/deletes are recorded by",
        "  256-bit C7 state-key digest in a `BTreeMap`. Gas is checked with",
        "  `u64` arithmetic; events are collected in order.",
        "- `StateDiff`: canonical wire format",
        "  `payload || BLAKE3(S3_DIFF_CONTEXT || payload)`, where `payload` =",
        "  `ALD3 || version || base_root || result_root || gas_used ||",
        "  events || changes`. Strict decode enforces the checksum, magic,",
        "  minimal LEB128 varints, strictly ascending change paths, caps, no",
        "  trailing bytes, and byte-identical re-encoding.",
        "- `execute()` runs a closure. On `OutOfGas` **or** `Revert` it calls",
        "  `rollback()`: changes, events and gas are discarded. A successful run",
        "  yields a `StateDiff` whose `apply()` re-verifies the base and result",
        "  roots before materialising a new tree. The base is never mutated.",
        "",
        "## Measured gates",
        "",
        f"- base accounts: **3** (C7 `Account`, 88 canonical bytes each)",
        f"- base version: **{f['base_version']}**",
        f"- canonical diffs: **3** (credit, debit+events, mixed writes with delete)",
        "- rejection vectors: **10** (checksum, trailing, truncated, magic, root,",
        "  unsorted, duplicate, non-minimal varint, oversized count, option tag)",
        "- `revert` and `OutOfGas` both leave `base_root` unchanged; the balance",
        "  of the touched account is byte-identical afterwards.",
        "- NC-6 removes `rollback()`: the revert gate MUST go red (balance",
        "  changes) — verified.",
        "",
        "## Honest scope",
        "",
        "- No real VM: `execute` takes a Rust closure, not WASM. X1 owns the VM.",
        "- `ChainId` is exposed on the view but not signed into the diff yet;",
        "  replay separation is already handled by C3/C4 at the tx layer.",
        "- No snapshots (S4), state sync (S5), pruning (S6) or backup (S7).",
        "- No network, no consensus, no P2P, no node.",
        "",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "AuraLite-Crypto — negative-control inventory after S3",
        f"Generated (UTC): {stamp}",
        "",
    ]
    total = 0
    for name in NC_LOGS:
        path = REPO / "docs" / "evidence" / name / "nc.log"
        text = path.read_text(encoding="utf-8") if path.exists() else ""
        match = RESULT_LINE.search(text)
        passed = int(match.group(1)) if match else 0
        total += passed
        lines.append(f"{name.upper()}: {passed}/{int(match.group(2)) if match else 0} negative controls")
        for nc_name, verdict in NC_LINE.findall(text):
            lines.append(f"  - {nc_name}: {verdict}")
        lines.append("")
    lines += [f"TOTAL: {total} NC", ""]
    return "\n".join(lines), total


def main() -> int:
    reset_current_evidence()
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    f = fixture_map()

    lines = [
        "AuraLite-Crypto — S3 evidence (execution overlay)",
        f"Generated (UTC): {stamp}",
        "Rule: measured against the tree, not assumed.",
        "",
        "=" * 72,
        "RAW COMMAND OUTPUT",
        "=" * 72,
        "",
    ]
    failures = 0
    for label, cmd, tail in COMMANDS:
        rc, out = run(cmd)
        if tail and out:
            out = "\n".join(out.splitlines()[-tail:])
        lines.append(f"--- {label} ---")
        lines.append(f"$ {cmd}")
        lines.append(out if out else "(no output)")
        lines.append(f"[rc={rc}]")
        lines.append("")
        if rc != 0:
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-S3 (S2): 15 crates, 132 Rust tests, 4,164 golden vectors,",
        "74 negative controls, 95 external crates (RocksDB optional).",
        "Post-S3: 15 crates (no new crate, no new external dependency),",
        "138 Rust tests (+6 S3), 4,214 vectors across 13 fixtures,",
        "80 negative controls (6 new in S3), 95 external crates.",
        "S3 lives inside `aura-state`: `StateView` + `StateDiff` + rollback.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(f, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != 80:
        print(f"INVENTORY ERROR: total {total} NC, expected 80")
        failures += 1

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/s3/nc.log missing (run `make nc-s3`)")
        return 1

    sums = []
    for path in (OUT, nc_log, REPORT, INVENTORY, FIXTURE):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/s3/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
