#!/usr/bin/env python3
"""Verify that the C9 test rig is actually WIRED, not just present.

A test rig that exists but is never executed protects nothing. This script is
part of `make test` and fails when a required gate disappears from the
Makefile / CI, when proptest is no longer a dev-dependency, or when a
cargo-fuzz target file is missing.

Usage:
    python3 tools/check_rig.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]


def check() -> list[str]:
    errors: list[str] = []
    makefile = (REPO / "Makefile").read_text(encoding="utf-8")
    ci = (REPO / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    manifest = (REPO / "crates" / "aura-testkit" / "Cargo.toml").read_text(encoding="utf-8")
    root_manifest = (REPO / "Cargo.toml").read_text(encoding="utf-8")

    required_in_makefile = [
        "test-core:",
        "fuzz-smoke:",
        "tools/count_vectors.py",
        "tools/gen_c9_vectors.py",
        "aura-fuzz",
        "make nc-all",
    ]
    for needle in required_in_makefile:
        if needle not in makefile:
            errors.append(f"Makefile: missing '{needle}'")

    for needle in ["make test-core", "make fuzz-smoke", "make nc-all"]:
        if needle not in ci:
            errors.append(f"CI: missing '{needle}'")

    if "proptest" not in manifest:
        errors.append("aura-testkit/Cargo.toml: proptest dev-dependency missing")
    if "crates/aura-testkit" not in root_manifest or "exclude = [\"fuzz\"]" not in root_manifest:
        errors.append("Cargo.toml: aura-testkit member or fuzz exclusion missing")

    fuzz_targets = sorted((REPO / "fuzz" / "fuzz_targets").glob("fuzz_*.rs"))
    if len(fuzz_targets) < 2:
        errors.append("fuzz/: need at least a codec and a tx libFuzzer target")
    for name in ("fuzz_codec.rs", "fuzz_tx.rs"):
        if not (REPO / "fuzz" / "fuzz_targets" / name).exists():
            errors.append(f"fuzz/: missing {name}")

    props = REPO / "crates" / "aura-testkit" / "tests" / "properties.rs"
    if props.exists():
        text = props.read_text(encoding="utf-8")
        cases = len(re.findall(r"#\[test\]", text))
        if cases < 3:
            errors.append(f"properties.rs: only {cases} proptest properties")
    else:
        errors.append("properties.rs missing")

    for script in ("gen_c9_vectors.py", "count_vectors.py", "nc_c9.py"):
        if not (REPO / "tools" / script).exists():
            errors.append(f"tools/{script} missing")

    # CI #1 failed because an evidence generator called `git log HEAD~1` in a
    # shallow clone. Every history lookup must have a fallback to HEAD.
    for script in sorted((REPO / "tools").glob("write_*_evidence.py")):
        for number, line in enumerate(script.read_text(encoding="utf-8").splitlines(), 1):
            stripped = line.strip()
            if stripped.startswith("#"):
                continue
            if "HEAD~1" in line and "||" not in line:
                errors.append(
                    f"{script.name}:{number}: shallow-unsafe git history lookup "
                    "(add `|| git log -1 --oneline` fallback)"
                )

    return errors


def main() -> int:
    errors = check()
    if errors:
        print(f"TEST RIG WIRING: FAIL ({len(errors)} problem(s))")
        for error in errors:
            print(f"  - {error}")
        return 1
    targets = len(list((REPO / "fuzz" / "fuzz_targets").glob("fuzz_*.rs")))
    print(
        f"TEST RIG WIRING: OK — test-core + fuzz-smoke + vector inventory + "
        f"proptest + {targets} libFuzzer targets are wired into CI"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
