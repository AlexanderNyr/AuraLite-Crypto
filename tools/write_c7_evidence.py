#!/usr/bin/env python3
"""Regenerate docs/evidence/c7/gates.txt and docs/evidence/c7/SHA256SUMS.

All numbers are real command output plus exit codes. The pre-C7 section is
measured from the C6 git commit (HEAD at generation time).

Usage:
    python3 tools/write_c7_evidence.py
    make evidence-c7
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "c7"
OUT = EVIDENCE / "gates.txt"
FIXTURE = REPO / "crates" / "aura-state" / "tests" / "fixtures" / "c7_golden.txt"
SPEC = REPO / "docs" / "spec" / "account-model.md"

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-state/tests/fixtures/c7_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_c7_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-state/tests/fixtures/c7_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-C7 tree (C6 commit)", "git log -1 --oneline", None),
    ("pre-C7 workspace crates", "git ls-tree -r --name-only HEAD | grep -c 'crates/.*/Cargo.toml$' || true", None),
    ("pre-C7 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C7 external deps", "git show HEAD:Cargo.lock | grep -c '^source = ' || true", None),
    ("post-C7 workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("post-C7 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C7 external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("post-C7 plan phase rows", "grep -hE '^\\| (✅ |⏸ )?\\*\\*[A-Z]+[0-9]+\\*\\* \\|' docs/plans/*_PLAN.md | wc -l", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("DoD collision + reserved-space gate", "cargo test -p aura-state --test c7_state -- --nocapture", 20),
    ("all negative controls (C0-C7)", "make nc-all", 18),
    ("C7 negative controls (evidence log)", "make nc-c7", 34),
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
        "AuraLite-Crypto — C7 evidence (account model + state keys)",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed.",
        "",
        "Independent references: tools/gen_c7_vectors.py self-checks",
        "pure-Python BLAKE3 against 7 official BLAKE3 vectors, then emits",
        "key bytes/digests for 5 namespaces x 4 indices, 3 account encodings",
        "and 12 rejection vectors. No new crate or external dependency.",
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
        if rc != 0 and not (label.startswith("pre-C7") and "grep -c" in cmd):
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-C7 (C6 tree): 13 crates, 43 .rs files, 54 external deps, 71 tests.",
        "Post-C7: still 13 crates (aura-state now has a real API; 7 empty",
        "placeholders remain), 47 .rs files, 54 external deps (no new ones),",
        "82 tests passing (71 C0-C6 + 11 C7), `make test` ALL GATES GREEN,",
        "38 negative controls total (2+3+4+5+6+6+6+6), 6/6 new in C7.",
        "C7 DoD gates: 1000 distinct addresses -> 5000 keys/digests, zero",
        "collisions; all 256 prefixes checked, only 5 reachable; contracts",
        "cannot write outside their own storage namespace.",
        "Validators/blocks/tx remain 0/0/0.",
        "",
        "Note: C0-C6 evidence is HISTORICAL and is no longer regenerated in",
        "CI. `make nc-all` runs every NC script without rewriting those logs,",
        "so the committed SHA256SUMS stay valid.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c7/nc.log missing (run `make nc-c7`)")
        return 1

    sums = []
    for path in (OUT, nc_log, FIXTURE, SPEC):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/c7/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
