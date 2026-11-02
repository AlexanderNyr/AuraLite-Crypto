//! W0 integration gate: key-exposure surface audit, threat notes, and
//! tree/log secret-material scanner (`NC: key in a log -> gate red (tree search)`).

use aura_wallet::baseline::{
    BIP44_PATH_TEMPLATE, EXPOSURE_SURFACES, THREAT_NOTES, audit_tree_for_key_leaks,
    default_workspace_root, scan_text_for_key_leaks, survey,
};

fn synthesize_log_line(
    fmt_style: &str,
    marker: &str,
    payload_kind: &str,
    payload_units: usize,
) -> String {
    let payload = match payload_kind {
        "hex_repeat" => "a5".repeat(payload_units / 2),
        "alnum_repeat" => "9z".repeat(payload_units / 2),
        "words" => vec!["abandon"; payload_units].join(" "),
        "pem" => String::new(),
        other => panic!("unknown payload_kind: {other}"),
    };

    match fmt_style {
        "kv" => format!("2026-11-02T00:00:00Z INFO aurad {marker}={payload}"),
        "kv_0x" => format!("2026-11-02T00:00:00Z INFO aurad {marker}=0x{payload}"),
        "kv_quote" => format!("2026-11-02T00:00:00Z INFO aurad {marker}=\"{payload}\""),
        "json" => format!("{{\"level\":\"INFO\",\"{marker}\":\"{payload}\"}}"),
        "prefix_token" => format!("2026-11-02T00:00:00Z WARN key dump {marker}{payload}"),
        "pem_priv" => format!("-----BEGIN {marker} KEY-----"),
        "pem_pub" => format!("-----BEGIN {marker}-----"),
        other => panic!("unknown fmt_style: {other}"),
    }
}

#[test]
fn w0_tree_search_and_exposure_audit_gate() {
    let root = default_workspace_root();
    let s = survey(&root).expect("survey workspace");
    assert_eq!(s.implemented(), 0);
    assert_eq!(s.placeholders(), 4);
    assert_eq!(s.missing(), 0);
    assert_eq!(EXPOSURE_SURFACES.len(), 8);
    assert_eq!(THREAT_NOTES.len(), 6);
    assert_eq!(BIP44_PATH_TEMPLATE, "m/44'/ALCRY'/0'/0/i");

    // Verify the live repository tree has zero leaked keys in any log or file.
    let audit = audit_tree_for_key_leaks(&root).expect("audit workspace tree");
    assert!(
        audit.is_clean(),
        "live workspace tree search found leaked key material: {:?}",
        audit.findings
    );
    assert!(audit.scanned_files >= 100);
    assert!(audit.scanned_log_files >= 20);
}

#[test]
fn w0_key_audit_vector_fixture_matches_scanner_rules() {
    let fixture_text = include_str!("fixtures/w0_key_audit.txt");
    let valid_surfaces: Vec<&str> = EXPOSURE_SURFACES.iter().map(|s| s.id).collect();
    let mut count = 0usize;

    for raw_line in fixture_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('|').collect();
        assert_eq!(cols.len(), 9, "expected 9 columns in {line}");
        let case_id = cols[0].strip_prefix("case_id=").unwrap_or(cols[0]);
        let fmt_style = cols[2];
        let marker = cols[3];
        let payload_kind = cols[4];
        let payload_units: usize = cols[5].parse().unwrap();
        let expected_findings: usize = cols[6].parse().unwrap();
        let expected_kind = cols[7];
        let surface_id = cols[8];

        assert!(
            valid_surfaces.contains(&surface_id),
            "{case_id}: unknown exposure surface {surface_id}"
        );

        let log_line = synthesize_log_line(fmt_style, marker, payload_kind, payload_units);
        let findings = scan_text_for_key_leaks(case_id, &log_line);
        assert_eq!(
            findings.len(),
            expected_findings,
            "{case_id}: expected {expected_findings} finding(s), got {:?}",
            findings
        );
        if expected_findings > 0 {
            assert_eq!(
                findings[0].kind.as_str(),
                expected_kind,
                "{case_id}: kind mismatch"
            );
        } else {
            assert_eq!(expected_kind, "none");
        }
        count += 1;
    }

    assert_eq!(count, 64, "expected 64 vectors in w0_key_audit.txt");
}
