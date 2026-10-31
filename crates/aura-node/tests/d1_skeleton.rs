//! D1 — the `aurad` skeleton gate: config, tracing, PID file, health probe.
//!
//! `NODE_PLAN` D1 lands the node process itself. Two things are asserted here:
//!
//! * configuration loads from TOML + env + CLI, and `chain-id` is mandatory —
//!   `aurad` refuses with exit 2 and names the `config` capability (D1);
//! * the process lifecycle works: tracing initialises, a PID file is written,
//!   the health probe reports Healthy when the PID file exists, and the binary
//!   accepts `--help`, `--version` and `report` while rejecting unknown commands.
//!
//! The refusal is tested through the real binary, not only the library — a stub
//! that exits 0 is the failure mode CONTRIBUTING.md §1 exists to prevent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_node::baseline::{self, EXIT_NO_WORKSPACE, EXIT_STUB, EXIT_USAGE, MISSING_CAPABILITIES};
use aura_node::config::CliConfig;
use aura_node::lifecycle::{self, HealthStatus};

const FIXTURE: &str = include_str!("fixtures/d1_skeleton.txt");

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

/// The fixture, parsed into single keys plus repeated `capability` lines.
fn fixture() -> (BTreeMap<String, String>, Vec<String>) {
    let mut singles = BTreeMap::new();
    let mut capabilities = Vec::new();
    for line in FIXTURE.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "capability" {
            capabilities.push(value.to_string());
        } else {
            singles.insert(key.to_string(), value.to_string());
        }
    }
    (singles, capabilities)
}

#[test]
fn config_chain_id_is_mandatory() {
    let cli = CliConfig::default();
    let err = aura_node::config::load(&cli).unwrap_err();
    assert!(err.to_string().contains("chain-id"));
    assert!(err.to_string().contains("mandatory"));
}

#[test]
fn config_cli_chain_id_wins() {
    let cli = CliConfig {
        chain_id: Some("cli-chain".to_string()),
        ..Default::default()
    };
    let config = aura_node::config::load(&cli).unwrap();
    assert_eq!(config.chain_id, "cli-chain");
    assert_eq!(config.data_dir, PathBuf::from(".aura"));
    assert_eq!(config.rpc_addr, "127.0.0.1:8545");
    assert_eq!(config.grpc_addr, "127.0.0.1:9090");
    assert!(config.validator_key.is_none());
}

#[test]
fn config_toml_unknown_field_is_rejected() {
    let dir = std::env::temp_dir().join(format!("aurad-d1-unknown-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("aurad.toml");
    std::fs::write(
        &config_path,
        "chain_id = \"test\"\nunknown_field = \"value\"\n",
    )
    .unwrap();
    let cli = CliConfig {
        config: Some(config_path),
        data_dir: Some(dir.join("data")),
        ..Default::default()
    };
    let error = aura_node::config::load(&cli).unwrap_err();
    assert!(
        matches!(error, aura_node::config::ConfigError::UnknownField(ref name) if name == "unknown_field"),
        "unexpected configuration error: {error:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lifecycle_init_tracing_succeeds() {
    let result = lifecycle::init_tracing();
    assert!(result.is_ok(), "structured tracing must initialise");
}

#[test]
fn lifecycle_health_probe_healthy_when_pid_exists() {
    let dir = std::env::temp_dir().join(format!("aurad-d1-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pid_file = dir.join("aurad.pid");
    std::fs::write(&pid_file, "12345\n").unwrap();
    assert_eq!(lifecycle::health_probe(&pid_file), HealthStatus::Healthy);
    assert_eq!(
        lifecycle::health_probe(Path::new("/nonexistent-aura-pid")),
        HealthStatus::Unhealthy
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lifecycle_signal_handle_starts_unset() {
    let handle = lifecycle::install_signal_handlers().unwrap();
    assert!(!handle.shutdown_requested());
}

#[test]
fn binary_help_and_version_succeed() {
    let output = Command::new(env!("CARGO_BIN_EXE_aurad"))
        .args(["--help"])
        .output()
        .expect("aurad runs");
    assert_eq!(output.status.code(), Some(0));
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("usage:"));
    assert!(out.contains("report"));
    assert!(out.contains("--version"));

    let output = Command::new(env!("CARGO_BIN_EXE_aurad"))
        .args(["--version"])
        .output()
        .expect("aurad runs");
    assert_eq!(output.status.code(), Some(0));
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("aurad"));
    assert!(out.contains("D6 observability"));
}

#[test]
fn binary_unknown_command_is_usage_error() {
    let (code, _, err) = run(&["serve"]);
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("unknown command `serve`"));
    assert!(err.contains("usage:"));
}

#[test]
fn binary_without_chain_id_refuses_and_names_d1() {
    let output = Command::new(env!("CARGO_BIN_EXE_aurad"))
        .output()
        .expect("aurad runs");
    assert_eq!(output.status.code(), Some(EXIT_STUB));
    let out = String::from_utf8_lossy(&output.stdout);
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(out.is_empty(), "refusal goes to stderr, got stdout {out:?}");
    assert!(
        err.contains("D1"),
        "the refusal must name the phase that owns config: {err}"
    );
    assert!(
        err.contains("chain-id"),
        "the refusal must mention chain-id: {err}"
    );
}

#[test]
fn report_measures_this_tree() {
    let (code, out, _) = run(&["report"]);
    assert_eq!(code, 0);
    assert!(out.contains("aura-exec"));
    assert!(out.contains("implemented"));
    // D1 config/lifecycle and the D2 transaction pipeline have landed; the
    // report lists the remaining six externally facing node capabilities.
    assert!(
        out.contains("node capabilities present: 4 of 8"),
        "the report must state how many capabilities are still absent: {out}"
    );
}

#[test]
fn report_refuses_a_tree_that_is_not_a_workspace() {
    let (code, _, err) = run(&["report", "--workspace", "/nonexistent-auralite"]);
    assert_eq!(code, EXIT_NO_WORKSPACE);
    assert!(err.contains("is not a workspace"));

    let output = Command::new(env!("CARGO_BIN_EXE_aurad"))
        .args(["report", "--workspace", "/nonexistent-auralite"])
        .output()
        .expect("aurad runs");
    assert_eq!(output.status.code(), Some(EXIT_NO_WORKSPACE));
}

#[test]
fn the_pinned_fixture_matches_the_live_measurement() {
    let (singles, capabilities) = fixture();
    assert_eq!(singles["phase"], baseline::PHASE);
    assert_eq!(singles["exit_stub"], EXIT_STUB.to_string());
    assert_eq!(singles["exit_usage"], EXIT_USAGE.to_string());
    assert_eq!(singles["exit_no_workspace"], EXIT_NO_WORKSPACE.to_string());
    assert_eq!(
        singles["capabilities"],
        MISSING_CAPABILITIES.len().to_string()
    );

    // The remaining missing capabilities must match MISSING_CAPABILITIES exactly.
    let expected: Vec<String> = MISSING_CAPABILITIES
        .iter()
        .map(|c| format!("{} {} {}", c.name, c.phase, c.what))
        .collect();
    assert_eq!(capabilities, expected);

    // Capabilities remaining after D4 must still be enumerated exactly.
    for cap in ["devnet", "cli"] {
        assert!(
            capabilities.iter().any(|line| line.starts_with(cap)),
            "capability {cap} must still be listed as missing after D4"
        );
    }
}
