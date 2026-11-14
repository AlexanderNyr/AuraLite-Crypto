//! W4 gate — 22 CLI vectors for the wallet-level M-of-N multisig contract
//! (`tests/fixtures/w4_multisig_vectors.json`).
//!
//! Gate text (WALLET_PLAN row W4): "3-of-5: 3 collected → success, 2 →
//! reject; NC: threshold bypass → red" — `collect_2_of_5_rejected` is that
//! named control.
//!
//! Vectors come from the independent oracle `tools/gen_w4_vectors.py`; this
//! runner materializes each case in a fresh sandbox and asserts exit code and
//! both output streams byte-for-byte (model: `tests/w3_offline.rs`). The suite
//! runs with `--features insecure-test-kdf` (setup uses `--kdf testing`).

#![cfg(feature = "insecure-test-kdf")]

const VECTORS: &str = include_str!("fixtures/w4_multisig_vectors.json");

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
        std::env::temp_dir().join(format!("w4vec-{}-{serial}-{name}", std::process::id()));
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
fn vectors_file_has_exactly_22_unique_cases() {
    let doc: serde_json::Value = serde_json::from_str(VECTORS).expect("vectors parse");
    let cases = doc["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 22, "the W4 gate requires exactly 22 vectors");
    let mut names: Vec<&str> = cases
        .iter()
        .map(|c| c["name"].as_str().expect("case name"))
        .collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 22, "vector names must be unique");
}

macro_rules! vec_case {
    ($name:ident) => {
        #[test]
        fn $name() {
            run_case(stringify!($name));
        }
    };
}

vec_case!(init_basic);
vec_case!(init_threshold_zero);
vec_case!(init_threshold_gt_n);
vec_case!(init_duplicate_member);
vec_case!(propose_transfer_basic);
vec_case!(propose_unknown_action);
vec_case!(propose_foreign_chain);
vec_case!(propose_expiry_gt_timeout);
vec_case!(propose_rotate);
vec_case!(approve_basic);
vec_case!(approve_not_a_member);
vec_case!(approve_expired);
vec_case!(approve_foreign_chain);
vec_case!(collect_3_of_5_passed);
vec_case!(collect_2_of_5_rejected);
vec_case!(collect_duplicate_member);
vec_case!(collect_forged_signature);
vec_case!(collect_foreign_proposal);
vec_case!(collect_expired);
vec_case!(collect_wrong_contract);
vec_case!(rotate_3_of_5_passed);
vec_case!(e2e_full_flow);
