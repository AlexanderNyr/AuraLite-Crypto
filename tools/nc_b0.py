#!/usr/bin/env python3
"""Six B0 negative controls against the current consensus baseline."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

BASE = "crates/aura-consensus/src/baseline.rs"

CASES = [
    (
        "NC-1 missing arguments must refuse",
        BASE,
        "        return refuse(err);",
        "        return 0;",
        "baseline::tests::refusal_exits_stub_without_arguments",
    ),
    (
        "NC-2 placeholder consensus layers must remain placeholders",
        BASE,
        "else if layer.has_consensus_logic() {",
        "else if true {",
        "baseline::tests::b3_implemented_and_placeholders",
    ),
    (
        "NC-3 report must reject non-workspace paths",
        BASE,
        '    if !root.join("crates").is_dir() {',
        "    if false {",
        "baseline::tests::report_refuses_a_tree_that_is_not_a_workspace",
    ),
    (
        "NC-4 unknown commands must return usage error",
        BASE,
        '            let _ = writeln!(err, "{}", usage());\n            EXIT_USAGE',
        '            let _ = writeln!(err, "{}", usage());\n            0',
        "baseline::tests::unknown_command_exits_usage",
    ),
    (
        "NC-5 refusal must name every missing capability",
        BASE,
        '''    for capability in &MISSING_CAPABILITIES {
        let _ = writeln!(
            text,
            "  {:<14} {:<4} {}",
            capability.name, capability.phase, capability.what
        );
    }''',
        "    let _ = &MISSING_CAPABILITIES;",
        "baseline::tests::refusal_names_every_missing_capability",
    ),
    (
        "NC-6 distinct logs must not be equivalent without consensus",
        BASE,
        '''    fn logs_equivalent(_a: &[&str], _b: &[&str]) -> bool {
        // No full consensus → no equivalence at B3.
        false
    }''',
        '''    fn logs_equivalent(_a: &[&str], _b: &[&str]) -> bool {
        // No full consensus → no equivalence at B1.
        true
    }''',
        "baseline::tests::two_different_logs_are_not_equivalent",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B0", CASES))
