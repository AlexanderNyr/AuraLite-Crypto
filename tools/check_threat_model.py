#!/usr/bin/env python3
"""H0 review gate: parse `docs/THREAT_MODEL.md` and enforce its schema.

Fails (exit 1, one `error: ...` line per violation) on: fewer than 30
scenarios, duplicate scenario ids, missing required fields (class, boundary,
assets, entry, impact, owner, mitigations), unknown class/boundary/asset
references, owners outside the roles list, empty mitigations, or any drift
from the pinned oracle output (tools/fixtures/h0_threat_model_vectors.json).

The named control (tools/nc_h0.py NC-1, "scenario without an owner") deletes
one Owner field and requires `error: scenario H0-S07: missing owner`.
Parsing here is implemented independently of tools/gen_h0_vectors.py: the
oracle extracts, this gate re-derives and compares.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "THREAT_MODEL.md"
VECTORS = ROOT / "tools" / "fixtures" / "h0_threat_model_vectors.json"
REQUIRED = ("class", "boundary", "assets", "entry", "impact", "owner", "mitigations")
COMPARE = ("title", "class", "boundary", "assets", "entry", "impact", "owner",
           "mitigations", "residual")
MIN_SCENARIOS = 30


def norm(s: str) -> str:
    return " ".join(s.split())


def section(text: str, header: str) -> str:
    m = re.search(rf"^## {re.escape(header)}[^\n]*\n(.*?)(?=^## |\Z)", text,
                  re.M | re.S)
    return m.group(1) if m else ""


def parse_doc(text: str):
    """Return (assets, adversaries, boundaries, roles, scenarios)."""
    assets = {f"A{n}" for n in
              re.findall(r"^### A(\d+) — ", section(text, "1. Assets"), re.M)}
    adversaries = re.findall(r"^### (ADV-[A-Z]+) — ",
                             section(text, "2. Adversaries"), re.M)
    boundaries = {f"B{n}" for n in
                  re.findall(r"^### B(\d+) — ",
                             section(text, "3. Trust boundaries"), re.M)}
    roles = [norm(m) for m in
             re.findall(r"^- ([^—\n]+?) — ",
                        section(text, "5. Owners and roles"), re.M)]
    scenarios = []
    body = section(text, "4. Scenarios")
    for m in re.finditer(
            r"^### (H0-S\d\d) — ([^\n]*)\n(.*?)(?=^### |^## |\Z)",
            body, re.M | re.S):
        sid, title, block = m.group(1), norm(m.group(2)), m.group(3)
        fields: dict[str, str] = {}
        cur: str | None = None
        for line in block.splitlines():
            if line.startswith("- "):
                key, _, val = line[2:].partition(": ")
                cur = key.strip().lower()
                fields[cur] = val.strip()
            elif line.strip() and cur:
                fields[cur] += " " + line.strip()
        rec = {"id": sid, "title": title}
        rec.update({k: norm(v) for k, v in fields.items()})
        scenarios.append(rec)
    return assets, adversaries, boundaries, roles, scenarios


def check(text: str) -> list[str]:
    assets, adversaries, boundaries, roles, scenarios = parse_doc(text)
    errors: list[str] = []
    if len(scenarios) < MIN_SCENARIOS:
        errors.append(f"error: {len(scenarios)} scenarios, minimum is {MIN_SCENARIOS}")
    seen: set[str] = set()
    for s in scenarios:
        sid = s["id"]
        if sid in seen:
            errors.append(f"error: duplicate scenario id {sid}")
        seen.add(sid)
        for f in REQUIRED:
            val = s.get(f)
            if val is None:
                errors.append(f"error: scenario {sid}: missing {f}")
            elif f == "mitigations" and val.strip() in ("-", ""):
                errors.append(f"error: scenario {sid}: empty mitigations")
        cls = s.get("class")
        if cls and cls not in adversaries:
            errors.append(f"error: scenario {sid}: unknown class {cls}")
        bnd = s.get("boundary")
        if bnd and bnd not in boundaries:
            errors.append(f"error: scenario {sid}: unknown boundary {bnd}")
        for tok in (s.get("assets") or "").split(","):
            tok = tok.strip()
            if tok and tok not in assets:
                errors.append(f"error: scenario {sid}: unknown asset {tok}")
        owner = s.get("owner")
        if owner and owner not in roles:
            errors.append(f"error: scenario {sid}: owner {owner} not in roles")

    # Stage 2: compare against the pinned oracle output.
    if not VECTORS.exists():
        errors.append("error: fixture missing; run make gen-h0-vectors")
    else:
        vec = json.loads(VECTORS.read_text(encoding="utf-8"))
        by_id = {s["id"]: s for s in vec.get("scenarios", [])}
        ids = {s["id"] for s in scenarios}
        for s in scenarios:
            v = by_id.get(s["id"])
            if v is None:
                errors.append(f"error: scenario {s['id']}: missing from vectors")
                continue
            for f in COMPARE:
                if s.get(f, "") != v.get(f, ""):
                    errors.append(f"error: scenario {s['id']}: {f} differs from vectors")
        for sid in by_id:
            if sid not in ids:
                errors.append(f"error: scenario {sid}: missing from document")
        for label, mine in (("roles", sorted(roles)),
                            ("adversaries", sorted(adversaries)),
                            ("boundaries", sorted(boundaries)),
                            ("assets", sorted(assets))):
            if mine != vec.get(label):
                errors.append(f"error: {label} differ from vectors")
    return errors


def main() -> int:
    text = DOC.read_text(encoding="utf-8")
    errors = check(text)
    if errors:
        print("\n".join(errors))
        print(f"THREAT MODEL: FAIL — {len(errors)} violation(s)")
        return 1
    _, _, _, _, scenarios = parse_doc(text)
    print(f"THREAT MODEL: OK — {len(scenarios)} scenarios, all owned")
    return 0


if __name__ == "__main__":
    sys.exit(main())
