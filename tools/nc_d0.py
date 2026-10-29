#!/usr/bin/env python3
"""Negative controls for D0 (the node baseline).

Six tampers, each one a mistake that looks harmless and destroys the claim "the
libraries exist, the node does not". Every one of them turns a *named* test in
`crates/aura-node/tests/d0_baseline.rs` red — a gate that cannot go red is not a gate
(CONTRIBUTING.md §1).

  NC-1 the stub exits 0                         -> the binary looks like a working node
                                                   in every script and CI job.
  NC-2 every layer is classified `implemented`  -> the baseline stops measuring, and the
                                                   four placeholder crates are reported
                                                   as finished work.
  NC-3 `report` measures a tree that is not a   -> the survey invents numbers for a path
       workspace                                  that has no crates/ directory.
  NC-4 an unknown command falls through to exit -> a typo (`aurad serve`) looks like
       0                                          success.
  NC-5 the refusal stops naming the capabilities-> the reader is told "not implemented"
                                                   with no phase to follow.
  NC-6 a node flag is no longer recognised      -> `--config` is reported as a *typo*
                                                   instead of "D1 owes this".

Each control runs one focused test rather than the whole file. A green baseline run of
the full gate comes first, so a red caused by a broken tree can never be mistaken for
a successful control.

Usage:
    python3 tools/nc_d0.py
    make nc-d0
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE = REPO / "crates" / "aura-node" / "src" / "baseline.rs"

GATE = ["cargo", "test", "-p", "aura-node", "--test", "d0_baseline", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test: str | None = None) -> bool:
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
    print("D0 NEGATIVE CONTROLS (node baseline gate)")
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
            "NC-1 the stub exits 0",
            BASELINE,
            """    let Some(command) = args.first().map(String::as_str) else {
        return refuse(err);
    };""",
            """    let Some(command) = args.first().map(String::as_str) else {
        let _ = refuse(err);
        return 0;
    };""",
            "stub_refuses_without_arguments",
        ),
        expect_red(
            "NC-2 every layer is classified `implemented`",
            BASELINE,
            """    let presence = if files == 0 {
        Presence::Missing
    } else if public_items == 0 {
        Presence::Placeholder
    } else {
        Presence::Implemented
    };""",
            """    let presence = Presence::Implemented;
    let _ = (files, public_items);""",
            "the_unlanded_layers_are_still_placeholders",
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
            let _ = other;
            0
        }""",
            "unknown_command_is_a_usage_error",
        ),
        expect_red(
            "NC-5 the refusal stops naming the capabilities",
            BASELINE,
            """    for capability in &MISSING_CAPABILITIES {
        let _ = writeln!(
            text,
            "  {:<12} {:<4} {}",
            capability.name, capability.phase, capability.what
        );
    }""",
            """    let _ = &MISSING_CAPABILITIES;""",
            "stub_names_every_missing_capability",
        ),
        expect_red(
            "NC-6 a node flag is no longer recognised",
            BASELINE,
            """        "--config" | "--data-dir" | "--rpc-addr" | "--chain-id" | "--validator-key" => {""",
            """        "--data-dir" | "--rpc-addr" | "--chain-id" | "--validator-key" => {""",
            "node_flags_name_their_phase",
        ),
    ]

    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
