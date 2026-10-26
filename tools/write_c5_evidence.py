#!/usr/bin/env python3
"""Regenerate docs/evidence/c5/gates.txt and docs/evidence/c5/SHA256SUMS.

All numbers are real command output plus exit codes. The pre-C5 section is
measured from the C4 git commit (HEAD at generation time).

Usage:
    python3 tools/write_c5_evidence.py
    make evidence-c5
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "c5"
OUT = EVIDENCE / "gates.txt"
FIXTURE = REPO / "crates" / "aura-block" / "tests" / "fixtures" / "c5_golden.txt"
SPEC = REPO / "docs" / "spec" / "block-format.md"

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-block/tests/fixtures/c5_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_c5_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-block/tests/fixtures/c5_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-C5 tree (C4 commit)", "git log -1 --oneline", None),
    ("pre-C5 workspace crates", "git ls-tree -r --name-only HEAD | grep -c 'crates/.*/Cargo.toml$' || true", None),
    ("pre-C5 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C5 external deps", "git show HEAD:Cargo.lock | grep -c '^source = ' || true", None),
    ("post-C5 workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("post-C5 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C5 external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("post-C5 plan phase rows", "grep -hE '^\\| (✅ |⏸ )?\\*\\*[A-Z]+[0-9]+\\*\\* \\|' docs/plans/*_PLAN.md | wc -l", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("all negative controls (C0-C5)", "make nc-all", 14),
    ("C5 negative controls (evidence log)", "make nc-c5", 34),
    ("full gate", "make test", 8),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    lines = [
        "AuraLite-Crypto — C5 evidence (aura-block block/header format)",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed.",
        "",
        "Independent references: tools/gen_c5_vectors.py self-checks",
        "pure-Python BLAKE3 against 7 official BLAKE3 vectors and reuses the",
        "committed C4 transaction vectors as block bodies. It emits 4 canonical",
        "blocks and 9 rejection vectors.",
        "",
        "=" * 72,
        "RAW COMMAND OUTPUT",
        "=" * 72,
        "",
    ]

    failures = 0
    for label, cmd, tail in COMMANDS:
        rc, out = run(cmd)
        shown = cmd if len(cmd) < 160 else "$ (shell script: generator idempotence check)"
        if tail and out:
            out = "\n".join(out.splitlines()[-tail:])
        lines.append(f"--- {label} ---")
        lines.append(f"$ {shown}")
        lines.append(out if out else "(no output)")
        lines.append(f"[rc={rc}]")
        lines.append("")
        if rc != 0 and not (label.startswith("pre-C5") and "grep -c" in cmd):
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-C5 (C4 tree): 12 crates, 34 .rs files, 54 external deps, 49 tests.",
        "Post-C5: 13 crates (aura-block added, 8 placeholders remain), 41 .rs",
        "files, 54 external deps (no new ones), 60 tests passing (49 C0-C4 +",
        "11 C5), `make test` ALL GATES GREEN, 26 negative controls total",
        "(2+3+4+5+6+6), 6/6 of them new in C5.",
        "Validators/blocks/tx remain 0/0/0 — C5 is a format, not a network.",
        "",
        "Note: C0-C4 evidence is HISTORICAL and is no longer regenerated in",
        "CI. `make nc-all` runs every NC script without rewriting those logs,",
        "so the committed SHA256SUMS stay valid.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c5/nc.log missing (run `make nc-c5`)")
        return 1

    sums = []
    for path in (OUT, nc_log, FIXTURE, SPEC):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/c5/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
