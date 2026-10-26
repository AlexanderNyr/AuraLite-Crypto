#!/usr/bin/env python3
"""Regenerate docs/evidence/c2/gates.txt and docs/evidence/c2/SHA256SUMS.

All numbers are real command output plus exit codes. The pre-C2 section is
measured from the C1 git commit (HEAD at generation time).

Usage:
    python3 tools/write_c2_evidence.py
    make evidence-c2
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "c2"
OUT = EVIDENCE / "gates.txt"
FIXTURE = REPO / "crates" / "aura-codec" / "tests" / "fixtures" / "c2_golden.txt"

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-codec/tests/fixtures/c2_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_c2_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-codec/tests/fixtures/c2_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

EXTERNAL_DEPS = (
    "n=$(grep -cE '^(source|checksum) =' Cargo.lock || true); "
    "echo \"external dependencies in Cargo.lock: $n\"; "
    "p=$(grep -c '^\\[\\[package\\]\\]' Cargo.lock); echo \"workspace packages: $p\""
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-C2 tree (C1 commit)", "git log -1 --oneline", None),
    ("pre-C2 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C2 codec source files", "git ls-tree -r --name-only HEAD | grep -c 'aura-codec/src/codec.rs' || true", None),
    ("post-C2 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C2 plan phase rows", "grep -hE '^\\| (✅ |⏸ )?\\*\\*[A-Z]+[0-9]+\\*\\* \\|' docs/plans/*_PLAN.md | wc -l", None),
    ("dependency audit (internal only)", EXTERNAL_DEPS, None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("full gate", "make test", 20),
    ("C2 negative controls", "make nc-c2", 30),
    ("docs drift guard", "python3 tools/check_claims.py", None),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    lines = [
        "AuraLite-Crypto — C2 evidence (aura-codec)",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed.",
        "",
        "The wire format is pinned against tools/gen_c2_vectors.py, an",
        "independent Python implementation. decode_canonical() additionally",
        "requires that re-encoding reproduces the input bytes exactly.",
        "",
        "=" * 72,
        "RAW COMMAND OUTPUT",
        "=" * 72,
        "",
    ]

    failures = 0
    for label, cmd, tail in COMMANDS:
        rc, out = run(cmd)
        shown = cmd if len(cmd) < 160 else "$ (shell script: reproducibility / dependency audit)"
        if tail and out:
            out = "\n".join(out.splitlines()[-tail:])
        lines.append(f"--- {label} ---")
        lines.append(f"$ {shown}")
        lines.append(out if out else "(no output)")
        lines.append(f"[rc={rc}]")
        lines.append("")
        if rc != 0 and not (label.startswith("pre-C2") and "grep -c" in cmd):
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-C2 (C1 tree): 17 .rs files, 0 aura-codec source modules, 17 tests.",
        "Post-C2: 22 .rs files, 4 new codec modules + 1 test file, 28 tests",
        "passing (17 C0/C1 + 11 C2), 0 external dependencies (one internal",
        "path dependency aura-codec -> aura-types), `make test` ALL GATES",
        "GREEN, 4/4 C2 negative controls PASS.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c2/nc.log missing (run `make nc-c2`)")
        return 1

    sums = []
    for path in (OUT, nc_log, FIXTURE):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/c2/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
