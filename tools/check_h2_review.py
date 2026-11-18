#!/usr/bin/env python3
"""H2 review gate: parse the side-channel checklist in
`docs/h2-crypto-review-plan.md` and enforce its schema.

Fails (exit 1, one `error: ...` line per violation) on: fewer than 10 items,
duplicate or malformed ids, missing required fields (claim, owner, evidence,
status), owners outside the roles list, unknown status values, missing
evidence files for verified items, any drift from the pinned oracle snapshot
(`tools/fixtures/h2_review_checklist.json`), or a failed source-claim check:
the reviewed sources must actually contain the property each item claims.

Named control (tools/nc_h2.py NC-4): delete one Owner field and require
`error: checklist item H2-SC07: missing owner`. Parsing here is implemented
independently of tools/gen_h2_vectors.py: the oracle extracts, this gate
re-derives and compares.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "h2-crypto-review-plan.md"
SNAPSHOT = ROOT / "tools" / "fixtures" / "h2_review_checklist.json"

REQUIRED = ("claim", "owner", "evidence", "status")
ROLES = {"core maintainer", "crypto reviewer", "wallet maintainer",
         "release maintainer"}
STATUSES = {"verified", "planned", "deferred"}
MIN_ITEMS = 10

# Source-claim checks: every item must be backed by the property it claims.
# (file, required substrings) — the reviewed source must contain each.
CLAIMS: dict[str, list[tuple[str, list[str]]]] = {
    "H2-SC01": [("crates/aura-crypto/src/secret.rs",
                 ["ConstantTimeEq", "ct_eq"])],
    "H2-SC02": [("crates/aura-crypto/src/secret.rs", ["Zeroizing<[u8; 32]>"])],
    "H2-SC03": [("crates/aura-crypto/src/secret.rs", ["[REDACTED]"])],
    "H2-SC04": [("crates/aura-crypto/src/ed25519.rs",
                 ["s_is_canonical", "NonCanonicalSignature"]),
                ("crates/aura-crypto/src/error.rs", ["NonCanonicalSignature"])],
    "H2-SC05": [("crates/aura-types/src/primitives.rs",
                 ['"tx"', '"vote"', '"block"', '"contract"', '"genesis"'])],
    "H2-SC06": [("crates/aura-crypto/src/hash.rs", ["prefix(chain_id)"])],
    "H2-SC07": [("crates/aura-crypto/src/hash.rs", ["as u64).to_le_bytes"])],
    "H2-SC08": [("crates/aura-crypto/src/hash.rs", ["AuraLite-Crypto/crypto-v1"])],
    "H2-SC09": [(None, ["ML-DSA", "FIPS 204"])],
    "H2-SC10": [(None, ["SLH-DSA", "FIPS 205"])],
}
# Negative claims: the property must NOT be present.
FORBIDDEN: dict[str, list[tuple[str, list[str]]]] = {
    "H2-SC03": [("crates/aura-crypto/src/secret.rs",
                 ["impl fmt::Display for SecretBytes"])],
}


def norm(s: str) -> str:
    return " ".join(s.split())


def parse_doc(text: str) -> list[dict]:
    """Independent line-based extraction of the checklist items."""
    start = text.find("## 3. Side-channel checklist")
    if start < 0:
        return []
    body = text[start:]
    nxt = body.find("\n## ", 1)
    if nxt > 0:
        body = body[:nxt]
    items: list[dict] = []
    cur: dict | None = None
    for line in body.splitlines():
        header = re.match(r"^### (H2-SC\d\d) — (.+)$", line)
        if header:
            if cur is not None:
                items.append(cur)
            cur = {"id": header.group(1), "title": norm(header.group(2))}
            continue
        if cur is not None and line.startswith("- "):
            key, _, val = line[2:].partition(": ")
            cur[key.strip().lower()] = norm(val)
    if cur is not None:
        items.append(cur)
    return items


def check_claims(items: list[dict], text: str, errors: list[str]) -> None:
    ids = {item["id"] for item in items}
    for item_id, checks in sorted(CLAIMS.items()):
        if item_id not in ids:
            continue
        for rel, needles in checks:
            source = text if rel is None else (
                (ROOT / rel).read_text(encoding="utf-8") if (ROOT / rel).exists()
                else ""
            )
            for needle in needles:
                if needle not in source:
                    errors.append(
                        f"error: checklist item {item_id}: claim check failed, "
                        f"{needle!r} not found in {rel or 'plan doc'}"
                    )
    for item_id, checks in sorted(FORBIDDEN.items()):
        if item_id not in ids:
            continue
        for rel, needles in checks:
            source = (ROOT / rel).read_text(encoding="utf-8") if (ROOT / rel).exists() else ""
            for needle in needles:
                if needle in source:
                    errors.append(
                        f"error: checklist item {item_id}: forbidden pattern "
                        f"{needle!r} present in {rel}"
                    )


def main() -> int:
    text = DOC.read_text(encoding="utf-8")
    items = parse_doc(text)
    errors: list[str] = []

    if len(items) < MIN_ITEMS:
        errors.append(f"error: checklist: fewer than {MIN_ITEMS} items ({len(items)})")
    seen: set[str] = set()
    for item in items:
        item_id = item["id"]
        if item_id in seen:
            errors.append(f"error: duplicate checklist id {item_id}")
        seen.add(item_id)
        for field in REQUIRED:
            if not item.get(field):
                errors.append(f"error: checklist item {item_id}: missing {field}")
        owner = item.get("owner", "")
        if owner and owner not in ROLES:
            errors.append(
                f"error: checklist item {item_id}: owner {owner!r} not in roles"
            )
        status = item.get("status", "")
        if status and status not in STATUSES:
            errors.append(
                f"error: checklist item {item_id}: unknown status {status!r}"
            )
        evidence = item.get("evidence", "")
        if evidence and status == "verified" and not (ROOT / evidence).is_file():
            errors.append(
                f"error: checklist item {item_id}: evidence file missing: {evidence}"
            )

    check_claims(items, text, errors)

    # Drift against the pinned oracle snapshot.
    try:
        snapshot = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        errors.append(f"error: checklist snapshot unreadable: {exc}")
        snapshot = {"items": []}
    expected = {rec["id"]: rec for rec in snapshot.get("items", [])}
    actual = {item["id"]: item for item in items}
    for item_id in sorted(set(expected) | set(actual)):
        if item_id not in actual:
            errors.append(f"error: checklist drift: {item_id} missing from doc")
        elif item_id not in expected:
            errors.append(f"error: checklist drift: {item_id} missing from snapshot")
        elif expected[item_id] != actual[item_id]:
            errors.append(f"error: checklist drift: {item_id} differs from snapshot")

    if errors:
        for line in errors:
            print(line)
        print(f"CRYPTO REVIEW: FAIL — {len(errors)} problem(s)")
        return 1
    print(f"CRYPTO REVIEW: OK — {len(items)} checklist items, all claims verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
