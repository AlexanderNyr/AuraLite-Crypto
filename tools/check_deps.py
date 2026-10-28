#!/usr/bin/env python3
"""Dependency gate for C3 (part of `make test`).

A lighter predecessor of `cargo deny` (H1). It fails on:

  * any external crate without a crates.io registry source or a checksum;
  * wildcard / caret / git dependency specifications;
  * a direct `serde` dependency in any workspace crate (rule D4);
  * drift between Cargo.lock and the frozen C3 baseline plus phase addenda
    (docs/spec/c3-dependency-allowlist.txt, *-dependency-addendum.txt);
  * a lock entry with an invalid sha256 checksum format.

H1 will add licence and advisory checks on top of this.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
SHA256 = re.compile(r"^[0-9a-f]{64}$")


def check() -> list[str]:
    errors: list[str] = []

    lock = tomllib.loads((REPO / "Cargo.lock").read_text(encoding="utf-8"))
    external: list[tuple[str, str]] = []
    for package in lock.get("package", []):
        if "source" not in package:
            continue
        name, version = package["name"], package["version"]
        external.append((name, version))
        if package["source"] != CRATES_IO:
            errors.append(f"{name} {version}: non-crates.io source {package['source']}")
        checksum = package.get("checksum", "")
        if not SHA256.fullmatch(checksum):
            errors.append(f"{name} {version}: missing/invalid sha256 checksum")
        if "git+" in package["source"] or "git+" in version:
            errors.append(f"{name} {version}: git dependency is forbidden")
    external.sort()

    root = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))
    workspace_deps = root.get("workspace", {}).get("dependencies", {})
    for name, spec in workspace_deps.items():
        if isinstance(spec, str):
            if "*" in spec or spec.startswith("^"):
                errors.append(f"[workspace.dependencies] {name}: unpinned '{spec}'")
        elif isinstance(spec, dict):
            if "git" in spec or "branch" in spec or "tag" in spec:
                errors.append(f"[workspace.dependencies] {name}: git dependency forbidden")
            version = spec.get("version")
            if version is not None and not version.startswith("="):
                errors.append(f"[workspace.dependencies] {name}: must use exact '=x.y.z', got '{version}'")
            if version is not None and "*" in version:
                errors.append(f"[workspace.dependencies] {name}: wildcard forbidden")

    for manifest in sorted((REPO / "crates").glob("*/Cargo.toml")):
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            for name, spec in data.get(section, {}).items():
                if name == "serde":
                    errors.append(f"{manifest.parent.name}: direct serde dependency forbidden (D4)")
                if isinstance(spec, str) and ("*" in spec or spec.startswith("^")):
                    errors.append(f"{manifest.parent.name}: unpinned {name} = '{spec}'")

    # The C3 baseline is frozen historical evidence (its checksum is pinned
    # by docs/evidence/c3/SHA256SUMS). Later phases record additive deltas.
    spec_dir = REPO / "docs" / "spec"
    evidence_dir = REPO / "docs" / "evidence"
    # S2+ addenda live under docs/evidence/ because docs/spec/FORMAT_INDEX.md
    # is frozen C10 evidence and must not gain new files.
    allowlist_paths = (
        [spec_dir / "c3-dependency-allowlist.txt"]
        + sorted(spec_dir.glob("*-dependency-addendum.txt"))
        + sorted(evidence_dir.glob("*-dependency-addendum.txt"))
    )
    allowed: list[tuple[str, str]] = []
    for allowlist_path in allowlist_paths:
        if not allowlist_path.exists():
            errors.append(f"{allowlist_path.relative_to(REPO)} is missing")
            continue
        for line in allowlist_path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" not in line:
                errors.append(f"allowlist: malformed line {line!r}")
                continue
            name, version = line.split("=", 1)
            allowed.append((name, version))
    if not errors:
        allowed.sort()
        if allowed != external:
            missing = sorted(set(external) - set(allowed))
            extra = sorted(set(allowed) - set(external))
            if missing:
                errors.append(f"allowlist drift: unrecorded {missing}")
            if extra:
                errors.append(f"allowlist drift: stale {extra}")

    return errors


def main() -> int:
    errors = check()
    if errors:
        print(f"DEPENDENCY GATE: FAIL ({len(errors)} problem(s))")
        for error in errors:
            print(f"  - {error}")
        return 1
    lock = tomllib.loads((REPO / "Cargo.lock").read_text(encoding="utf-8"))
    total = len(lock.get("package", []))
    external = sum(1 for p in lock.get("package", []) if "source" in p)
    print(
        f"DEPENDENCY GATE: OK — {total} lock packages "
        f"({total - external} local, {external} external, all checksummed + allowlisted)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
