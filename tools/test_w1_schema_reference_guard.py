#!/usr/bin/env python3
"""Check that the W1 schema reference fails on corrupted inputs and bypasses."""
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest import mock

import w1_schema_reference as ref


def main():
    # The oracle does not rely on Python assert statements. Optimized Python
    # must still run its complete fixture comparison.
    for flag in ("-O", "-OO"):
        result = subprocess.run([sys.executable, flag, str(Path(ref.__file__))], capture_output=True, text=True, check=False)
        if result.returncode != 0 or "PASS independent Python W1 schema vectors" not in result.stdout:
            raise SystemExit(f"FAIL optimized reference check {flag}: {result.stdout}{result.stderr}")

    # Corrupt a temporary copy; committed fixtures are never touched.
    with tempfile.TemporaryDirectory(prefix="w1-schema-guard-") as directory:
        path = Path(directory) / "vectors.json"
        path.write_bytes(ref.OUT.read_bytes() + b"corruption")
        with mock.patch.object(ref, "OUT", path), mock.patch.object(sys, "argv", ["reference"]):
            try:
                with redirect_stdout(StringIO()):
                    ref.main()
            except SystemExit as exc:
                if "vectors differ" not in str(exc):
                    raise
            else:
                raise SystemExit("FAIL: corrupted reference accepted")

    # Forcing the independent parser to accept every input must fail when the
    # generator checks its explicit expected reject cases.
    with mock.patch.object(ref, "accepts", return_value=True):
        try:
            ref.generate()
        except ValueError as exc:
            if "independent oracle disagrees" not in str(exc):
                raise
        else:
            raise SystemExit("FAIL: all-accept parser mutation went undetected")
    print("PASS W1 schema oracle under -O/-OO; corruption and accept-all mutations detected")


if __name__ == "__main__":
    main()
