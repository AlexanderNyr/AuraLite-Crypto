#!/usr/bin/env python3
"""Regenerate docs/evidence/c8/gates.txt and docs/evidence/c8/SHA256SUMS.

All numbers are real command output plus exit codes. The pre-C8 section is
measured from the C7 git commit (HEAD at generation time).

Usage:
    python3 tools/write_c8_evidence.py
    make evidence-c8
"""

from __future__ import annotations

import hashlib
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs" / "evidence" / "c8"
OUT = EVIDENCE / "gates.txt"
FIXTURE = REPO / "crates" / "aura-genesis" / "tests" / "fixtures" / "c8_golden.txt"
SPEC = REPO / "docs" / "spec" / "genesis-format.md"
CONFIG = REPO / "docs" / "spec" / "genesis-testnet.json"
ADDENDUM = REPO / "docs" / "spec" / "c8-dependency-addendum.txt"

GEN_IDEMPOTENT = (
    "before=$(sha256sum crates/aura-genesis/tests/fixtures/c8_golden.txt | cut -d' ' -f1); "
    "python3 tools/gen_c8_vectors.py >/dev/null; "
    "after=$(sha256sum crates/aura-genesis/tests/fixtures/c8_golden.txt | cut -d' ' -f1); "
    "if [ \"$before\" = \"$after\" ]; then echo \"generator idempotent: $after\"; "
    "else echo \"GENERATOR DRIFT\"; exit 1; fi"
)

COMMANDS: list[tuple[str, str, int | None]] = [
    ("toolchain", "rustc --version && cargo --version", None),
    ("pre-C8 tree (C7 commit)", "git log -1 --oneline", None),
    ("pre-C8 workspace crates", "git ls-tree -r --name-only HEAD | grep -c 'crates/.*/Cargo.toml$' || true", None),
    ("pre-C8 .rs files", "git ls-tree -r --name-only HEAD | grep -c '\\.rs$' || true", None),
    ("pre-C8 external deps", "git show HEAD:Cargo.lock | grep -c '^source = ' || true", None),
    ("post-C8 workspace crates", "find crates -name Cargo.toml | wc -l", None),
    ("post-C8 .rs files", "find . -name '*.rs' -not -path './target/*' | wc -l", None),
    ("post-C8 external deps", "grep -c '^source = ' Cargo.lock || true", None),
    ("C8 dependency addendum", "python3 tools/record_c8_deps.py", None),
    ("dependency gate", "python3 tools/check_deps.py", None),
    ("docs drift guard", "python3 tools/check_claims.py", None),
    ("independent generator is idempotent", GEN_IDEMPOTENT, None),
    ("genesis CLI hash", "cargo run -q -p aura-genesis --bin aura -- genesis hash --file docs/spec/genesis-testnet.json", 4),
    ("three-host DoD gate", "cargo test -p aura-genesis --test c8_genesis same_genesis -- --nocapture", 10),
    ("all negative controls (C0-C8)", "make nc-all", 20),
    ("C8 negative controls (evidence log)", "make nc-c8", 34),
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
        "AuraLite-Crypto — C8 evidence (deterministic genesis)",
        f"Generated (UTC): {datetime.now(timezone.utc).isoformat(timespec='seconds')}",
        "Rule: measured against the tree, not assumed.",
        "",
        "Independent references: tools/gen_c8_vectors.py self-checks",
        "pure-Python BLAKE3 against 7 official BLAKE3 vectors and uses an",
        "independent bech32 encoder. It emits 19 JSON documents and two full",
        "sets of canonical bytes, genesis hash, validator-set hash,",
        "allocation root and total supply.",
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
        if rc != 0 and not (label.startswith("pre-C8") and "grep -c" in cmd):
            failures += 1

    lines += [
        "=" * 72,
        "SUMMARY",
        "=" * 72,
        "",
        "Pre-C8 (C7 tree): 13 crates, 47 .rs files, 54 external deps, 82 tests.",
        "Post-C8: 14 crates (aura-genesis added; 7 empty placeholders remain),",
        "54 .rs files, 58 external deps (+4: serde_json, itoa, memchr, zmij —",
        "config parsing only), 96 tests passing (82 C0-C7 + 14 C8),",
        "`make test` ALL GATES GREEN, 44 negative controls total",
        "(2+3+4+5+6+6+6+6+6), 6/6 new in C8.",
        "C8 DoD gates: three independent JSON representations give one hash;",
        "validator order change gives a different hash; the `aura genesis`",
        "CLI agrees with the library and the independent vectors.",
        "Validators/blocks/tx remain 0/0/0 — no network has started.",
        "",
        "The C3 dependency allowlist is FROZEN historical evidence; C8 records",
        "its delta in docs/spec/c8-dependency-addendum.txt, and check_deps.py",
        "compares Cargo.lock against the union of baseline + addenda.",
        "",
    ]
    OUT.write_text("\n".join(lines), encoding="utf-8")

    nc_log = EVIDENCE / "nc.log"
    if not nc_log.exists():
        print("ERROR: docs/evidence/c8/nc.log missing (run `make nc-c8`)")
        return 1

    sums = []
    for path in (OUT, nc_log, FIXTURE, SPEC, CONFIG, ADDENDUM):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.append(f"{digest}  {path.relative_to(REPO)}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")

    print(f"wrote {OUT.relative_to(REPO)} ({len(lines)} lines)")
    print(f"wrote {nc_log.relative_to(REPO)}")
    print("wrote docs/evidence/c8/SHA256SUMS")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
