//! D0 — the baseline gate: **the libraries exist, the node does not.**
//!
//! `NODE_PLAN`'s first phase is a measurement, not a feature. Two things are
//! asserted here:
//!
//! * the binary **refuses**. `aurad` exits non-zero, says which capability is
//!   missing and names the phase that owes it — for every argument shape a user
//!   might try. A stub that exits 0 is the failure mode CONTRIBUTING.md §1 exists to
//!   prevent, so the refusal is tested through the real binary, not only the library.
//! * the survey is **measured**, not a hand-written table. The gate reads the tree it
//!   runs in, and compares the result with
//!   `tests/fixtures/d0_baseline.txt`, which `tools/gen_d0_vectors.py` writes from
//!   the binary's own `report --fixture` output. Land consensus code (B1) without
//!   touching the plan and this gate goes red — which is the whole point.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_node::baseline::{
    self, EXIT_NO_WORKSPACE, EXIT_STUB, EXIT_USAGE, LAYERS, MISSING_CAPABILITIES, Presence,
    default_root, survey,
};

const FIXTURE: &str = include_str!("fixtures/d0_baseline.txt");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run the *library* entry point and capture both streams.
fn run(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = baseline::run(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

/// The fixture, parsed into `key -> value`; repeated keys accumulate in order.
fn fixture() -> (BTreeMap<String, String>, Vec<String>, Vec<String>) {
    let mut singles = BTreeMap::new();
    let mut capability = Vec::new();
    let mut layer = Vec::new();
    for line in FIXTURE.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "capability" => capability.push(value.to_string()),
            "layer" => layer.push(value.to_string()),
            other => {
                singles.insert(other.to_string(), value.to_string());
            }
        }
    }
    (singles, capability, layer)
}

#[test]
fn stub_refuses_without_arguments() {
    let (code, out, err) = run(&[]);
    assert_eq!(code, EXIT_STUB, "a node stub must not report success");
    assert!(
        out.is_empty(),
        "the refusal belongs on stderr, got stdout {out:?}"
    );
    assert!(err.contains("the node does not exist") || err.contains("the node does not"));
    assert!(
        err.contains("D0 baseline"),
        "the phase must be named: {err}"
    );
}

#[test]
fn stub_names_every_missing_capability() {
    let (_, _, err) = run(&[]);
    for capability in &MISSING_CAPABILITIES {
        assert!(
            err.contains(capability.name),
            "the refusal does not mention `{}`:\n{err}",
            capability.name
        );
        assert!(
            err.contains(capability.phase),
            "the refusal does not say which phase lands `{}`:\n{err}",
            capability.name
        );
        assert!(
            err.contains(capability.what),
            "the refusal does not say what `{}` will do:\n{err}",
            capability.name
        );
    }
}

#[test]
fn the_binary_refuses_too() {
    // The DoD is literal: running the binary must produce a meaningful refusal.
    let output = Command::new(env!("CARGO_BIN_EXE_aurad"))
        .output()
        .expect("aurad runs");
    assert_eq!(output.status.code(), Some(EXIT_STUB));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("D1"),
        "the binary's refusal names no phase:\n{err}"
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn node_flags_name_their_phase() {
    for (flag, phase) in [
        ("--config", "D1"),
        ("--chain-id", "D1"),
        ("--data-dir", "D1"),
        ("--rpc-addr", "D1"),
        ("--validator-key", "D1"),
    ] {
        let (code, _, err) = run(&[flag, "x"]);
        assert_eq!(code, EXIT_STUB, "`{flag}` must be refused");
        assert!(
            err.contains(flag) && err.contains(phase),
            "`{flag}` must name itself and {phase}:\n{err}"
        );
    }
}

#[test]
fn unknown_command_is_a_usage_error() {
    let (code, _, err) = run(&["serve"]);
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("unknown command `serve`"), "{err}");
    assert!(
        err.contains("usage:"),
        "a usage error prints the usage:\n{err}"
    );
}

#[test]
fn help_and_version_succeed_and_are_documented() {
    let (code, out, _) = run(&["--help"]);
    assert_eq!(code, 0);
    for needle in [
        "report",
        "--workspace",
        "--version",
        "--fixture",
        "exit codes",
    ] {
        assert!(
            out.contains(needle),
            "`--help` does not document {needle}:\n{out}"
        );
    }

    let (code, out, _) = run(&["--version"]);
    assert_eq!(code, 0);
    assert!(out.contains("aurad"));
    assert!(
        out.contains(baseline::PHASE),
        "the version names the phase: {out}"
    );
    assert!(
        out.contains("does not exist"),
        "the version must not imply a node: {out}"
    );
}

#[test]
fn report_measures_this_tree() {
    let root = default_root();
    assert!(
        root.join("crates").is_dir(),
        "{} is not a workspace",
        root.display()
    );
    let measured = survey(&root);
    assert_eq!(measured.layers.len(), LAYERS.len());
    assert!(
        measured.total_files() > 40,
        "measured only {} files",
        measured.total_files()
    );
    for entry in &measured.layers {
        match entry.presence {
            Presence::Missing => assert_eq!(entry.files, 0, "{}", entry.layer.name),
            Presence::Placeholder => {
                assert!(entry.files > 0, "{}", entry.layer.name);
                assert_eq!(
                    entry.public_items, 0,
                    "{} is not a placeholder",
                    entry.layer.name
                );
            }
            Presence::Implemented => {
                assert!(
                    entry.files > 0 && entry.public_items > 0,
                    "{}",
                    entry.layer.name
                )
            }
        }
    }

    let (code, out, _) = run(&["report"]);
    assert_eq!(code, 0);
    assert!(out.contains("aura-exec"), "{out}");
    assert!(out.contains("implemented"), "{out}");
    assert!(
        out.contains("node capabilities present: 0 of 9"),
        "the report must state the node is absent:\n{out}"
    );
}

#[test]
fn the_pinned_baseline_matches_this_tree() {
    let (singles, capabilities, layers) = fixture();
    assert_eq!(singles["phase"], baseline::PHASE);
    assert_eq!(singles["exit_stub"], EXIT_STUB.to_string());
    assert_eq!(singles["exit_usage"], EXIT_USAGE.to_string());
    assert_eq!(singles["exit_no_workspace"], EXIT_NO_WORKSPACE.to_string());
    assert_eq!(singles["layers"], LAYERS.len().to_string());
    assert_eq!(
        singles["capabilities"],
        MISSING_CAPABILITIES.len().to_string()
    );

    let expected_capabilities: Vec<String> = MISSING_CAPABILITIES
        .iter()
        .map(|entry| format!("{} {} {}", entry.name, entry.phase, entry.what))
        .collect();
    assert_eq!(capabilities, expected_capabilities);

    let measured = survey(&repo_root());
    let live: Vec<String> = measured
        .layers
        .iter()
        .map(|entry| {
            format!(
                "{} {} {}",
                entry.layer.name,
                entry.layer.phase,
                entry.presence.as_str()
            )
        })
        .collect();
    assert_eq!(
        layers, live,
        "the tree changed under the pinned D0 baseline — either the survey is wrong or a \
         phase landed without updating the plan and this fixture"
    );
    assert_eq!(singles["implemented"], measured.implemented().to_string());
    assert_eq!(singles["placeholders"], measured.placeholders().to_string());
    assert_eq!(singles["missing"], measured.missing().to_string());
}

#[test]
fn the_four_unlanded_layers_are_still_placeholders() {
    let measured = survey(&repo_root());
    assert_eq!(
        measured.placeholder_names(),
        vec!["aura-consensus", "aura-p2p", "aura-mempool", "aura-rpc"],
        "a placeholder grew public API without the plan being updated"
    );
    for entry in &measured.layers {
        let phase = entry.layer.phase;
        let is_placeholder = entry.presence == Presence::Placeholder;
        if matches!(phase, "B1" | "N1" | "D3" | "D4") {
            assert!(
                is_placeholder,
                "{} must still be a placeholder at D0",
                entry.layer.name
            );
        } else {
            assert!(
                !is_placeholder,
                "{} must be implemented at D0",
                entry.layer.name
            );
        }
    }
}

#[test]
fn report_refuses_a_tree_that_is_not_a_workspace() {
    let (code, out, err) = run(&["report", "--workspace", "/nonexistent-auralite"]);
    assert_eq!(code, EXIT_NO_WORKSPACE);
    assert!(out.is_empty());
    assert!(err.contains("is not a workspace"), "{err}");

    // The same through the real binary, so the exit code reaches the shell.
    let output = Command::new(env!("CARGO_BIN_EXE_aurad"))
        .args(["report", "--workspace", "/nonexistent-auralite"])
        .output()
        .expect("aurad runs");
    assert_eq!(output.status.code(), Some(EXIT_NO_WORKSPACE));
}

#[test]
fn report_rejects_unknown_arguments() {
    let (code, _, err) = run(&["report", "--json"]);
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("unknown argument `--json`"), "{err}");

    let (code, _, err) = run(&["report", "--workspace"]);
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("needs a path"), "{err}");
}

#[test]
fn the_survey_is_deterministic() {
    // Two surveys of the same tree must agree exactly: the report is a measurement,
    // and a measurement that wobbles between runs cannot be pinned.
    let root = repo_root();
    let first = survey(&root);
    let second = survey(&root);
    assert_eq!(first, second);
    assert_eq!(first.table(), second.table());
    assert_eq!(
        baseline::fixture_text(&first),
        baseline::fixture_text(&second)
    );
}

#[test]
fn the_fixture_is_what_the_binary_prints() {
    // The generator writes the fixture from `report --fixture`, so the two must be
    // byte-identical: a reader can reproduce the pin with one command.
    let (code, out, _) = run(&["report", "--fixture"]);
    assert_eq!(code, 0);
    assert_eq!(out, FIXTURE, "run `make gen-d0-vectors` to refresh the pin");
}
