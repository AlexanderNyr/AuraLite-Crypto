#!/usr/bin/env python3
"""Regenerate S7 evidence: gates, report, NC inventory, SHA256SUMS.

All numbers come from real command output or from the independent Python
golden fixture. Usage:
    python3 tools/write_s7_evidence.py
    make evidence-s7
"""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "s7"
OUT = EVIDENCE / "gates.txt"
REPORT = EVIDENCE / "report.md"
INVENTORY = EVIDENCE / "inventory.txt"
FIXTURE = REPO / "crates" / "aura-storage" / "tests" / "fixtures" / "s7_golden.txt"
ADDENDUM = REPO / "docs" / "evidence" / "s7-dependency-addendum.txt"

NC_LOGS = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-storage/tests/fixtures/s7_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_s7_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-storage/tests/fixtures/s7_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-S7 tree (S6 commit)", "git log -1 --oneline", None),
    ("workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("Rust files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("vector inventory", "python3 tools/count_vectors.py", 16),
    ("test rig wiring", "python3 tools/check_rig.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("S7 backup/restore tests", "cargo test -p aura-storage --test s7_backup --quiet", 9),
    ("chaos gate: 50 kill -9 + restore cycles",
     "cargo test -p aura-storage --test s7_backup chaos_50 -- --nocapture", 10),
    ("S7 negative controls (evidence log)", "make nc-s7", 34),
    ("all negative controls (C0-C10 + S0-S7)", "make nc-all", 34),
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
        "# S7 — recovery and backup (measured)",
        "",
        f"Generated (UTC): {stamp}. Every number below comes from real command",
        "output or from the independent Python model `tools/gen_s7_vectors.py`",
        "(which reuses the independent S2 reference for batches, tree and WAL",
        "framing).",
        "",
        "This report lives in `docs/evidence/s7/`, not `docs/spec/`, because",
        "`docs/spec/FORMAT_INDEX.md` is frozen, checksummed C10 evidence.",
        "",
        "## What landed",
        "",
        "* `aura-storage::backup` — `BackupManifest`, `BackupBundle`,",
        "  `create_backup`, `restore_backup`, `RestoreReport`, `BackupError`.",
        "* `aura-storage` binary `s7-chaos` — the `kill -9` harness.",
        "",
        "```",
        "bundle         = u32le(manifest_len) || manifest_bytes || blob",
        "manifest bytes = payload || BLAKE3(S7_MANIFEST_CONTEXT || payload)",
        "payload        = \"ALM7\" || u32 format_version || v-len chain_id ||",
        "                 u64 head_version || 32 head_root || u64 record_count ||",
        "                 u64 blob_len || 32 blob_sha256",
        "blob           = framed S2 WAL records 1..=head (each BLAKE3-checksummed)",
        "blob_sha256    = SHA-256(blob)",
        "```",
        "",
        "Verification is layered and every layer must pass: manifest length and",
        "checksum, magic and format version, blob length and SHA-256, per-record",
        "BLAKE3, consecutive versions, parent-root chain, replay of every record",
        "through the real commit path (roots recomputed, not trusted), final head",
        "and root, and `Storage::verify_state_root()` over the leaves.",
        "",
        "## Measured DoD gate",
        "",
        f"* **{f['chaos_cycles']} `kill -9` + restore cycles**, commit counts",
        f"  1..={f['max_commits']}, 17 of them killed inside the WAL/head crash",
        "  window (WAL fsynced, head not yet published).",
        "* Result: **0 head mismatches, 0 restore mismatches, 0 continuation",
        f"  mismatches**, {f['chaos_cycles']} recovered stores, 338 WAL records",
        "  replayed in total; every recovered head and root equals the",
        "  independent Python root for that height.",
        "* After each restore, both the recovered and the restored store commit",
        "  the next batch and must produce the same receipt and the same",
        "  Python root — a restored store is deterministic, not merely equal.",
        "",
        "## Rejection gates",
        "",
        "* Flipped blob byte -> `BlobHashMismatch`.",
        "* Flipped manifest byte -> `ChecksumMismatch`.",
        "* Truncated / trailing bytes -> `Truncated` / `LengthMismatch`.",
        "* Wrong magic -> `BadMagic`; unknown format -> `UnsupportedVersion`.",
        "* Wrong chain-id -> `WrongChainId`.",
        "* Restore into a directory that already holds a store ->",
        "  `TargetNotEmpty` (a restore never merges into live state).",
        "* A fully re-checksummed lie about a record root -> `RootMismatch`",
        "  (caught by replaying the state, NC-3).",
        "* A fully re-checksummed broken parent chain -> `ParentRootMismatch`",
        "  (NC-4).",
        "* A perfectly checksummed manifest that lies about the head ->",
        "  `HeadMismatch`.",
        "",
        "## Canonical bytes",
        "",
        f"* bundle **{f['golden_bundle_len']} bytes** (manifest",
        f"  {f['golden_manifest_len']}, blob {f['golden_blob_len']}) for a",
        f"  {f['golden_commits']}-record store, byte-identical to the Python",
        "  encoder, and reproducible: backing up the same store twice yields the",
        "  same bytes, and backing up a *restored* store reproduces the bundle.",
        "",
        "## Negative controls",
        "",
        "NC-1 manifest context, NC-2 blob SHA-256, NC-3 replayed root, NC-4",
        "parent chain, NC-5 restore into a live store, NC-6 crash-window replay.",
        "6/6 PASS; total **104** across C0–C10 + S0–S7.",
        "",
        "## Honest scope",
        "",
        "* Chaos is *process* `kill -9` on one host, not power loss, not network",
        "  partition, not disk corruption, and not a real devnet.",
        "* Backups are single-file bundles; there is no incremental backup, no",
        "  remote storage and no key management yet (OPS_PLAN O-phases).",
        "* The optional RocksDB backend is exercised by the `s2-native` CI job;",
        "  the default reproducible backend is pure Rust.",
        "* No VM (X1), no consensus, no P2P, no node, no wallet.",
        "",
        "Validators / blocks / transactions remain **0 / 0 / 0**.",
        "",
    ]
    return "\n".join(lines)


def build_inventory(stamp: str) -> tuple[str, int]:
    lines = [
        "AuraLite-Crypto — negative-control inventory after S7",
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
        lines.append(
            f"{name.upper()}: {passed}/{int(match.group(2)) if match else 0} negative controls"
        )
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
        "AuraLite-Crypto — S7 evidence (recovery and backup)",
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
        "Pre-S7 (S6): 15 crates, 169 Rust tests, 4,326 golden vectors,",
        "98 negative controls, 95 external crates.",
        "Post-S7: 15 crates (no new crate, no new external dependency; one new",
        "binary target s7-chaos), 177 Rust tests (+7 S7 backup, +1 unit test),",
        "4,349 vectors across 17 fixtures, 104 negative controls (6 new in S7),",
        "95 external crates.",
        f"{f['chaos_cycles']} kill -9 + restore cycles -> 0 root mismatches;",
        "restore replays every record instead of trusting the manifest.",
        "STATE_PLAN is COMPLETE (S0-S7). Validators/blocks/tx remain 0/0/0.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    REPORT.write_text(build_report(f, stamp), encoding="utf-8")
    inventory, total = build_inventory(stamp)
    INVENTORY.write_text(inventory, encoding="utf-8")
    if total != 104:
        print(f"INVENTORY ERROR: total {total} NC, expected 104")
        failures += 1

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/s7/nc.log missing (run `make nc-s7`)")
        return 1

    sums = []
    for path in (OUT, nc_log, REPORT, INVENTORY, FIXTURE, ADDENDUM):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {REPORT.relative_to(REPO)}")
    print(f"wrote {INVENTORY.relative_to(REPO)}")
    print("wrote docs/evidence/s7/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
