#!/usr/bin/env python3
"""Negative controls for C0.

A green `make test` means nothing unless it can be made red. This script
tamper-tests the two halves of the C0 gate and requires BOTH to fail:

  NC-1 (Rust): change `DENOM` from "ALCRY" to "NOPE" and run
       `cargo test -p aura-types`. The identity test must go red.
  NC-2 (docs): copy the docs into a temp tree, delete the C0 phase row from
       CORE_PLAN.md and run check_claims.py --root <temp>. It must go red.

The source file is always restored in a `finally` block. Output is captured by
`make nc-c0` into docs/evidence/c0/nc.log.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
LIB = REPO / "crates" / "aura-types" / "src" / "lib.rs"
CHECKER = REPO / "tools" / "check_claims.py"
DOC_FILES = ["README.md", "ROADMAP.md", "CHANGELOG.md", "CONTRIBUTING.md", "Cargo.toml"]
C0_ROW = re.compile(r"^\| (?:✅ |⏸ )?\*\*C0\*\* \|")


def run(cmd: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=cwd or REPO, capture_output=True, text=True)


def rust_nc() -> tuple[bool, str]:
    original = LIB.read_text(encoding="utf-8")
    tampered = original.replace('pub const DENOM: &str = "ALCRY";', 'pub const DENOM: &str = "NOPE";')
    if tampered == original:
        return False, "tamper target `DENOM` not found in lib.rs — NC is blind"
    try:
        LIB.write_text(tampered, encoding="utf-8")
        result = run(["cargo", "test", "-p", "aura-types", "--quiet"])
        went_red = result.returncode != 0
        detail = (
            f"cargo test -p aura-types -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        snippet = "\n".join((result.stdout + result.stderr).strip().splitlines()[-6:])
        return went_red, f"{detail}\n      {snippet}"
    finally:
        LIB.write_text(original, encoding="utf-8")


def docs_nc() -> tuple[bool, str]:
    with tempfile.TemporaryDirectory(prefix="aura-nc-") as tmp:
        root = Path(tmp)
        for name in DOC_FILES:
            shutil.copy2(REPO / name, root / name)
        shutil.copytree(REPO / "docs", root / "docs")

        plan = root / "docs" / "plans" / "CORE_PLAN.md"
        lines = plan.read_text(encoding="utf-8").splitlines()
        kept = [line for line in lines if not C0_ROW.match(line)]
        removed = len(lines) - len(kept)
        if removed != 1:
            return False, f"expected to remove exactly 1 C0 row, removed {removed}"
        plan.write_text("\n".join(kept) + "\n", encoding="utf-8")

        result = run([sys.executable, str(CHECKER), "--root", str(root)])
        went_red = result.returncode != 0
        detail = (
            f"check_claims.py --root <temp> -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        snippet = "\n".join(result.stdout.strip().splitlines()[:5])
        return went_red, f"{detail}\n      {snippet}"


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    print(f"C0 NEGATIVE CONTROLS — {stamp}")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    results = []
    for label, fn in (("NC-1 Rust identity", rust_nc), ("NC-2 docs drift guard", docs_nc)):
        ok, detail = fn()
        results.append(ok)
        print(f"{label}: {'PASS' if ok else 'FAIL'}")
        print(f"      {detail}")
        print()

    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    if passed != len(results):
        return 1

    restored = run(["cargo", "test", "-p", "aura-types", "--quiet"])
    print(f"restore check: cargo test -p aura-types -> rc={restored.returncode} "
          f"({'GREEN as expected' if restored.returncode == 0 else 'STILL RED — FAIL'})")
    return 0 if restored.returncode == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
