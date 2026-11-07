#!/usr/bin/env python3
"""Exercise mnemonic reference fail-closed behavior without changing checkout."""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools/w1_mnemonic_reference.py"
FIXTURE = ROOT / "crates/aura-wallet/tests/fixtures/w1_mnemonic_reference.json"
MARKER = "if not OUT.is_file() or OUT.read_bytes() != generate():"


def invoke(script, flag):
    args = [sys.executable] + ([flag] if flag else []) + [str(script)]
    return subprocess.run(args, capture_output=True, text=True, check=False)


def main():
    source = SCRIPT.read_text(encoding="utf-8")
    if source.count(MARKER) != 1:
        raise SystemExit("FAIL mnemonic reference guard marker is absent or not unique")
    with tempfile.TemporaryDirectory(prefix="w1-mnemonic-guard-") as temp:
        root = Path(temp)
        script = root / "tools/w1_mnemonic_reference.py"
        fixture = root / "crates/aura-wallet/tests/fixtures/w1_mnemonic_reference.json"
        script.parent.mkdir(parents=True)
        fixture.parent.mkdir(parents=True)
        script.write_text(source, encoding="utf-8")
        shutil.copyfile(FIXTURE, fixture)
        for flag in ("", "-O", "-OO"):
            baseline = invoke(script, flag)
            if baseline.returncode != 0:
                raise SystemExit(f"FAIL mnemonic reference baseline {flag}: {baseline.stderr}")
        fixture.write_bytes(fixture.read_bytes() + b"corruption\n")
        for flag in ("", "-O", "-OO"):
            result = invoke(script, flag)
            if result.returncode == 0 or "FAIL public mnemonic whitespace reference vectors differ" not in result.stderr:
                raise SystemExit(f"FAIL corrupted mnemonic reference accepted under {flag}")
        script.write_text(source.replace(MARKER, "if not OUT.is_file():", 1), encoding="utf-8")
        if invoke(script, "-O").returncode != 0:
            raise SystemExit("FAIL reference comparison bypass mutation did not produce false success")
    print("PASS mnemonic reference rejects corruption under normal/-O/-OO; comparison bypass mutation verified")


if __name__ == "__main__":
    main()
