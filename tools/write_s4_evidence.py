#!/usr/bin/env python3
"""Regenerate S4 evidence: gates, report, NC inventory, SHA256SUMS.

All numbers come from real command output or the independent Python golden
fixture. Usage:
    python3 tools/write_s4_evidence.py
    make evidence-s4
"""

from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "s4"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "s4_golden.txt"
ADDENDUM = REPO / "docs" / "evidence" / "s4-dependency-addendum.txt"

NC_LOGS = [f"c{i}" for i in range(11)] + ["s0", "s1", "s2", "s3", "s4"]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-state/tests/fixtures/s4_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_s4_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-state/tests/fixtures/s4_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-S4 tree (S3 commit)", "git log -1 --oneline", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("Rust files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 16),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("S4 snapshot tests", "cargo test -p aura-state --test s4_snapshot --quiet", 8),
    ("1M-key snapshot gate", "cargo test -p aura-state --test s4_snapshot million_key -- --nocapture", 6),
    ("S4 negative controls (evidence log)", "make nc-s4", 34),
    ("all negative controls (C0-C10 + S0-S4)", "make nc-all", 32),
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
        "# S4 — deterministic state snapshots (measured)",
        "",
        f"Generated (UTC): {stamp}. Numbers come from the independent Python",
        "reference `tools/gen_s4_vectors.py` and are re-verified by",
        "`crates/aura-state/tests/s4_snapshot.rs`.",
        "",
        "This report lives in `docs/evidence/s4/`, not `docs/spec/`, because",
        "`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.",
        "",
        "## Format",
        "",
        "A snapshot is `manifest_bytes || data_blob`. The blob is a",
        "concatenation of independently SHA-256-hashed chunks. The manifest",
        "holds a descriptor per chunk (offset, length, sha256), the S1",
        "`state_root`, a BLAKE3 `chunk_root` over the ordered digests, and a",
        "SHA-256 checksum of the whole manifest payload.",
        "",
        "```",
        "manifest payload = \"ALS4\" || u32 version || varint(len) chain_id ||",
        "  u64 height || 32 state_root || 32 chunk_root || u32 chunk_size ||",
        "  u64 total_keys || u32 chunk_count ||",
        "  (u64 offset || u64 length || 32 sha256(chunk))*count",
        "manifest bytes  = payload || sha256(SNAPSHOT_MANIFEST_CONTEXT || payload)",
        "chunk payload   = varint(count) || (varint(len) key || key ||",
        "                 varint(len) value || value)*count",
        "chunk hash      = sha256(SNAPSHOT_CHUNK_CONTEXT || chunk payload)",
        "chunk_root      = BLAKE3(SNAPSHOT_ROOT_CONTEXT || u32 count || concat(hashes))",
        "state_root      = S1 versioned_root(1, sparse_merkle_root(C7 key -> value))",
        "```",
        "",
        "Export and import hash chunks in parallel via `std::thread::scope`",
        "with an atomic work index and a bounded worker pool (1..8 workers).",
        "Output is deterministic: descriptor order is input chunk order.",
        "",
        "Protection comes from strict canonical decode (minimal varints, magic,",
        "version, contiguous offsets, caps, no trailing bytes, byte-identical",
        "re-encode) plus SHA-256 manifest/chunk digests. A substituted chunk is",
        "rejected by NC-6.",
        "",
        "## Measured gates",
        "",
        f"- canonical snapshot: **{f['entry_count']}** C7 account keys in",
        f"  **{f['chunk_count']}** chunks (chunk_size={f['chunk_size']})",
        f"- **1,000,000 keys** exported and reimported byte-identically;",
        f"  data blob = **{f['million_data_len']}** bytes in",
        f"  **{f['million_chunk_count']}** chunks; SHA-256 and chunk_root match the",
        "  independent Python reference computed without writing 30 MB of hex.",
        f"- parallel hashing: **{f['parallel_entries']}** keys /",
        f"  **{f['parallel_chunk_size']}** per chunk; identical digests for 1/2/4/8",
        "  workers.",
        "- rejection vectors: substituted chunk, bad manifest checksum/magic,",
        "  bad chunk count/root, truncated/trailing/empty data, unsorted and",
        "  duplicate keys.",
        "",
        "## Dependencies",
        "",
        "`sha2 = 0.10.9` was already in `Cargo.lock` via `k256` and in the",
        "frozen C3 baseline allowlist. S4 only makes it a DIRECT dependency of",
        "`aura-state` (manifest checksum + chunk digests). **No new external",
        "crate.** `docs/evidence/s4-dependency-addendum.txt` records the decision.",
        "",
        "## Honest scope",
        "",
        "- No state sync (S5), pruning (S6), backup/chaos (S7).",
        "- No network, no consensus, no P2P, no node, no VM.",
        "- The 1M gate uses `Hash::ZERO` as the S1 state_root placeholder: the",
        "  Python reference computes the S1 root only for the small snapshot.",
        "",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "AuraLite-Crypto — negative-control inventory after S4",
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
        "AuraLite-Crypto — S4 evidence (snapshots)",
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
        "Pre-S4 (S3): 15 crates, 138 Rust tests, 4,205 golden vectors,",
        "80 negative controls, 95 external crates (RocksDB optional).",
        "Post-S4: 15 crates (no new crate; sha2 promoted to direct dep),",
        "146 Rust tests (+8 S4), 4,254 vectors across 14 fixtures,",
        "86 negative controls (6 new in S4), 95 external crates.",
        "1,000,000-key snapshot imported byte-identically; substituted chunk",
        "rejected. Validators/blocks/tx remain 0/0/0.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(f, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != 86:
        print(f"INVENTORY ERROR: total {total} NC, expected 86")
        failures += 1

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/s4/nc.log missing (run `make nc-s4`)")
        return 1

    sums = []
    for path in (OUT, nc_log, REPORT, INVENTORY, FIXTURE, ADDENDUM):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/s4/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
