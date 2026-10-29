#!/usr/bin/env python3
"""Negative controls for X7 (the determinism gate).

Six tampers, each one a mistake that looks harmless and destroys the claim. Every
one of them turns a *named* test in `crates/aura-exec/tests/x7_determinism.rs` red,
and the log records which test that was — a gate that cannot go red is not a gate
(CONTRIBUTING.md §1).

  NC-1 the version stamp is dropped from the  -> two chains at different heights
       state root                               share a root, so a header no longer
                                                commits to the history that produced it.
  NC-2 an empty diff counts as a commit       -> a refusal is indistinguishable from
                                                a write, and the version advances for
                                                transactions that changed nothing.
  NC-3 the digest ignores the resulting root  -> two nodes that executed the same
                                                transactions in a different order
                                                produce the same digest, because
                                                nothing in the chain depends on what
                                                the order produced.
  NC-4 a refusal still applies its diff       -> a refused step moves the root.
  NC-5 a starved call is billed nothing       -> an out-of-gas call is free, which is
                                                the shape of a denial-of-service bug.
  NC-6 the generator ignores its seed         -> the gate passes on any input, which
                                                makes every other control meaningless.

Each control runs one focused test rather than the whole file: the tamper must
redden *that* property, and running the covering test alone keeps the loop honest
and fast. A green baseline run of the full gate comes first, so a red caused by a
broken tree can never be mistaken for a successful control.

Usage:
    python3 tools/nc_x7.py
    make nc-x7
"""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DETCHECK = REPO / "crates" / "aura-exec" / "src" / "detcheck.rs"
TREE_RS = REPO / "crates" / "aura-state" / "src" / "tree.rs"

GATE = ["cargo", "test", "-p", "aura-exec", "--test", "x7_determinism", "--quiet"]


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)


def expect_red(
    label: str, path: Path, old: str, new: str, test: str | None = None
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
    print("X7 NEGATIVE CONTROLS (determinism gate)")
    print("Rule: a gate that cannot go red is not a gate (CONTRIBUTING.md §1).")
    print()

    print("baseline: the untampered gate")
    baseline = run(GATE)
    print(f"      rc={baseline.returncode} ({'GREEN as expected' if baseline.returncode == 0 else 'RED — fix the tree first'})")
    if baseline.returncode != 0:
        tail = "\n".join((baseline.stdout + baseline.stderr).strip().splitlines()[-10:])
        print(tail)
        print("RESULT: 0/6 negative controls PASS (baseline is not green)")
        return 1
    print()

    results = [
        expect_red(
            "NC-1 the version stamp is dropped from the state root",
            TREE_RS,
            """pub fn versioned_root(version: u64, tree_root: &Hash) -> Hash {
    let mut input = Vec::with_capacity(S1_ROOT_CONTEXT.len() + 8 + 32);
    input.extend_from_slice(S1_ROOT_CONTEXT);
    input.extend_from_slice(&version.to_le_bytes());
    input.extend_from_slice(tree_root.as_bytes());
    blake3_hash(&input)
}""",
            """pub fn versioned_root(version: u64, tree_root: &Hash) -> Hash {
    let _ = version;
    *tree_root
}""",
            "the_pinned_measurement_describes_this_tree",
        ),
        expect_red(
            "NC-2 an empty diff counts as a commit",
            DETCHECK,
            """            Some(diff) if !diff.changes.is_empty() => {
                let next = apply_diff(&self.tree, diff).expect("diff applies to its own base");
                self.tree = next;
                self.committed += 1;""",
            """            Some(diff) => {
                let next = apply_diff(&self.tree, diff).expect("diff applies to its own base");
                self.tree = next;
                self.committed += 1;""",
            "the_pinned_measurement_describes_this_tree",
        ),
        expect_red(
            "NC-3 the rolling digest ignores the resulting root",
            DETCHECK,
            """    bytes.push(class);
    bytes.extend_from_slice(&(status as i32).to_le_bytes());
    bytes.extend_from_slice(&gas.to_le_bytes());
    bytes.extend_from_slice(root.as_bytes());""",
            """    bytes.push(class);
    bytes.extend_from_slice(&(status as i32).to_le_bytes());
    bytes.extend_from_slice(&gas.to_le_bytes());
    let _ = root;""",
            "the_digest_depends_on_the_order_of_the_steps",
        ),
        expect_red(
            "NC-4 a refusal still applies its diff",
            DETCHECK,
            """            _ => {
                // Returned but wrote nothing: a refusal. The root must not move,
                // and the assertion is made here rather than in the test so that a
                // non-deterministic *commit* cannot slip through as a pass.
                let before = self.tree.state_root();
                self.refused += 1;""",
            """            other => {
                if let Some(diff) = other {
                    self.tree = apply_diff(&self.tree, diff).expect("diff applies");
                }
                let before = self.tree.state_root();
                self.refused += 1;""",
            "every_commit_advances_the_version_by_exactly_one",
        ),
        expect_red(
            "NC-5 a starved call is billed nothing",
            DETCHECK,
            """                let class = 3u8;
                self.bill(index, gas, &events);""",
            """                let class = 3u8;
                let _ = gas;""",
            "a_starved_call_is_billed_and_changes_nothing",
        ),
        expect_red(
            "NC-6 the generator ignores its seed",
            DETCHECK,
            """pub fn scenario_of(seed: u64, transactions: usize) -> Vec<Step> {
    let mut prng = Prng::new(seed);""",
            """pub fn scenario_of(seed: u64, transactions: usize) -> Vec<Step> {
    let _ = seed;
    let mut prng = Prng::new(X7_SEED);""",
            "a_different_input_changes_the_root",
        ),
    ]

    print()
    passed = sum(results)
    print(f"RESULT: {passed}/{len(results)} negative controls PASS")
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
