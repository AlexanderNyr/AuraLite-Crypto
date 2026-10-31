#!/usr/bin/env python3
"""D1 negative controls using focused Rust unit tests only."""
from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
AURAD = REPO / "crates/aura-node/src/bin/aurad.rs"
BASELINE = REPO / "crates/aura-node/src/baseline.rs"
CONFIG = REPO / "crates/aura-node/src/config.rs"


def focused_command(path: Path, test: str) -> list[str]:
    if path == AURAD:
        target, exact = ["--bin", "aurad"], f"tests::{test}"
    elif path == CONFIG:
        target, exact = ["--lib"], f"config::tests::{test}"
    else:
        target, exact = ["--lib"], f"baseline::tests::{test}"
    return ["cargo", "test", "-p", "aura-node", *target, "--", "--exact", exact]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test: str) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL (tamper target not found in {path.name})")
        return False

    cmd = focused_command(path, test)
    exact = cmd[-1]
    baseline = run(cmd)
    baseline_out = baseline.stdout + baseline.stderr
    if baseline.returncode != 0 or re.search(
        rf"test\s+{re.escape(exact)}\s+\.\.\.\s+ok", baseline_out
    ) is None:
        print(f"{label}: FAIL (exact unit test was not green before mutation)")
        print(baseline_out[-5000:])
        return False

    try:
        path.write_text(original.replace(old, new, 1), encoding="utf-8")
        os.utime(path, None)
        result = run(cmd)
        output = result.stdout + result.stderr
        went_red = result.returncode != 0 and re.search(
            rf"test\s+{re.escape(exact)}\s+\.\.\.\s+FAILED", output
        ) is not None
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        if not went_red:
            print(f"      expected {exact} to fail after mutation")
            print(output[-5000:])
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    print("D1 NEGATIVE CONTROLS (focused library/binary unit tests)")
    cases = [
        (
            "NC-1 aurad without --chain-id must refuse",
            AURAD,
            "            return baseline::EXIT_STUB;",
            "            return 0;",
            "run_node_refuses_without_chain_id",
        ),
        (
            "NC-2 unknown TOML fields must be rejected",
            CONFIG,
            """        for key in table.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(ConfigError::UnknownField(key.clone()));
            }
        }""",
            """        for key in table.keys() {
            if false {
                return Err(ConfigError::UnknownField(key.clone()));
            }
        }""",
            "config_toml_unknown_field_is_rejected",
        ),
        (
            "NC-3 report must reject non-workspace paths",
            BASELINE,
            '    if !root.join("crates").is_dir() {',
            "    if false {",
            "report_refuses_a_tree_that_is_not_a_workspace",
        ),
        (
            "NC-4 unknown commands must return usage error",
            AURAD,
            '            let _ = writeln!(err, "{}", usage());\n            baseline::EXIT_USAGE',
            '            let _ = writeln!(err, "{}", usage());\n            0',
            "unknown_command_is_usage_error",
        ),
        (
            "NC-5 refusal must name the D1 phase",
            AURAD,
            "the `config` capability lands with D1",
            "the `config` capability lands with unknown phase",
            "run_node_refuses_without_chain_id",
        ),
        (
            "NC-6 config::load must require chain-id",
            CONFIG,
            ".ok_or(ConfigError::MissingChainId)?",
            '.unwrap_or("dummy-chain")',
            "chain_id_is_mandatory",
        ),
    ]
    passed = sum(expect_red(*case) for case in cases)
    print(f"RESULT: {passed}/{len(cases)} negative controls PASS")
    return 0 if passed == len(cases) else 1


if __name__ == "__main__":
    raise SystemExit(main())
