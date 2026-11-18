#!/usr/bin/env python3
"""H7 OpSec checks (docs/h7-opsec-plan.md D-H7-2, D-H7-3, D-H7-6).

Fail-closed gates, wired into `make test` and CI:
1. secrets      — tools/scan_secrets.py over the tree (secret in git -> red)
2. ci-lint      — every workflow pins least-privilege `permissions:` and
                  the pinned toolchain
3. toolchain    — rust-toolchain.toml stays on the pinned channel
Prints `RESULT: 3/3 opsec checks PASS` on success.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from tools import scan_secrets  # noqa: E402

PINNED_TOOLCHAIN = "1.99.1"
WORKFLOWS = ROOT / ".github" / "workflows"


def check_secrets() -> tuple[bool, str]:
    findings = scan_secrets.scan_tree(ROOT)
    if findings:
        first = findings[0]
        return False, ("H7: secret in git must turn CI red (%s:%d %s)"
                       % (first[0], first[2], first[1]))
    return True, "no secret patterns in the tree"


def check_ci_lint() -> tuple[bool, str]:
    files = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    if not files:
        return False, "H7: CI must pin permissions and toolchain (no workflows)"
    for path in files:
        text = path.read_text(encoding="utf-8")
        if not re.search(r"^permissions:\s*$", text, re.MULTILINE):
            return False, ("H7: CI must pin permissions and toolchain "
                           "(%s has no permissions block)" % path.name)
        if PINNED_TOOLCHAIN not in text:
            return False, ("H7: CI must pin permissions and toolchain "
                           "(%s does not pin %s)" % (path.name, PINNED_TOOLCHAIN))
    return True, "workflows pin permissions and toolchain (%d files)" % len(files)


def check_toolchain_pin() -> tuple[bool, str]:
    path = ROOT / "rust-toolchain.toml"
    text = path.read_text(encoding="utf-8") if path.exists() else ""
    if 'channel = "%s"' % PINNED_TOOLCHAIN not in text:
        return False, ("H7: toolchain must stay pinned "
                       "(rust-toolchain.toml is not on %s)" % PINNED_TOOLCHAIN)
    return True, "rust-toolchain.toml pinned to %s" % PINNED_TOOLCHAIN


def main() -> int:
    checks = [check_secrets, check_ci_lint, check_toolchain_pin]
    passed = 0
    for check in checks:
        ok, note = check()
        print("[%s] %s: %s" % ("PASS" if ok else "FAIL", check.__name__, note),
              flush=True)
        passed += bool(ok)
    print("RESULT: %d/%d opsec checks PASS" % (passed, len(checks)), flush=True)
    return 0 if passed == len(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
