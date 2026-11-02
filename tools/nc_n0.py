#!/usr/bin/env python3
"""Negative controls for N0 (the P2P network baseline).

Six tampers, each turning a named library unit test in
`crates/aura-p2p/src/baseline.rs` red via `--lib -- --exact baseline::tests::<test>`:

  NC-1 the refusal exits 0                      -> `aura-p2p` pretends a network exists.
  NC-2 placeholder network layers marked impl   -> N1..N8 placeholders reported as implemented.
  NC-3 `report` accepts a non-workspace path    -> survey invents numbers without crates/.
  NC-4 unknown command exits 0                  -> invalid CLI invocation succeeds silently.
  NC-5 refusal drops missing-capability table   -> missing N1..N8 phases are omitted.
  NC-6 Node B pretends to observe Node A's tx   -> falsifies the N0 two-node isolation gap.

Usage:
    python3 tools/nc_n0.py
    make nc-n0
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BASELINE = REPO / "crates" / "aura-p2p" / "src" / "baseline.rs"

GATE = ["cargo", "test", "-p", "aura-p2p", "--lib", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test: str) -> bool:
    cmd = list(GATE) + ["--", "--exact", f"baseline::tests::{test}"]
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
        raw = "\n".join(
            line.rstrip()
            for line in (result.stdout + result.stderr).strip().splitlines()
            if line.strip()
        )
        tail = "\n".join(raw.splitlines()[-2:])
        snippet = re.sub(r"finished in [\d.]+s", "finished", tail)
        print(f"{label}: {'PASS' if went_red else 'FAIL'}")
        print(
            f"      baseline::tests::{test} -> rc={result.returncode} "
            f"({'RED as expected' if went_red else 'UNEXPECTEDLY GREEN'})"
        )
        if snippet:
            print("      " + snippet.replace("\n", "\n      "))
        return went_red
    finally:
        path.write_text(original, encoding="utf-8")
        os.utime(path, None)


def main() -> int:
    print("N0 NEGATIVE CONTROLS (P2P baseline gate)")
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
            "NC-1 missing arguments must refuse",
            BASELINE,
            """    let Some(command) = args.first().map(String::as_str) else {
        return refuse(err);
    };""",
            """    let Some(command) = args.first().map(String::as_str) else {
        let _ = refuse(err);
        return 0;
    };""",
            "refusal_exits_stub_without_arguments",
        ),
        expect_red(
            "NC-2 placeholder network layers must remain placeholders",
            BASELINE,
            """    let presence = if files == 0 {
        Presence::Missing
    } else if layer.has_network_logic() {
        Presence::Implemented
    } else {
        Presence::Placeholder
    };""",
            """    let presence = if files == 0 {
        Presence::Missing
    } else if true {
        Presence::Implemented
    } else {
        Presence::Placeholder
    };""",
            "n0_implemented_and_placeholders",
        ),
        expect_red(
            "NC-3 report must reject non-workspace paths",
            BASELINE,
            '    if !root.join("crates").is_dir() {',
            "    if false {",
            "report_refuses_a_tree_that_is_not_a_workspace",
        ),
        expect_red(
            "NC-4 unknown commands must return usage error",
            BASELINE,
            """        other => {
            let _ = writeln!(err, "aura-p2p: unknown command `{other}`.");
            let _ = writeln!(err, "{}", usage());
            EXIT_USAGE
        }""",
            """        other => {
            let _ = writeln!(err, "aura-p2p: unknown command `{other}`.");
            let _ = writeln!(err, "{}", usage());
            0
        }""",
            "unknown_command_exits_usage",
        ),
        expect_red(
            "NC-5 refusal must name every missing capability",
            BASELINE,
            """    for capability in &MISSING_CAPABILITIES {
        let _ = writeln!(
            text,
            "  {:<14} {:<4} {}",
            capability.name, capability.phase, capability.what
        );
    }""",
            "    let _ = &MISSING_CAPABILITIES;",
            "refusal_names_every_missing_capability",
        ),
        expect_red(
            "NC-6 Node B cannot observe Node A's tx without P2P",
            BASELINE,
            """pub fn node_b_can_observe_node_a_tx(_node_a_txs: &[&str], node_b_txs: &[&str]) -> bool {
    !node_b_txs.is_empty()
}""",
            """pub fn node_b_can_observe_node_a_tx(_node_a_txs: &[&str], _node_b_txs: &[&str]) -> bool {
    true
}""",
            "node_b_cannot_observe_node_a_tx_without_p2p",
        ),
    ]

    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    restored = run(GATE)
    if restored.returncode != 0:
        print("restore check: aura-p2p library tests remain red")
        return 1
    print("restore check: cargo test -p aura-p2p --lib -> GREEN")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
