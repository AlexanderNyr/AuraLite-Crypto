//! H7 OpSec gates (docs/h7-opsec-plan.md §2).
//!
//! Validator-key hygiene: keystore files stay owner-only (save `0600`, the
//! loader rejects loose permissions), and key access is logged without any
//! secret material.
//! `tools/nc_h7.py` injects the bugs these tests must catch.

use std::path::Path;

use aura_wallet::keystore::{KdfParams, Keystore, access_log_line, append_access_log};
use aura_wallet::secret::{Password, SecretSeed};

fn test_kdf() -> KdfParams {
    #[cfg(feature = "insecure-test-kdf")]
    {
        KdfParams::testing()
    }
    #[cfg(not(feature = "insecure-test-kdf"))]
    {
        KdfParams::default_argon2id()
    }
}

fn test_keystore() -> Keystore {
    let seed = SecretSeed::new([7; 64]);
    let password = Password::new("h7-test-pass");
    #[cfg(feature = "insecure-test-kdf")]
    let ks = Keystore::encrypt_seed_insecure_for_testing(&seed, &password, test_kdf());
    #[cfg(not(feature = "insecure-test-kdf"))]
    let ks = Keystore::encrypt_seed(&seed, &password, test_kdf());
    ks.expect("keystore built")
}

#[test]
fn h7_keystore_owner_only() {
    let dir = std::env::temp_dir().join(format!("auralite-h7-opsec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("h7-ks.json");
    let _ = std::fs::remove_file(&path);

    let ks = test_keystore();
    ks.save_new(&path).expect("save_new succeeds");

    // Save side: the file must be owner-only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "H7: validator keys must stay owner-only");
    }

    // Load side: a valid keystore with loose permissions must be rejected.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            Keystore::load(&path).is_err(),
            "H7: keystore loader must reject loose permissions"
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(
            Keystore::load(&path).is_err(),
            "H7: keystore loader must reject loose permissions"
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(Keystore::load(&path).is_ok());
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn h7_access_log_without_secrets() {
    let dir = std::env::temp_dir().join(format!("auralite-h7-access-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ks_path = dir.join("validator-a.json");
    let log_path = dir.join("validator-a.access.log");

    // The record format is pinned and deterministic.
    let line = access_log_line("save", Path::new("validator-a.json"), true);
    assert_eq!(
        line, "keystore save file=validator-a.json ok=true",
        "H7: key access must be logged without secrets"
    );
    let failed = access_log_line("load", Path::new("validator-a.json"), false);
    assert_eq!(
        failed, "keystore load file=validator-a.json ok=false",
        "H7: key access must be logged without secrets"
    );

    // No secret material can appear: the record only carries op/file/ok.
    for needle in ["h7-test-pass", "password", "seed", "mnemonic", "-----"] {
        assert!(
            !line.contains(needle),
            "H7: key access must be logged without secrets"
        );
    }

    // The log file is created owner-only and appends both records.
    append_access_log(&log_path, &line).expect("append record");
    append_access_log(&log_path, &failed).expect("append record");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&log_path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o077,
            0,
            "H7: key access must be logged without secrets"
        );
    }
    let content = std::fs::read_to_string(&log_path).unwrap();
    assert_eq!(content, format!("{line}\n{failed}\n"));
    let _ = ks_path;

    std::fs::remove_dir_all(&dir).unwrap();
}
