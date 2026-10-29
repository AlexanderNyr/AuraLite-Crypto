#!/usr/bin/env python3
"""Negative controls for D1 (the aurad skeleton).

Six tampers, each one a mistake that looks harmless and destroys the claim
"the node starts and shuts down cleanly". Every one of them turns a *named*
test in `crates/aura-node/tests/d1_skeleton.rs` red — a gate that cannot go
red is not a gate (CONTRIBUTING.md §1).

  NC-1 `aurad` without --chain-id exits 0                    -> the node looks
                                                                 configured when it is not.
  NC-2 a TOML config with an unknown field is accepted       -> typos in the
                                                                 config file are silently ignored
                                                                 instead of failing at startup.
  NC-3 `report` measures a tree that is not a workspace      -> the survey
                                                                 invents numbers for a path
                                                                 that has no crates/ directory.
  NC-4 an unknown command falls through to exit 0            -> a typo (`aurad serve`)
                                                                 looks like success.
  NC-5 the refusal stops naming the D1 phase                 -> the reader is told
                                                                 "not implemented" with no phase to follow.
  NC-6 `config::load` accepts a missing chain-id from env    -> env var is
                                                                 checked after CLI/TOML but still
                                                                 must be mandatory.

Each control runs one focused test rather than the whole file. A green baseline
run of the full gate comes first, so a red caused by a broken tree can never be
mistaken for a successful control.

Usage:
    python3 tools/nc_d1.py
    make nc-d1
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
AURAD = REPO / "crates" / "aura-node" / "src" / "bin" / "aurad.rs"
BASELINE = REPO / "crates" / "aura-node" / "src" / "baseline.rs"
CONFIG = REPO / "crates" / "aura-node" / "src" / "config.rs"
D1_TEST = REPO / "crates" / "aura-node" / "tests" / "d1_skeleton.rs"

GATE = ["cargo", "test", "-p", "aura-node", "--test", "d1_skeleton", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(
    label: str,
    path: Path,
    old: str,
    new: str,
    test: str | None = None,
) -> bool:
    cmd = list(GATE)
    if test:
        cmd += ["--", "--exact", test]
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — tamper target not found in {path.name}")
        return False
    tampered = original.replace(old, new, 1)
    if tampered == original:
        print(f"{label}: FAIL — tamper was a no-op")
        return False
    try:
        path.write_text(tampered, encoding="utf-8")
        os.utime(path, None)
        result = run(cmd)
        went_red = result.returncode != 0
        shown = test or "the whole gate"
        raw = "\n".join((result.stdout + result.stderr).strip().splitlines()[-2:])
        snippet = re.sub(r"finished in [\d.]+s", "finished", raw)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      {shown} -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        print(f"      {snippet}")
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    print("D1 NEGATIVE CONTROLS (aurad skeleton gate)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    print("baseline: the untampered gate")
    baseline = run(GATE)
    print(
        f"      rc={baseline.returncode} "
        f"({'GREEN as expected' if baseline.returncode == 0 else 'RED — fix the tree first'})"
    )
    if baseline.returncode != 0:
        tail = "\n".join((baseline.stdout + baseline.stderr).strip().splitlines()[-10:])
        print(tail)
        print("RESULT: 0/6 negative controls PASS (baseline is not green)")
        return 1
    print()

    results = [
        expect_red(
            "NC-1 `aurad` without --chain-id exits 0",
            AURAD,
            """        Err(e) => {
            let _ = writeln!(err, "aurad: configuration error: {e}");
            let _ = writeln!(err, "  the `config` capability lands with D1 (TOML + env + CLI, `chain-id` mandatory)");
            let _ = writeln!(err, "  pass --chain-id <id>, set AURA_CHAIN_ID, or add chain_id to aurad.toml");
            return baseline::EXIT_STUB;
        }""",
            """        Err(_e) => {
            return 0;
        }""",
            "binary_without_chain_id_refuses_and_names_d1",
        ),
        expect_red(
            "NC-2 a TOML config with an unknown field is accepted",
            CONFIG,
            """#[serde(deny_unknown_fields)]
pub struct TomlConfig {
    chain_id: Option<String>,
    data_dir: Option<String>,
    rpc_addr: Option<String>,
    validator_key: Option<String>,
}""",
            """pub struct TomlConfig {
    chain_id: Option<String>,
    data_dir: Option<String>,
    rpc_addr: Option<String>,
    validator_key: Option<String>,
}""",
            "config_toml_unknown_field_is_rejected",
        ),
        expect_red(
            "NC-3 `report` measures a tree that is not a workspace",
            BASELINE,
            """    if !root.join("crates").is_dir() {""",
            """    if false {""",
            "report_refuses_a_tree_that_is_not_a_workspace",
        ),
        expect_red(
            "NC-4 an unknown command falls through to exit 0",
            BASELINE,
            """        other => {
            let _ = writeln!(err, "aurad: unknown command `{other}`.");
            let _ = writeln!(err, "{}", usage());
            EXIT_USAGE
        }""",
            """        other => {
            let _ = writeln!(err, "aurad: unknown command `{other}`.");
            let _ = writeln!(err, "{}", usage());
            0
        }""",
            "binary_unknown_command_is_usage_error",
        ),
        expect_red(
            "NC-5 the refusal stops naming the D1 phase",
            AURAD,
            """            let _ = writeln!(err, "  the `config` capability lands with D1 (TOML + env + CLI, `chain-id` mandatory)");""",
            """            let _ = writeln!(err, "  not implemented");""",
            "binary_without_chain_id_refuses_and_names_d1",
        ),
        expect_red(
            "NC-6 `config::load` accepts a missing chain-id",
            CONFIG,
            """.ok_or(ConfigError::MissingChainId)?""",
            """.unwrap_or("dummy-chain".to_string())?""",
            "config_chain_id_is_mandatory",
        ),
    ]

    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
