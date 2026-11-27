#!/usr/bin/env python3
"""O0 focused mutation controls; restore the source exactly after each test.

These controls test the tests, not the safety of the CI layout. Never run
while another process is building the same checkout. Model: tools/nc_w7.py.

Control set (docs/o0-ci-plan.md):
  1. disabled-shard check dropped -> gate red (the named O0 control:
     disabled_shard_refused — "disable a shard -> CI red")
  2. missing-target check dropped -> gate red
  3. duplicate-target check dropped -> gate red
  4. shard-set check dropped -> gate red
  5. empty-shard check dropped -> gate red
  6. unknown-line strictness dropped -> gate red (no secret shards)
"""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CI = ROOT / "crates/aura-testkit/src/ci.rs"


def run_test(test: str) -> subprocess.CompletedProcess[str]:
    cmd = [
        "cargo", "test", "-p", "aura-testkit",
        "--test", "o0_ci", test, "--", "--exact",
    ]
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=False)


def control(label: str, old: str, new: str, test: str, source: Path = CI) -> bool:
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
        "NC: disabled-shard check dropped -> gate red (named: disabled_shard_refused)",
        "        if *disabled && shard_load(registry, name) > 0 {",
        "        if false {",
        "disabled_shard_refused",
    ))
    checks.append(control(
        "NC: missing-target check dropped -> gate red",
        "        if registrations(registry, target) == 0 {",
        "        if false {",
        "missing_target_refused",
    ))
    checks.append(control(
        "NC: duplicate-target check dropped -> gate red",
        "        if registrations(registry, target) > 1 {",
        "        if false {",
        "duplicate_target_refused",
    ))
    checks.append(control(
        "NC: shard-set check dropped -> gate red",
        "        if !registry.shards.iter().any(|(declared, _)| declared == name) {",
        "        if false {",
        "shard_set_mismatch_refused",
    ))
    checks.append(control(
        "NC: empty-shard check dropped -> gate red",
        "        if shard_load(registry, name) == 0 {",
        "        if false {",
        "empty_shard_refused",
    ))
    checks.append(control(
        "NC: unknown-line strictness dropped -> gate red",
        '            return Err(format!("unknown registry line: {line}"));',
        "            continue;",
        "foreign_registry_line_refused",
    ))

    print(f"RESULT: {sum(checks)}/{len(checks)} negative controls PASS")
    return 0 if all(checks) else 1


if __name__ == "__main__":
    sys.exit(main())
