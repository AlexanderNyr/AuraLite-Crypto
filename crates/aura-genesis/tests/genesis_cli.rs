//! C8 gate: the `aura genesis` CLI must agree with the library and vectors.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_aura");
const GOLDEN: &str = include_str!("fixtures/c8_golden.txt");

fn fixture(key: &str) -> String {
    for line in GOLDEN.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || !trimmed.contains('=') {
            continue;
        }
        let (name, value) = trimmed.split_once('=').unwrap();
        if name == key {
            return value.to_owned();
        }
    }
    panic!("missing fixture key {key}");
}

fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/genesis-testnet.json")
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("aura binary runs")
}

#[test]
fn cli_hash_matches_the_independent_vector() {
    let path = sample_path();
    let output = run(&["genesis", "hash", "--file", path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        fixture("genesis_1_hash")
    );
}

#[test]
fn cli_verify_accepts_the_right_hash_and_rejects_a_wrong_one() {
    let path = sample_path();
    let good = fixture("genesis_1_hash");
    let output = run(&[
        "genesis",
        "verify",
        "--file",
        path.to_str().unwrap(),
        "--expect",
        &good,
    ]);
    assert!(
        output.status.success(),
        "verify should pass: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("OK "));

    let bad = format!("{:0>64}", "0");
    let output = run(&[
        "genesis",
        "verify",
        "--file",
        path.to_str().unwrap(),
        "--expect",
        &bad,
    ]);
    assert!(!output.status.success(), "verify must reject a wrong hash");
    assert!(String::from_utf8_lossy(&output.stderr).contains("MISMATCH"));
}

#[test]
fn cli_show_reports_all_commitments() {
    let path = sample_path();
    let output = run(&["genesis", "show", "--file", path.to_str().unwrap()]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(&format!("genesis_hash={}", fixture("genesis_1_hash"))));
    assert!(text.contains(&format!(
        "validator_set_hash={}",
        fixture("validator_set_hash_1")
    )));
    assert!(text.contains(&format!("allocation_root={}", fixture("allocation_root_1"))));
    assert!(text.contains(&format!("total_supply={}", fixture("total_supply_1"))));
}

#[test]
fn cli_prints_usage_on_bad_arguments() {
    let output = run(&["nonsense"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("aura genesis hash"));
}
