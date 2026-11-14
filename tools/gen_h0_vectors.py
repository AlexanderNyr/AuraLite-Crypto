#!/usr/bin/env python3
"""Deterministic H0 oracle: extract the threat-model inventory from
`docs/THREAT_MODEL.md` into `tools/fixtures/h0_threat_model_vectors.json`.

Independent of `tools/check_threat_model.py`: this script derives the expected
records (assets, adversaries, boundaries, roles, the 30+ scenarios with their
fields) straight from the document text; the review gate re-derives the same
records with its own parser and compares. Regenerate after editing the threat
model (`make gen-h0-vectors`). Output is JSON with sorted keys and a trailing
newline so it diffs cleanly.
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "THREAT_MODEL.md"
OUT = ROOT / "tools" / "fixtures" / "h0_threat_model_vectors.json"


def squash(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def main() -> int:
    text = DOC.read_text(encoding="utf-8")

    def grab(header: str) -> str:
        hit = re.search(r"^## " + re.escape(header) + r"[^\n]*\n(.*?)(?=^## |\Z)",
                        text, flags=re.M | re.S)
        return hit.group(1) if hit else ""

    assets = sorted(set(re.findall(r"^### (A\d+) — ", grab("1. Assets"), re.M)))
    adversaries = sorted(re.findall(r"^### (ADV-[A-Z]+) — ", grab("2. Adversaries"), re.M))
    boundaries = sorted(set(re.findall(r"^### (B\d+) — ", grab("3. Trust boundaries"), re.M)))
    roles = sorted(squash(r) for r in
                   re.findall(r"^- (.+?) — .+$", grab("5. Owners and roles"), re.M))

    scenarios = []
    for block in re.split(r"(?=^### H0-S\d\d — )", grab("4. Scenarios"), flags=re.M):
        head = re.match(r"^### (H0-S\d\d) — (.+)$", block, flags=re.M)
        if not head:
            continue
        rec: dict[str, object] = {"id": head.group(1), "title": squash(head.group(2))}
        current = None
        for line in block.splitlines()[1:]:
            field = re.match(r"^- ([A-Za-z]+): (.*)$", line)
            if field:
                current = field.group(1).lower()
                rec[current] = field.group(2).strip()
            elif current and line.startswith("  ") and line.strip():
                rec[current] += " " + line.strip()
        scenarios.append({k: (squash(v) if isinstance(v, str) else v)
                          for k, v in rec.items()})
    scenarios.sort(key=lambda s: str(s["id"]))

    doc = {
        "format": "AURALITE-THREAT-VECTORS-V1",
        "source": "docs/THREAT_MODEL.md",
        "doc_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
        "counts": {
            "scenarios": len(scenarios),
            "assets": len(assets),
            "adversaries": len(adversaries),
            "boundaries": len(boundaries),
            "roles": len(roles),
        },
        "roles": roles,
        "adversaries": adversaries,
        "boundaries": boundaries,
        "assets": assets,
        "scenarios": scenarios,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)} ({len(scenarios)} scenarios, "
          f"{len(roles)} roles)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
