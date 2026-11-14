#!/usr/bin/env python3
"""H0 focused mutation controls; restore `docs/THREAT_MODEL.md` exactly after
each test.

These controls test the review gate, not the safety of the system. Never run
while another process is editing the same checkout. Model: tools/nc_w4.py.

Control set (docs/plans/SECURITY_PLAN.md row H0 + docs/THREAT_MODEL.md §6):
  1. scenario without an owner -> gate red (the named H0 control)
  2. duplicate scenario id -> gate red
  3. unknown adversary class -> gate red
  4. unknown trust boundary -> gate red
  5. empty mitigations -> gate red
  6. fewer than 30 scenarios -> gate red
"""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "THREAT_MODEL.md"


def run_gate() -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, "tools/check_threat_model.py"],
                          cwd=ROOT, text=True, capture_output=True, check=False)


def mutate(text: str, pattern: str, repl: str) -> str:
    n = len(re.findall(pattern, text, flags=re.M | re.S))
    if n != 1:
        raise SystemExit(f"FAIL: mutation target not unique ({pattern!r})")
    return re.sub(pattern, repl, text, count=1, flags=re.M | re.S)


def control(label: str, mutate_fn, expect: str) -> bool:
    original = DOC.read_text(encoding="utf-8")
    try:
        DOC.write_text(mutate_fn(original), encoding="utf-8")
        result = run_gate()
    finally:
        DOC.write_text(original, encoding="utf-8")
    output = result.stdout + result.stderr
    ok = result.returncode != 0 and expect in output
    print(f"{'PASS' if ok else 'FAIL'} {label}: {expect}")
    if not ok:
        print(output[-2500:])
    return ok


def block(sid: str) -> str:
    return rf"(### {sid} — .*?)(?=^### |^## |\Z)"


def main() -> int:
    checks: list[bool] = []

    checks.append(control(
        "NC: scenario without an owner -> gate red",
        lambda t: mutate(t, r"(### H0-S07 — .*?)(- Owner: [^\n]*\n)", r"\1"),
        "error: scenario H0-S07: missing owner",
    ))
    checks.append(control(
        "NC: duplicate scenario id -> gate red",
        lambda t: mutate(t, r"^### H0-S10 — ", "### H0-S09 — "),
        "error: duplicate scenario id H0-S09",
    ))
    checks.append(control(
        "NC: unknown adversary class -> gate red",
        lambda t: mutate(t, r"(### H0-S03 — .*?)(- Class: )[^\n]*",
                         r"\g<1>\g<2>ADV-BOGUS"),
        "error: scenario H0-S03: unknown class ADV-BOGUS",
    ))
    checks.append(control(
        "NC: unknown trust boundary -> gate red",
        lambda t: mutate(t, r"(### H0-S15 — .*?)(- Boundary: )[^\n]*",
                         r"\g<1>\g<2>B9"),
        "error: scenario H0-S15: unknown boundary B9",
    ))
    checks.append(control(
        "NC: empty mitigations -> gate red",
        lambda t: mutate(
            t,
            r"(### H0-S21 — .*?)(- Mitigations: )[^\n]*\n(?:  [^\n]*\n)*",
            r"\g<1>\g<2>-\n"),
        "error: scenario H0-S21: empty mitigations",
    ))

    def drop_three(t: str) -> str:
        for sid in ("H0-S32", "H0-S31", "H0-S30"):
            t = mutate(t, block(sid), "")
        return t

    checks.append(control(
        "NC: fewer than 30 scenarios -> gate red",
        drop_three,
        "error: 29 scenarios, minimum is 30",
    ))

    passed = sum(checks)
    total = len(checks)
    print(f"RESULT: {passed}/{total} negative controls PASS")
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
