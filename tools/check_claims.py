#!/usr/bin/env python3
"""Docs drift guard for AuraLite-Crypto (C0).

The lesson of AuraLite-OS/MATURITY_AUDIT.md: a guarantee that exists only on
paper protects nothing. This script is part of `make test` and fails on:

  * a broken local markdown link;
  * a plan with no Status / Stage line;
  * a missing, extra or duplicated phase row (exact expected phase set);
  * a phase row without exactly 4 columns (Phase / Title / DoD / Gate);
  * a CHANGELOG with no C0 entry once code exists.

Usage:
    python3 tools/check_claims.py [--root /path/to/tree]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Exact phase ledger. If a phase is added, this table must be updated in the
# same patch as the plan (that IS the drift guard).
EXPECTED: dict[str, tuple[str, int]] = {
    "CORE_PLAN.md": ("C", 11),
    "STATE_PLAN.md": ("S", 8),
    "EXEC_PLAN.md": ("X", 10),
    "CONSENSUS_PLAN.md": ("B", 11),
    "NET_PLAN.md": ("N", 9),
    "NODE_PLAN.md": ("D", 9),
    "WALLET_PLAN.md": ("W", 8),
    "ECOSYSTEM_PLAN.md": ("E", 10),
    "ECONOMICS_PLAN.md": ("T", 9),
    "SECURITY_PLAN.md": ("H", 10),
    "PERF_PLAN.md": ("P", 7),
    "OPS_PLAN.md": ("O", 10),
}

LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
PHASE_ROW_RE = re.compile(r"^\| (?:✅ |⏸ )?\*\*([A-Z]+)(\d+)\*\* \|", re.M)
ANY_PHASE_ROW_RE = re.compile(r"^\| (?:✅ |⏸ )?\*\*[A-Z]+\d+\*\* \|", re.M)


def check(root: Path) -> list[str]:
    errors: list[str] = []
    md_files = sorted(root.rglob("*.md"))
    if not md_files:
        return [f"no markdown files under {root}"]

    # 1. Local links.
    for path in md_files:
        for match in LINK_RE.finditer(path.read_text(encoding="utf-8")):
            target = match.group(1).split("#")[0].strip()
            if not target or target.startswith(("http://", "https://", "mailto:")):
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                errors.append(f"{path.relative_to(root)}: broken link -> {target}")

    # 2. Plan ledger.
    plans = root / "docs" / "plans"
    for name, (prefix, count) in EXPECTED.items():
        path = plans / name
        if not path.exists():
            errors.append(f"docs/plans/{name}: missing plan file")
            continue
        text = path.read_text(encoding="utf-8")
        if "**Status:**" not in text:
            errors.append(f"docs/plans/{name}: no Status line")
        if "**Stage:**" not in text:
            errors.append(f"docs/plans/{name}: no Stage line")

        rows = PHASE_ROW_RE.findall(text)
        numbers = [int(num) for pre, num in rows if pre == prefix]
        expected = set(range(count))
        missing = sorted(expected - set(numbers))
        extra = sorted(set(numbers) - expected)
        duplicates = sorted({n for n in numbers if numbers.count(n) > 1})
        if missing:
            errors.append(f"docs/plans/{name}: missing phase rows {[prefix + str(n) for n in missing]}")
        if extra:
            errors.append(f"docs/plans/{name}: unexpected phase rows {[prefix + str(n) for n in extra]}")
        if duplicates:
            errors.append(f"docs/plans/{name}: duplicate phase rows {[prefix + str(n) for n in duplicates]}")

        for line in text.splitlines():
            if ANY_PHASE_ROW_RE.match(line):
                cells = line.strip().strip("|").split("|")
                if len(cells) != 4:
                    errors.append(
                        f"docs/plans/{name}: phase row has {len(cells)} columns, expected 4: {line[:70]}"
                    )

    extra_plans = {p.name for p in plans.glob("*_PLAN.md")} - set(EXPECTED)
    if extra_plans:
        errors.append(f"unregistered plan files (update EXPECTED): {sorted(extra_plans)}")

    # 3. Docs sync with code.
    if (root / "Cargo.toml").exists():
        changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
        if "[C0" not in changelog:
            errors.append("CHANGELOG.md: no C0 entry although Cargo.toml exists (rule D9)")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=None, help="tree to check (default: repository root)")
    args = parser.parse_args()
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parents[1]

    errors = check(root)
    if errors:
        print(f"DOCS DRIFT GUARD: FAIL ({len(errors)} problem(s))")
        for error in errors:
            print(f"  - {error}")
        return 1

    total_phases = sum(count for _, count in EXPECTED.values())
    print(
        f"DOCS DRIFT GUARD: OK — {len(list(root.rglob('*.md')))} markdown files, "
        f"{len(EXPECTED)} plans, {total_phases} phase rows, all local links resolve"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
