#!/usr/bin/env python3
"""Fail-closed test for Python's -O / -OO stripping the W1 oracle assertions."""
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ORACLE = ROOT / "tools/w1_interop_reference.py"
MESSAGE = "W1 interop checks require Python assertions; disable optimization"


def invoke(path: Path, optimization: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, optimization, "-c", "import runpy, sys; runpy.run_path(sys.argv[1])", str(path)],
        cwd=ROOT, capture_output=True, text=True, check=False,
    )


def main() -> None:
    for flag in ("-O", "-OO"):
        actual = invoke(ORACLE, flag)
        if actual.returncode == 0 or MESSAGE not in actual.stderr:
            raise SystemExit(f"FAIL: oracle did not reject optimization {flag}")

    # Mutation control: disabling the guard must make this check fail.  This
    # runs a temporary copy only; it never modifies the checkout or fixtures.
    source = ORACLE.read_text(encoding="utf-8")
    marker = "if not __debug__:\n    raise SystemExit(\"" + MESSAGE + "\")"
    if source.count(marker) != 1:
        raise SystemExit("FAIL: guard marker is missing or non-unique")
    with tempfile.TemporaryDirectory(prefix="w1-interop-guard-") as directory:
        mutated = Path(directory) / "oracle.py"
        mutated.write_text(source.replace(marker, "if False:\n    raise SystemExit(\"" + MESSAGE + "\")", 1), encoding="utf-8")
        result = invoke(mutated, "-O")
        if result.returncode != 0:
            raise SystemExit("FAIL: guard mutation did not produce the expected unchecked import: " + result.stderr[-1000:])
    print("PASS W1 interop rejects -O/-OO; guard bypass mutation verified")


if __name__ == "__main__":
    main()
