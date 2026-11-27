#!/usr/bin/env python3
"""O1 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the release pipeline. Never
run while another process is building the same checkout. Model: tools/nc_o0.py.

Control set (docs/o1-releases-plan.md):
  1. repro-equality check dropped -> gate red (the named O1 control:
     repro_mismatch_refused — "two independent hosts -> identical sha256")
  2. version check dropped -> gate red
  3. changelog check dropped -> gate red
  4. checksum match check dropped -> gate red
  5. signature length check dropped -> gate red
  6. unknown-line strictness dropped -> gate red (no commands in manifests)
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
RELEASE = ROOT / "crates/aura-testkit/src/release.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-testkit",
        "--test", "o1_release", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = RELEASE) -> bool:
    original = source.read_text(encoding="utf-8")
    if original.count(old) != 1:
        print(f"FAIL {label}: mutation target not unique")
        return False
    try:
        source.write_text(original.replace(old, new, 1), encoding="utf-8")
        result = run_test(test)
    finally:
        source.write_text(original, encoding="utf-8")
    # Compilation failure is NOT a valid red test.
    ok = result.returncode != 0 and f"test {test} ... FAILED" in result.stdout
    print(f"{'PASS' if ok else 'FAIL'} {label}: {test}")
    if not ok:
        print((result.stdout + result.stderr)[-2500:])
    return ok


def main() -> int:
    checks: list[bool] = []

    checks.append(control(
        "NC: repro-equality check dropped -> gate red (named: repro_mismatch_refused)",
        "    if hosts.iter().any(|(_, digest)| digest != first) {",
        "    if false {",
        "repro_mismatch_refused",
    ))
    checks.append(control(
        "NC: version check dropped -> gate red",
        "    if manifest.version != workspace.version {",
        "    if false {",
        "version_drift_refused",
    ))
    checks.append(control(
        "NC: changelog check dropped -> gate red",
        '    if !changelog.contains(&format!("## [{}", manifest.version)) {',
        "    if false {",
        "changelog_missing_refused",
    ))
    checks.append(control(
        "NC: checksum match check dropped -> gate red",
        "                if expected != actual {",
        "                if false {",
        "checksum_mismatch_refused",
    ))
    checks.append(control(
        "NC: signature length check dropped -> gate red",
        "        if decoded.len() != SIG_BYTES {",
        "        if false {",
        "signature_malformed_refused",
    ))
    checks.append(control(
        "NC: unknown-line strictness dropped -> gate red",
        '            return Err(format!("unknown manifest line: {line}"));',
        "            continue;",
        "foreign_manifest_line_refused",
    ))

    print(f"RESULT: {sum(checks)}/{len(checks)} negative controls PASS")
    return 0 if all(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
