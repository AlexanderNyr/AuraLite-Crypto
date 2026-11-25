//! W5 gate — 10 hardware scenarios ("human confirmed/denied") for the
//! HID/CCID-like device protocol with on-screen confirmation
//! (`tests/fixtures/w5_hardware_vectors.json`).
//!
//! Gate text (WALLET_PLAN row W5): "10 \"human confirmed/denied\" scenarios;
//! NC: blind confirmation → red" — `blind_confirmation_rejected` is that
//! named control.
//!
//! Vectors come from the independent oracle `tools/gen_w5_vectors.py`; this
//! runner materializes each case in a fresh sandbox and asserts exit code and
//! both output streams byte-for-byte (model: `tests/w4_multisig.rs`). The
//! suite runs with `--features insecure-test-kdf` (setup uses `--kdf testing`).

#![cfg(feature = "insecure-test-kdf")]

const VECTORS: &str = include_str!("fixtures/w5_hardware_vectors.json");

fn substitute(text: &str, sandbox: &str, phrases: &serde_json::Value) -> String {
    let mut out = text.replace("{sandbox}", sandbox);
    if let Some(map) = phrases.as_object() {
        for (token, phrase) in map {
            out = out.replace(&format!("{{{token}}}"), phrase.as_str().expect("phrase"));
        }
    }
    out
}

fn run_argv(argv: &[String]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = aura_wallet::cli::run(argv, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).expect("stdout utf-8"),
        String::from_utf8(err).expect("stderr utf-8"),
    )
}

/// Materialize a vector case in a fresh sandbox and run its setup steps.
fn materialize(name: &str) -> (std::path::PathBuf, serde_json::Value, serde_json::Value) {
    let doc: serde_json::Value = serde_json::from_str(VECTORS).expect("vectors parse");
    let phrases = doc
        .get("phrases")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let cases = doc["cases"].as_array().expect("cases array");
    let case = cases
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("vector {name} not found"))
        .clone();

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let sandbox =
        std::env::temp_dir().join(format!("w5vec-{}-{serial}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sandbox);
    std::fs::create_dir_all(&sandbox).expect("sandbox created");
    let sandbox_str = sandbox.to_string_lossy().to_string();

    for (rel, content) in case["files"].as_object().expect("files object") {
        let path = sandbox.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("file parent");
        }
        std::fs::write(&path, content.as_str().expect("file content")).expect("file written");
    }

    for setup in case["setup"].as_array().expect("setup array") {
        let argv: Vec<String> = setup
            .as_array()
            .expect("setup argv")
            .iter()
            .map(|a| substitute(a.as_str().expect("argv str"), &sandbox_str, &phrases))
            .collect();
        let (code, _, err) = run_argv(&argv);
        assert_eq!(code, 0, "setup step failed: {err}");
    }
    (sandbox, case, phrases)
}

fn run_case(name: &str) {
    let (sandbox, case, phrases) = materialize(name);
    let sandbox_str = sandbox.to_string_lossy().to_string();
    let argv: Vec<String> = case["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|a| substitute(a.as_str().expect("argv str"), &sandbox_str, &phrases))
        .collect();
    let (code, out, err) = run_argv(&argv);

    let expected_exit = case["expect_exit"].as_i64().expect("expect_exit") as i32;
    let expect_lines = |key: &str| -> String {
        let mut text = String::new();
        for line in case[key].as_array().expect(key) {
            text.push_str(&substitute(
                line.as_str().expect("line"),
                &sandbox_str,
                &phrases,
            ));
            text.push('\n');
        }
        text
    };

    assert_eq!(
        code, expected_exit,
        "vector {name}: exit mismatch\nstdout:\n{out}\nstderr:\n{err}"
    );
    assert_eq!(
        out,
        expect_lines("expect_stdout"),
        "vector {name}: stdout mismatch"
    );
    assert_eq!(
        err,
        expect_lines("expect_stderr"),
        "vector {name}: stderr mismatch"
    );

    let _ = std::fs::remove_dir_all(&sandbox);
}

#[test]
fn vectors_file_has_exactly_10_unique_cases() {
    let doc: serde_json::Value = serde_json::from_str(VECTORS).expect("vectors parse");
    let cases = doc["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 10, "the W5 gate requires exactly 10 vectors");
    let mut names: Vec<&str> = cases
        .iter()
        .map(|c| c["name"].as_str().expect("case name"))
        .collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 10, "vector names must be unique");
}

macro_rules! vec_case {
    ($name:ident) => {
        #[test]
        fn $name() {
            run_case(stringify!($name));
        }
    };
}

vec_case!(confirm_transfer_basic);
vec_case!(deny_transfer_basic);
vec_case!(confirm_transfer_high_value);
vec_case!(deny_transfer_high_value);
vec_case!(blind_confirmation_rejected);
vec_case!(display_recipient_substitution);
vec_case!(deny_then_retry_confirm);
vec_case!(confirm_delegate);
vec_case!(undecodable_payload_refused);
vec_case!(foreign_chain_refused);
