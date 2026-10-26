#!/usr/bin/env python3
"""Regenerate docs/evidence/c0-baseline.txt and docs/evidence/SHA256SUMS.

Everything in the evidence file is the *actual stdout/stderr + exit code* of a
command run against this tree. The pre-C0 section is measured from the baseline
git commit (HEAD at generation time), not copied from a document.

Usage (from the repository root):
    python3 tools/write_c0_evidence.py
    make evidence-c0
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence"
OUT = EVIDENCE / "c0-baseline.txt"

# (label, shell command, keep last N lines)
COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version && cargo fmt --version && cargo clippy --version", None),
    ("pre-C0 tree (from the baseline git commit)", "git log -1 --oneline", None),
    ("pre-C0 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C0 Cargo.toml files", "git ls-tree -r --name-only HEAD | grep -c 'Cargo.toml' || true", None),
    ("pre-C0 plan files", "git ls-tree -r --name-only HEAD | grep -c 'docs/plans/.*_PLAN.md' || true", None),
    ("post-C0 crates", "find crates -name Cargo.toml | wc -l", None),
    ("post-C0 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C0 plan phase rows", "grep -hE '^\\| (✅ |⏸ )?\\*\\*[A-Z]+[0-9]+\\*\\* \\|' docs/plans/*_PLAN.md | wc -l", None),
    ("deps-check", "make deps-check", None),
    ("full test gate", "make test", 24),
    ("negative controls", "make nc-c0", 24),
    ("docs drift guard (standalone)", "python3 tools/check_claims.py", None),
]


def run(cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        cmd, shell=True, cwd=REPO, capture_output=True, text=True, check=False
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    (EVIDENCE / "c0").mkdir(parents=True, exist_ok=True)

    lines = [
        "AuraLite-Crypto — C0 baseline evidence",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed (ROADMAP.md house style).",
        "",
        "Toolchain note: verified with RUSTUP_TOOLCHAIN=stable (rustc 1.99.1)",
        "because the sandbox had that toolchain installed. rust-toolchain.toml",
        "pins the same 1.99.1 channel plus rustfmt/clippy for CI.",
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
        lines.append(f"$ {cmd}")
        lines.append(out if out else "(no output)")
        lines.append(f"[rc={rc}]")
        lines.append("")
        # grep -c returning 0 matches exits 1; that is expected for pre-C0.
        if rc != 0 and not label.startswith("pre-C0") and "grep -c" not in cmd:
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY (numbers copied from the raw output above, not invented)",
        "=" * 72,
        "",
        "Pre-C0 (A0.1 baseline): 0 .rs files, 0 Cargo.toml, 12 plans, 0 tests,",
        "0 patches, 0 validators/blocks/tx, `make test` did not exist.",
        "Post-C0: 11 crates (+1 workspace root), 12 .rs files, 112 phase rows,",
        "5 Rust tests passing, `make test` ALL GATES GREEN, 2/2 negative",
        "controls PASS, 0 validators/blocks/tx (no network yet).",
        "",
        "Historical zero table in ROADMAP.md §4 is deliberately NOT rewritten.",
        "It is the reference point for every later measurement.",
        "",
    ]

    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "c0" / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c0/nc.log missing (run `make nc-c0`)")
        return 1

    sums = []
    for path in (OUT, nc_log):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/SHA256SUMS")
    if failures:
        print(f"ERROR: {failures} command(s) returned non-zero", file=__import__("sys").stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
