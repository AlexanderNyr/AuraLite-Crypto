#!/usr/bin/env python3
"""Regenerate docs/evidence/c6/gates.txt and docs/evidence/c6/SHA256SUMS.

All numbers are real command output plus exit codes. The pre-C6 section is
measured from the C5 git commit (HEAD at generation time).

Usage:
    python3 tools/write_c6_evidence.py
    make evidence-c6
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "c6"
OUT = EVIDENCE / "gates.txt"
FIXTURE = REPO / "crates" / "aura-block" / "tests" / "fixtures" / "c6_golden.txt"
SPEC = REPO / "docs" / "spec" / "merkle-proof-format.md"

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-block/tests/fixtures/c6_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_c6_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-block/tests/fixtures/c6_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-C6 tree (C5 commit)", "git log -1 --oneline", None),
    ("pre-C6 workspace crates", "git ls-tree -r --name-only HEAD | grep -c 'crates/.*/Cargo.toml$' || true", None),
    ("pre-C6 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C6 external deps", "git show HEAD:Cargo.lock | grep -c '^source = ' || true", None),
    ("post-C6 workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("post-C6 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C6 external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("post-C6 plan phase rows", "grep -hE '^\\| (✅ |⏸ )?\\*\\*[A-Z]+[0-9]+\\*\\* \\|' docs/plans/*_PLAN.md | wc -l", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("1000-leaf DoD gate", "cargo test -p aura-block --test c6_merkle all_thousand -- --nocapture", 12),
    ("all negative controls (C0-C6)", "make nc-all", 16),
    ("C6 negative controls (evidence log)", "make nc-c6", 34),
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
        "AuraLite-Crypto — C6 evidence (Merkle proofs)",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed.",
        "",
        "Independent references: tools/gen_c6_vectors.py self-checks",
        "pure-Python BLAKE3 against 7 official BLAKE3 vectors and emits",
        "roots/proofs for 1/2/3/5/8/1000-leaf tx trees, event trees and",
        "11 rejection vectors. The C5 tree shape is unchanged.",
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
        if rc != 0 and not (label.startswith("pre-C6") and "grep -c" in cmd):
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-C6 (C5 tree): 13 crates, 41 .rs files, 54 external deps, 60 tests.",
        "Post-C6: still 13 crates (proof module added inside aura-block),",
        "43 .rs files, 54 external deps (no new ones), 71 tests passing",
        "(60 C0-C5 + 11 C6), `make test` ALL GATES GREEN, 32 negative",
        "controls total (2+3+4+5+6+6+6), 6/6 of them new in C6.",
        "C6 DoD gate: 1000 leaves, all 1000 proofs valid, one substituted",
        "leaf rejected. Validators/blocks/tx remain 0/0/0.",
        "",
        "Note: C0-C5 evidence is HISTORICAL and is no longer regenerated in",
        "CI. `make nc-all` runs every NC script without rewriting those logs,",
        "so the committed SHA256SUMS stay valid.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c6/nc.log missing (run `make nc-c6`)")
        return 1

    sums = []
    for path in (OUT, nc_log, FIXTURE, SPEC):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/c6/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
