#!/usr/bin/env python3
"""Regenerate docs/evidence/review-w1-source-drift.txt from the tiers.

Replicates audit_closeout.verify_evidence_checksums' expected set: for
every checksum row in every docs/evidence/**/SHA256SUMS, resolve the row
name against the repo root; if the file's current sha differs from the
row sha and the name is a repo path (not docs/evidence/...), emit one
`HIST CUR name` ledger row. Rows are deduplicated and sorted.
"""
import hashlib
import pathlib
import re

REPO = pathlib.Path(__file__).resolve().parents[1]
LEDGER = REPO / "docs" / "evidence" / "review-w1-source-drift.txt"
SHA_LINE = re.compile(r"^([0-9a-f]{64}) [ *](.+)$")

rows = set()
for sums in sorted((REPO / "docs" / "evidence").rglob("SHA256SUMS")):
    for line in sums.read_text(encoding="utf-8").splitlines():
        m = SHA_LINE.fullmatch(line)
        if not m:
            continue
        hist, name = m.groups()
        if name.startswith("docs/evidence/"):
            continue
        path = REPO / name
        if not path.is_file():
            print("MISSING", name)
            continue
        cur = hashlib.sha256(path.read_bytes()).hexdigest()
        if cur != hist:
            rows.add((name, hist, cur))

lines = [f"{hist} {cur} {name}" for name, hist, cur in sorted(rows)]
LEDGER.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"drift ledger: {len(lines)} rows")
