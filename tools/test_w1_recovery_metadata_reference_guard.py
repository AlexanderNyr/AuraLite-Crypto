#!/usr/bin/env python3
"""Check that the W1 recovery-metadata reference fails closed on mutations."""
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest import mock

import w1_recovery_metadata_reference as ref


def main() -> None:
    for flag in ("-O", "-OO"):
        result = subprocess.run(
            [sys.executable, flag, str(Path(ref.__file__))],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0 or "PASS independent Python W1 recovery-metadata vectors" not in result.stdout:
            raise SystemExit(f"FAIL optimized recovery-metadata check {flag}: {result.stdout}{result.stderr}")

    with tempfile.TemporaryDirectory(prefix="w1-recovery-ref-guard-") as directory:
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
                raise SystemExit("FAIL: corrupted recovery-metadata fixture accepted")

    for bad_eval in (
        {"accepted": True, "error": None, "profile": "bip39-nfkd-unicode-18.0",
         "canonical": '{"normalization_profile":"bip39-nfkd-unicode-18.0"}'},
        {"accepted": False, "error": "invalid_format", "profile": None, "canonical": None},
    ):
        with mock.patch.object(ref, "evaluate", return_value=bad_eval):
            try:
                ref.generate()
            except ValueError as exc:
                if "oracle disagrees" not in str(exc):
                    raise
            else:
                raise SystemExit("FAIL: mutated recovery-metadata oracle went undetected")

    print("PASS W1 recovery-metadata oracle under -O/-OO; corruption and oracle mutations detected")


if __name__ == "__main__":
    main()
