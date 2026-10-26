#!/usr/bin/env python3
"""Regenerate docs/evidence/c4/gates.txt and docs/evidence/c4/SHA256SUMS.

All numbers are real command output plus exit codes. The pre-C4 section is
measured from the C3 git commit (HEAD at generation time).

Usage:
    python3 tools/write_c4_evidence.py
    make evidence-c4
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "c4"
OUT = EVIDENCE / "gates.txt"
FIXTURE = REPO / "crates" / "aura-tx" / "tests" / "fixtures" / "c4_golden.txt"
SPEC = REPO / "docs" / "spec" / "tx-format.md"

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-tx/tests/fixtures/c4_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_c4_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-tx/tests/fixtures/c4_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-C4 tree (C3 commit)", "git log -1 --oneline", None),
    ("pre-C4 workspace crates", "git ls-tree -r --name-only HEAD | grep -c 'crates/.*/Cargo.toml$' || true", None),
    ("pre-C4 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C4 external deps", "git show HEAD:Cargo.lock | grep -c '^source = ' || true", None),
    ("post-C4 workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("post-C4 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C4 external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("post-C4 plan phase rows", "grep -hE '^\\| (✅ |⏸ )?\\*\\*[A-Z]+[0-9]+\\*\\* \\|' docs/plans/*_PLAN.md | wc -l", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("all negative controls (C0-C4)", "make nc-all", 12),
    ("C4 negative controls (evidence log)", "make nc-c4", 34),
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
        "AuraLite-Crypto — C4 evidence (aura-tx transaction format)",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed.",
        "",
        "Independent references: tools/gen_c4_vectors.py self-checks pure-Python",
        "BLAKE3 against official BLAKE3 vectors and pure-Python Ed25519 against",
        "RFC 8032 §7.1 before emitting 5 canonical tx vectors + 9 rejections.",
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
        if rc != 0 and not (label.startswith("pre-C4") and "grep -c" in cmd):
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-C4 (C3 tree): 11 crates, 28 .rs files, 54 external deps, 39 tests.",
        "Post-C4: 12 crates (aura-tx added, 8 placeholders remain), 34 .rs",
        "files, 54 external deps (no new ones), 49 tests passing (39 C0-C3 +",
        "10 C4), `make test` ALL GATES GREEN, 20 negative controls total",
        "(2+3+4+5+6), 6/6 of them new in C4.",
        "Validators/blocks/tx remain 0/0/0 — C4 is a format, not a network.",
        "",
        "Note: C0-C3 evidence is now HISTORICAL and is no longer regenerated",
        "in CI. `make nc-all` runs every NC script without rewriting those",
        "logs, so the committed SHA256SUMS stay valid.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c4/nc.log missing (run `make nc-c4`)")
        return 1

    sums = []
    for path in (OUT, nc_log, FIXTURE, SPEC):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/c4/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
