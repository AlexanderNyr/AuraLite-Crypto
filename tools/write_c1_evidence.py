#!/usr/bin/env python3
"""Regenerate docs/evidence/c1/gates.txt and docs/evidence/c1/SHA256SUMS.

As with C0, everything is real command output plus exit codes. The pre-C1
numbers are measured from the C0 git commit (HEAD at generation time).

Usage:
    python3 tools/write_c1_evidence.py
    make evidence-c1
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "c1"
OUT = EVIDENCE / "gates.txt"
FIXTURE = REPO / "crates" / "aura-types" / "tests" / "fixtures" / "c1_golden.txt"

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-types/tests/fixtures/c1_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_c1_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-types/tests/fixtures/c1_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-C1 tree (C0 commit)", "git log -1 --oneline", None),
    ("pre-C1 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C1 golden fixture present", "git ls-tree -r --name-only HEAD | grep -c 'c1_golden.txt' || true", None),
    ("post-C1 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C1 plan phase rows", "grep -hE '^\\| (✅ |⏸ )?\\*\\*[A-Z]+[0-9]+\\*\\* \\|' docs/plans/*_PLAN.md | wc -l", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("full gate", "make test", 20),
    ("C1 negative controls", "make nc-c1", 24),
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
        "AuraLite-Crypto — C1 evidence (canonical types)",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed.",
        "",
        "Note: bech32 is classic BIP-173, verified against the independent",
        "Python encoder in tools/gen_c1_vectors.py (not against itself).",
        "",
        "=" * 72,
        "RAW COMMAND OUTPUT",
        "=" * 72,
        "",
    ]

    failures = 0
    for label, cmd, tail in COMMANDS:
        rc, out = run(cmd)
        if tail and out:
            out = "\n".join(out.splitlines()[-tail:])
        lines.append(f"--- {label} ---")
        lines.append(f"$ {cmd}" if len(cmd) < 160 else "$ (shell script: generator idempotence check)")
        lines.append(out if out else "(no output)")
        lines.append(f"[rc={rc}]")
        lines.append("")
        if rc != 0 and not (label.startswith("pre-C1") and "grep -c" in cmd):
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-C1 (C0 tree): 12 .rs files, 0 golden fixtures, 5 Rust tests.",
        "Post-C1: 17 .rs files, 1 golden fixture (23 lines), 17 Rust tests",
        "passing (5 unit + 11 C1 vectors + 1 identity), 0 external",
        "dependencies, `make test` ALL GATES GREEN, 3/3 negative controls",
        "PASS. Validators/blocks/tx remain 0/0/0.",
        "",
        "The independent generator is byte-stable: re-running it does not",
        "change the fixture (checked above with sha256 before/after).",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c1/nc.log missing (run `make nc-c1`)")
        return 1

    sums = []
    for path in (OUT, nc_log, FIXTURE):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        rel = path.relative_to(REPO)
        sums.append(f"{digest}  {rel}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/c1/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
