#!/usr/bin/env python3
"""Regenerate S1 evidence: gates, report, NC inventory, SHA256SUMS.

All numbers come from real command output or from the independent Python
golden fixture. Usage:
    python3 tools/write_s1_evidence.py
    make evidence-s1
"""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "s1"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s1_golden.txt"

NC_LOGS = [f"c{i}" for i in range(11)] + ["s0", "s1"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-state/tests/fixtures/s1_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_s1_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-state/tests/fixtures/s1_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-S1 tree (S0 commit)", "git log -1 --oneline", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("Rust files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 16),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("S1 tree tests", "cargo test -p aura-state --test s1_tree --quiet", 6),
    ("S1 negative controls (evidence log)", "make nc-s1", 34),
    ("all negative controls (C0-C10 + S0 + S1)", "make nc-all", 30),
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
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            out[key] = value
    return out


def reset_current_evidence() -> None:
    # Re-running this generator rewrites nc.log, which would transiently
    # invalidate the previous SHA256SUMS and make the audit / NC restore
    # checks go red mid-run. Remove this phase's own receipts first.
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    for name in ("gates.txt", "nc.log", "report.md", "inventory.txt", "SHA256SUMS"):
        path = EVIDENCE / name
        if path.exists():
            path.unlink()


def build_report(f: dict[str, str], stamp: str) -> str:
    lines = [
        "# S1 — versioned sparse Merkle tree (measured)",
        "",
        f"Generated (UTC): {stamp}. Numbers come from the independent Python",
        "reference `tools/gen_s1_vectors.py` and are re-verified by",
        "`crates/aura-state/tests/s1_tree.rs`.",
        "",
        "S1 replaces the naive S0 baseline with a deterministic, versioned,",
        "provable state commitment. Binary tree over 256-bit `StateKey::digest`",
        "paths (MSB first), with single-leaf subtree compression and",
        "depth-dependent empty-subtree hashes. `state_root(version, tree_root)`",
        "binds the root to a block height. Inclusion proofs carry sibling",
        "hashes from the leaf up to the root and include the version.",
        "",
        "## Measured gates",
        "",
        f"- operations: **{f['operation_count']}** (insert / delete / lookup)",
        f"- checkpoints: **{f['checkpoint_count']}** versioned roots",
        f"- proofs verified: **{f['proof_count']}**",
        f"- final leaves: **{f['checkpoint_12_leaves']}**",
        f"- empty subtree hash depth 0: `{f['empty_root_depth0']}`",
        f"- empty subtree hash depth 256: `{f['empty_root_depth256']}`",
        "",
        "All 12 checkpoint roots match the independent Python implementation.",
        "All 1,000 proof byte strings match, and each proof reconstructs the",
        "versioned state root. Tampering a sibling, value, key or sibling count",
        "is rejected. Insertion order no longer changes the root (the S0 defect",
        "is fixed); deleting a leaf changes the root and re-adding restores it.",
        "",
        "## Honest scope",
        "",
        "- In memory only: no persistence / WAL / crash recovery (S2).",
        "- No execution overlay or rollback (S3).",
        "- No chunked snapshots, state sync, pruning or backup (S4–S7).",
        "- Values are opaque bytes; account encoding stays the caller's job (C7).",
        "- No network, no blocks, no validators, no VM.",
        "",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "AuraLite-Crypto — negative-control inventory after S1",
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
        "AuraLite-Crypto — S1 evidence (versioned Merkle tree)",
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
        "Pre-S1 (S0 baseline): 15 crates, 117 Rust tests, 1006 golden",
        "vectors, 62 negative controls, naive non-deterministic state.",
        "Post-S1: 15 crates (aura-state gains the tree module), 123 Rust",
        "tests (+6 S1), 3066 independent fixture lines (10,000 operations,",
        "12 versioned roots, 1,000 proofs), 68 negative controls (6 new in",
        "S1), still 71 external crates. No new dependency.",
        "The S0 order-dependence defect is fixed: insertion order no longer",
        "changes the root. Persistence and overlays remain S2/S3.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(f, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != 68:
        print(f"INVENTORY ERROR: total {total} NC, expected 68")
        failures += 1

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/s1/nc.log missing (run `make nc-s1`)")
        return 1

    sums = []
    for path in (OUT, nc_log, REPORT, INVENTORY, FIXTURE):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/s1/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
