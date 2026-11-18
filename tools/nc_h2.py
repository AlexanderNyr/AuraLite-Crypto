#!/usr/bin/env python3
"""H2 focused mutation controls; restore every mutated file exactly after
each test.

These controls test the gates, not the safety of the system. Never run while
another process is editing the same checkout. Model: tools/nc_h0.py.

Control set (docs/h2-crypto-review-plan.md §5):
  1. (named) shared Domain::tag() -> a signature passes under any domain and
     the h2 suite goes red: `H2: wrong-domain signature must be rejected`
  2. remove the canonical-S guard -> high-S vector red:
     `H2: high-S signature must be rejected`
  3. drop the chain binding -> cross-chain digest red:
     `H2: cross-chain digest must be chain-bound`
  4. delete a checklist Owner field -> `error: checklist item H2-SC07: missing owner`
  5. delete a checklist Evidence field -> `error: checklist item H2-SC08: missing evidence`
  6. delete a checklist Status field -> `error: checklist item H2-SC04: missing status`
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "h2-crypto-review-plan.md"
PRIMITIVES = ROOT / "crates" / "aura-types" / "src" / "primitives.rs"
ED25519 = ROOT / "crates" / "aura-crypto" / "src" / "ed25519.rs"
HASH = ROOT / "crates" / "aura-crypto" / "src" / "hash.rs"


def run_suite() -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["cargo", "test", "-p", "aura-crypto", "--test", "h2_crypto"],
        cwd=ROOT, text=True, capture_output=True, check=False)


def run_gate() -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, "tools/check_h2_review.py"],
                          cwd=ROOT, text=True, capture_output=True, check=False)


def mutate(text: str, pattern: str, repl: str) -> str:
    n = len(re.findall(pattern, text, flags=re.M | re.S))
    if n != 1:
        raise SystemExit(f"FAIL: mutation target not unique ({pattern!r})")
    return re.sub(pattern, repl, text, count=1, flags=re.M | re.S)


def control(label: str, path: Path, mutate_fn, expect: str,
            runner=run_gate) -> bool:
    original = path.read_text(encoding="utf-8")
    try:
        path.write_text(mutate_fn(original), encoding="utf-8")
        result = runner()
    finally:
        path.write_text(original, encoding="utf-8")
    output = result.stdout + result.stderr
    ok = result.returncode != 0 and expect in output
    print(f"{'PASS' if ok else 'FAIL'} {label}: {expect}")
    if not ok:
        print(output[-2500:])
    return ok


def main() -> int:
    checks: list[bool] = []

    checks.append(control(
        "NC: shared domain -> signature passes, suite red",
        PRIMITIVES,
        lambda t: mutate(
            t,
            r"(pub const fn tag\(&self\) -> &'static str \{\n        match self \{)"
            r"(.*?)"
            r"(\n        \}\n    \})",
            "\\1\n            _ => \"tx\",\\3"),
        "H2: wrong-domain signature must be rejected",
        run_suite,
    ))
    checks.append(control(
        "NC: canonical-S guard removed -> high-S red",
        ED25519,
        lambda t: mutate(
            t,
            r"    if !s_is_canonical\(signature\) \{\n"
            r"        return Err\(CryptoError::NonCanonicalSignature\);\n"
            r"    \}\n",
            ""),
        "H2: high-S signature must be rejected",
        run_suite,
    ))
    checks.append(control(
        "NC: chain binding dropped -> cross-chain red",
        HASH,
        lambda t: mutate(t, r"let prefix = domain\.prefix\(chain_id\);",
                         'let prefix = domain.prefix("");'),
        "H2: cross-chain digest must be chain-bound",
        run_suite,
    ))
    checks.append(control(
        "NC: checklist item without an owner -> gate red",
        DOC,
        lambda t: mutate(t, r"(### H2-SC07 — .*?)(- Owner: [^\n]*\n)", r"\1"),
        "error: checklist item H2-SC07: missing owner",
    ))
    checks.append(control(
        "NC: checklist item without evidence -> gate red",
        DOC,
        lambda t: mutate(t, r"(### H2-SC08 — .*?)(- Evidence: [^\n]*\n)", r"\1"),
        "error: checklist item H2-SC08: missing evidence",
    ))
    checks.append(control(
        "NC: checklist item without status -> gate red",
        DOC,
        lambda t: mutate(t, r"(### H2-SC04 — .*?)(- Status: [^\n]*\n)", r"\1"),
        "error: checklist item H2-SC04: missing status",
    ))

    passed = sum(checks)
    total = len(checks)
    print(f"RESULT: {passed}/{total} negative controls PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
