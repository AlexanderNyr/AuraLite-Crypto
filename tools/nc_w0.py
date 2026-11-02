#!/usr/bin/env python3
"""Six W0 negative controls against `crates/aura-wallet/src/baseline.rs` library unit tests."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = "crates/aura-wallet/src/baseline.rs"


def run_cmd(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True)


def replace_once(rel_path: str, old: str, new: str) -> str:
    path = ROOT / rel_path
    original = path.read_text(encoding="utf-8")
    count = original.count(old)
    if count != 1:
        raise RuntimeError(f"{rel_path}: expected 1 occurrence of {old!r}, found {count}")
    path.write_text(original.replace(old, new, 1), encoding="utf-8")
    return original


def restore(rel_path: str, content: str) -> None:
    (ROOT / rel_path).write_text(content, encoding="utf-8")


def expect_red(
    label: str,
    rel_path: str,
    old: str,
    new: str,
    test_filter: str,
) -> bool:
    original = replace_once(rel_path, old, new)
    try:
        res = run_cmd(
            [
                "cargo",
                "test",
                "-p",
                "aura-wallet",
                "--lib",
                "--",
                "--exact",
                test_filter,
            ]
        )
    finally:
        restore(rel_path, original)

    if res.returncode != 0:
        print(f"PASS {label}: {test_filter} failed as expected")
        return True
    print(f"FAIL {label}: {test_filter} unexpectedly passed")
    return False


def main() -> int:
    print("W0: validating focused baseline tests")
    base = run_cmd(["cargo", "test", "-p", "aura-wallet", "--lib"])
    if base.returncode != 0:
        sys.stderr.write(base.stdout + base.stderr)
        return 1

    controls = [
        (
            "NC-1 key in a log -> tree search must fail (disable .log secret scanning)",
            BASELINE,
            'let mut file_findings = scan_text_for_key_leaks(&rel, &content);\n    findings.append(&mut file_findings);',
            'let _ = (&rel, &content, &mut findings);',
            "baseline::tests::key_in_a_log_turns_tree_search_gate_red",
        ),
        (
            "NC-2 W0 layer survey must report placeholders until W1-W4 modules exist",
            BASELINE,
            "            } else {\n                Presence::Placeholder\n            }",
            "            } else {\n                Presence::Implemented\n            }",
            "baseline::tests::w0_all_layers_are_placeholders",
        ),
        (
            "NC-3 report must refuse a directory without crates/",
            BASELINE,
            "    if !crates.is_dir() {\n        return Err(format!(",
            "    if false && !crates.is_dir() {\n        return Err(format!(",
            "baseline::tests::report_refuses_a_tree_that_is_not_a_workspace",
        ),
        (
            "NC-4 aura-wallet without arguments must exit EXIT_STUB (2), not 0",
            BASELINE,
            "    let Some(first) = args.first().map(String::as_str) else {\n        let _ = write!(stderr, \"{}\", format_refusal(None));\n        return EXIT_STUB;\n    };",
            "    let Some(first) = args.first().map(String::as_str) else {\n        return 0;\n    };",
            "baseline::tests::refusal_exits_stub_without_arguments",
        ),
        (
            "NC-5 refusal message must name every missing W1-W4 capability",
            BASELINE,
            '    for cap in MISSING_CAPABILITIES {\n        let _ = writeln!(out, "  - {:<18} [{}]: {}", cap.name, cap.phase, cap.what);\n    }',
            "    for _cap in MISSING_CAPABILITIES {}",
            "baseline::tests::refusal_names_every_missing_capability",
        ),
        (
            "NC-6 log scanner must detect leaked BIP-39 mnemonics, PEM blocks, and raw seeds",
            BASELINE,
            '        if trimmed_line.starts_with("-----BEGIN ") && trimmed_line.ends_with("PRIVATE KEY-----") {',
            '        if false && trimmed_line.starts_with("-----BEGIN ") && trimmed_line.ends_with("PRIVATE KEY-----") {',
            "baseline::tests::scanner_detects_mnemonic_pem_and_seed_leaks_and_ignores_public_hashes",
        ),
    ]

    passed = 0
    for label, rel_path, old, new, test_filter in controls:
        if expect_red(label, rel_path, old, new, test_filter):
            passed += 1

    print(f"RESULT: {passed}/{len(controls)} negative controls PASS")
    return 0 if passed == len(controls) else 1


if __name__ == "__main__":
    raise SystemExit(main())
