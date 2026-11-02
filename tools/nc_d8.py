#!/usr/bin/env python3
"""Six D8 CLI mutation controls, each targeting an exact aura-node library unit test."""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CLI_RS = REPO / "crates/aura-node/src/cli.rs"

CASES = [
    (
        "NC-1 aura --help must document all subcommand families",
        CLI_RS,
        '        "--help" | "-h" | "help" => {\n            let _ = write!(out, "{HELP_TEXT}");\n            EXIT_OK\n        }',
        '        "--help" | "-h" | "help" => {\n            let _ = writeln!(out, "aura help");\n            EXIT_OK\n        }',
        "cli::tests::help_documents_all_subcommands_and_flags",
    ),
    (
        "NC-2 keys verify must reject forged signatures",
        CLI_RS,
        "            if verify(&pubkey, &chain_id, Domain::Tx, &msg, &sig_bytes).is_ok() {",
        "            if true {",
        "cli::tests::keys_verify_rejects_forged_signature",
    ),
    (
        "NC-3 tx transfer must reject gas limit below floor",
        CLI_RS,
        "            if gas_limit < CLI_TRANSFER_GAS {",
        "            if false {",
        "cli::tests::tx_transfer_rejects_below_gas_floor",
    ),
    (
        "NC-4 node config-check must reject unknown TOML fields",
        CLI_RS,
        "                if !ALLOWED_NODE_TOML_KEYS.contains(&key.as_str()) {",
        "                if false {",
        "cli::tests::node_config_check_rejects_unknown_field_and_missing_chain_id",
    ),
    (
        "NC-5 snapshot verify must reject tampered digest",
        CLI_RS,
        "            if constant_time_eq(snap_hash.as_bytes(), &expect_bytes) {",
        "            if true {",
        "cli::tests::snapshot_verify_rejects_tampered_digest",
    ),
    (
        "NC-6 upgrade plan must reject non-increasing protocol version",
        CLI_RS,
        "    if to_version <= from_version {",
        "    if false {",
        "cli::tests::upgrade_plan_rejects_downgrade_or_same_version",
    ),
]


def focused_test(test: str) -> tuple[bool, str]:
    command = [
        "cargo",
        "test",
        "-p",
        "aura-node",
        "--lib",
        "--",
        "--exact",
        test,
    ]
    result = subprocess.run(command, cwd=REPO, text=True, capture_output=True)
    output = result.stdout + result.stderr
    ran_ok = result.returncode == 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+ok", output
    ) is not None
    ran_failed = result.returncode != 0 and re.search(
        rf"test\s+{re.escape(test)}\s+\.\.\.\s+FAILED", output
    ) is not None
    return (ran_ok if result.returncode == 0 else ran_failed), output


def main() -> int:
    print("D8: validating focused baseline tests")
    for label, path, old, _new, test in CASES:
        original = path.read_text(encoding="utf-8")
        if old not in original:
            print(f"FAIL {label}: tamper target not found in {path.relative_to(REPO)}")
            return 1
        ok, output = focused_test(test)
        if not ok:
            print(f"FAIL {label}: target test did not pass before mutation")
            print(output[-5000:])
            return 1

    passed = 0
    for label, path, old, new, test in CASES:
        original = path.read_text(encoding="utf-8")
        mutated = original.replace(old, new, 1)
        path.write_text(mutated, encoding="utf-8")
        try:
            red, output = focused_test(test)
        finally:
            path.write_text(original, encoding="utf-8")
        if red:
            passed += 1
            print(f"PASS {label}: {test} failed as expected")
        else:
            print(f"FAIL {label}: expected named test to fail: {test}")
            print(output[-5000:])

    print(f"RESULT: {passed}/{len(CASES)} negative controls PASS")
    return 0 if passed == len(CASES) else 1


if __name__ == "__main__":
    sys.exit(main())
