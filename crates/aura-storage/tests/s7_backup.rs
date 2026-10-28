//! S7 — backup, restore and the chaos gate (DoD).
//!
//! The DoD line is: *50 `kill -9` + restore cycles, zero root mismatches*.
//! Each cycle SIGKILLs a writer process (sometimes between the WAL fsync and
//! head publication), reopens the store, compares head and root against the
//! INDEPENDENT Python fixture, backs the store up, restores the bundle into a
//! fresh directory and compares again. Every layer of the bundle
//! (manifest checksum, blob SHA-256, per-record BLAKE3, version sequence,
//! parent-root chain, replayed roots, head, leaves) is verified.
//!
//! One process, one machine, no network: `validators / blocks / tx` stay
//! 0 / 0 / 0.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use aura_storage::{
    BACKUP_FORMAT_VERSION, BackupBundle, BackupError, BackupManifest, MANIFEST_MAGIC,
    S7_MANIFEST_CONTEXT, Storage, create_backup, restore_backup, vector_changes,
};
use aura_types::Hash;

const GOLDEN: &str = include_str!("fixtures/s7_golden.txt");
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn fixture() -> &'static BTreeMap<String, String> {
    static PARSED: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        let mut map = BTreeMap::new();
        for line in GOLDEN.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let (key, value) = trimmed
                .split_once('=')
                .unwrap_or_else(|| panic!("malformed golden line: {trimmed}"));
            assert!(map.insert(key.to_owned(), value.to_owned()).is_none());
        }
        map
    })
}

fn chain_id() -> &'static str {
    &fixture()["chain_id"]
}

fn number(key: &str) -> u64 {
    fixture()[key].parse().expect("numeric fixture value")
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn root_at(version: u64) -> Hash {
    let key = format!("root_at_{version}");
    Hash::from_hex(&fixture()[&key]).expect("fixture root is hex")
}

fn temp_dir(tag: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("aura-s7-{tag}-{}-{unique}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn chaos_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_s7-chaos")
        .or(option_env!("CARGO_BIN_EXE_s7_chaos"))
        .expect("s7-chaos binary must be built")
}

/// A store with `commits` deterministic records.
fn store_with(dir: &Path, commits: u64) -> Storage {
    let mut storage = Storage::open(dir).expect("open store");
    for index in 1..=commits {
        let receipt = storage.commit(vector_changes(index)).expect("commit");
        assert_eq!(receipt.version, index);
    }
    storage
}

#[test]
fn constants_and_golden_bundle_match_the_independent_model() {
    assert_eq!(to_hex(MANIFEST_MAGIC), fixture()["manifest_magic"]);
    assert_eq!(to_hex(S7_MANIFEST_CONTEXT), fixture()["manifest_context"]);
    assert_eq!(BACKUP_FORMAT_VERSION, number("format_version") as u32);

    let dir = temp_dir("golden");
    let store = store_with(&dir, number("golden_commits"));
    let bundle_bytes = create_backup(&store, chain_id()).expect("backup");
    assert_eq!(
        to_hex(&bundle_bytes),
        fixture()["golden_bundle_hex"],
        "bundle bytes must match the independent Python encoder"
    );

    let bundle = BackupBundle::parse(&bundle_bytes).expect("parse bundle");
    assert_eq!(bundle.manifest.chain_id, chain_id());
    assert_eq!(bundle.manifest.head_version, number("golden_commits"));
    assert_eq!(bundle.manifest.head_root, store.head().root);
    assert_eq!(bundle.manifest.record_count, number("golden_commits"));
    assert_eq!(bundle.manifest.blob_len, number("golden_blob_len"));
    assert_eq!(
        to_hex(bundle.manifest.to_bytes().as_slice()),
        fixture()["golden_manifest_hex"]
    );
    assert_eq!(
        to_hex(bundle.manifest.blob_sha256.as_bytes()),
        fixture()["golden_blob_sha256"]
    );
    assert_eq!(bundle_bytes.len() as u64, number("golden_bundle_len"));
    assert_eq!(
        bundle.records().expect("records").len() as u64,
        number("golden_commits")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn backup_restore_round_trip_is_byte_identical() {
    let source_dir = temp_dir("roundtrip-src");
    let source = store_with(&source_dir, 8);
    let first = create_backup(&source, chain_id()).expect("backup");
    let second = create_backup(&source, chain_id()).expect("backup again");
    assert_eq!(first, second, "backup must be deterministic");

    let target_dir = temp_dir("roundtrip-dst");
    let report = restore_backup(&first, &target_dir, chain_id()).expect("restore");
    assert_eq!(report.records, 8);
    assert_eq!(report.head_version, source.head().version);
    assert_eq!(report.head_root, source.head().root);
    assert_eq!(report.chain_id, chain_id());

    let restored = Storage::open(&target_dir).expect("reopen restored store");
    assert_eq!(restored.head(), source.head());
    assert_eq!(
        restored.wal_records(),
        source.wal_records(),
        "restored WAL must equal the source WAL record for record"
    );
    restored
        .verify_state_root()
        .expect("restored root verifies");
    // A backup of the restored store is the same bundle again.
    assert_eq!(create_backup(&restored, chain_id()).unwrap(), first);

    // Both stores continue identically.
    let mut source = source;
    let mut restored = restored;
    let left = source.commit(vector_changes(9)).expect("commit 9");
    let right = restored.commit(vector_changes(9)).expect("commit 9");
    assert_eq!(left, right, "a restored store must stay deterministic");
    let _ = fs::remove_dir_all(&source_dir);
    let _ = fs::remove_dir_all(&target_dir);
}

#[test]
fn restore_rejects_every_tampered_bundle() {
    let source_dir = temp_dir("tamper-src");
    let source = store_with(&source_dir, 5);
    let bundle = create_backup(&source, chain_id()).expect("backup");

    // 1. A flipped blob byte: the SHA-256 layer must catch it.
    let mut flipped_blob = bundle.clone();
    let last = flipped_blob.len() - 1;
    flipped_blob[last] ^= 1;
    assert_eq!(
        BackupBundle::parse(&flipped_blob).map(|_| ()),
        Err(BackupError::BlobHashMismatch)
    );

    // 2. A flipped manifest byte: the manifest checksum must catch it.
    let mut flipped_manifest = bundle.clone();
    flipped_manifest[10] ^= 1;
    assert!(matches!(
        BackupBundle::parse(&flipped_manifest).map(|_| ()),
        Err(BackupError::ChecksumMismatch)
            | Err(BackupError::BadMagic)
            | Err(BackupError::Truncated)
    ));

    // 3. Truncation and trailing bytes.
    let truncated = &bundle[..bundle.len() - 10];
    assert!(matches!(
        BackupBundle::parse(truncated).map(|_| ()),
        Err(BackupError::Truncated) | Err(BackupError::LengthMismatch { .. })
    ));
    let mut trailing = bundle.clone();
    trailing.push(0);
    assert!(matches!(
        BackupBundle::parse(&trailing).map(|_| ()),
        Err(BackupError::LengthMismatch { .. })
    ));

    // 4. A bad magic in the manifest.
    let mut bad_magic = bundle.clone();
    bad_magic[4] ^= 1;
    assert!(matches!(
        BackupBundle::parse(&bad_magic).map(|_| ()),
        Err(BackupError::BadMagic) | Err(BackupError::ChecksumMismatch)
    ));

    // 5. Wrong chain-id.
    let target = temp_dir("tamper-dst");
    match restore_backup(&bundle, &target, "auralite-1") {
        Err(BackupError::WrongChainId { expected, got }) => {
            assert_eq!(expected, "auralite-1");
            assert_eq!(got, chain_id());
        }
        other => panic!("expected WrongChainId, got {other:?}"),
    }

    // 6. Restore refuses to merge into a live store.
    let live = temp_dir("tamper-live");
    let _store = store_with(&live, 2);
    assert_eq!(
        restore_backup(&bundle, &live, chain_id()),
        Err(BackupError::TargetNotEmpty)
    );

    let _ = fs::remove_dir_all(&source_dir);
    let _ = fs::remove_dir_all(&target);
    let _ = fs::remove_dir_all(&live);
}

#[test]
fn restore_replays_instead_of_trusting_the_manifest() {
    let source_dir = temp_dir("lying-src");
    let source = store_with(&source_dir, 4);
    let bundle_bytes = create_backup(&source, chain_id()).expect("backup");
    let bundle = BackupBundle::parse(&bundle_bytes).expect("parse");

    // A perfectly checksummed manifest that lies about the head root.
    let mut lying = bundle.manifest.clone();
    lying.head_root = Hash::from_bytes([0x42u8; 32]);
    let lying_bundle = BackupBundle {
        manifest: lying,
        blob: bundle.blob.clone(),
    }
    .to_bytes();
    let target = temp_dir("lying-dst");
    assert_eq!(
        restore_backup(&lying_bundle, &target, chain_id()),
        Err(BackupError::HeadMismatch)
    );

    // A perfectly checksummed manifest that lies about the format version.
    let mut future = bundle.manifest.clone();
    future.format_version = BACKUP_FORMAT_VERSION + 1;
    assert_eq!(
        BackupManifest::from_bytes(&future.to_bytes()),
        Err(BackupError::UnsupportedVersion(BACKUP_FORMAT_VERSION + 1))
    );

    // A blob with one record's declared root changed but its frame
    // re-checksummed (a hostile peer re-signing a lie) must fail replay.
    let mut records = bundle.records().expect("records");
    records[1].state_root = Hash::from_bytes([0x11u8; 32]);
    let mut blob = Vec::new();
    for record in &records {
        blob.extend_from_slice(&aura_storage::encode_record(record));
    }
    let mut manifest = bundle.manifest.clone();
    manifest.blob_len = blob.len() as u64;
    manifest.blob_sha256 = aura_state::sha256_bytes(&blob);
    manifest.head_root = records.last().unwrap().state_root;
    let forged = BackupBundle { manifest, blob }.to_bytes();
    let forged_target = temp_dir("forged-dst");
    assert_eq!(
        restore_backup(&forged, &forged_target, chain_id()),
        Err(BackupError::RootMismatch { version: 2 }),
        "a re-checksummed lie must be caught by replaying the state"
    );

    let _ = fs::remove_dir_all(&source_dir);
    let _ = fs::remove_dir_all(&target);
    let _ = fs::remove_dir_all(&forged_target);
}

#[test]
fn restore_rejects_a_broken_parent_chain() {
    // A bundle whose frames all verify and whose roots are the true ones, but
    // where one record claims the wrong parent. The declared chain must be
    // checked, not just the replayed roots.
    let source_dir = temp_dir("chain-src");
    let source = store_with(&source_dir, 4);
    let bundle =
        BackupBundle::parse(&create_backup(&source, chain_id()).expect("backup")).expect("parse");

    let mut records = bundle.records().expect("records");
    records[2].parent_root = Hash::from_bytes([0x77u8; 32]);
    let mut blob = Vec::new();
    for record in &records {
        blob.extend_from_slice(&aura_storage::encode_record(record));
    }
    let mut manifest = bundle.manifest.clone();
    manifest.blob_len = blob.len() as u64;
    manifest.blob_sha256 = aura_state::sha256_bytes(&blob);
    let forged = BackupBundle { manifest, blob }.to_bytes();

    let target = temp_dir("chain-dst");
    assert_eq!(
        restore_backup(&forged, &target, chain_id()),
        Err(BackupError::ParentRootMismatch),
        "a broken parent chain must be rejected even when every checksum holds"
    );

    let _ = fs::remove_dir_all(&source_dir);
    let _ = fs::remove_dir_all(&target);
}

#[test]
fn empty_store_backup_round_trips() {
    let source_dir = temp_dir("empty-src");
    let source = Storage::open(&source_dir).expect("open empty store");
    let bundle = create_backup(&source, chain_id()).expect("backup");
    let parsed = BackupBundle::parse(&bundle).expect("parse");
    assert_eq!(parsed.manifest.head_version, 0);
    assert_eq!(parsed.manifest.head_root, Hash::ZERO);
    assert_eq!(parsed.manifest.record_count, 0);
    assert_eq!(parsed.manifest.blob_len, 0);

    let target = temp_dir("empty-dst");
    let report = restore_backup(&bundle, &target, chain_id()).expect("restore");
    assert_eq!(report.records, 0);
    assert_eq!(report.head_version, 0);
    assert_eq!(report.head_root, Hash::ZERO);
    let _ = fs::remove_dir_all(&source_dir);
    let _ = fs::remove_dir_all(&target);
}

#[test]
fn chaos_50_kill9_and_restore_cycles_zero_root_mismatches() {
    let cycles = number("chaos_cycles");
    let max_commits = number("max_commits");
    assert_eq!(cycles, 50, "the DoD asks for 50 cycles");

    let mut head_mismatches = 0u64;
    let mut restore_mismatches = 0u64;
    let mut continue_mismatches = 0u64;
    let mut recovered_records = 0u64;
    let mut crash_windows = 0u64;

    for cycle in 0..cycles {
        let commits = 1 + (cycle * 7) % max_commits;
        let crash_after_wal = cycle % 3 == 0;
        let dir = temp_dir(&format!("chaos-{cycle}"));

        let mut command = Command::new(chaos_bin());
        command
            .arg("--dir")
            .arg(&dir)
            .arg("--commits")
            .arg(commits.to_string());
        if crash_after_wal {
            command.arg("--crash-after-wal");
            crash_windows += 1;
        }
        let output = command.output().expect("spawn chaos writer");
        let status = output.status;
        assert!(
            !status.success() && status.code().is_none(),
            "cycle {cycle}: writer must die by SIGKILL, got {status:?}"
        );

        // Expected head: published records, plus the WAL-only record that the
        // reopen must replay when the crash landed in the commit window.
        let expected_version = commits + u64::from(crash_after_wal);
        let expected_root = root_at(expected_version);

        let store = Storage::open(&dir).expect("reopen after kill -9");
        if store.head().version != expected_version || store.head().root != expected_root {
            head_mismatches += 1;
            eprintln!(
                "cycle {cycle}: recovered head {}/{} != expected {}/{}",
                store.head().version,
                store.head().root.to_hex(),
                expected_version,
                expected_root.to_hex()
            );
            let _ = fs::remove_dir_all(&dir);
            continue;
        }
        recovered_records += store.wal_record_count() as u64;

        // Backup the recovered store and restore it into a fresh directory.
        let bundle = create_backup(&store, chain_id()).expect("backup");
        let target = temp_dir(&format!("chaos-restore-{cycle}"));
        let report = restore_backup(&bundle, &target, chain_id()).expect("restore");
        if report.head_version != expected_version || report.head_root != expected_root {
            restore_mismatches += 1;
        }
        let restored = Storage::open(&target).expect("open restored");
        if restored.head() != store.head() || restored.wal_records() != store.wal_records() {
            restore_mismatches += 1;
        }

        // Both stores must move on identically.
        if expected_version < max_commits + 1 {
            let mut left = store;
            let mut right = restored;
            let next = expected_version + 1;
            let left_receipt = left.commit(vector_changes(next)).expect("commit");
            let right_receipt = right.commit(vector_changes(next)).expect("commit");
            if left_receipt != right_receipt || left_receipt.root != root_at(next) {
                continue_mismatches += 1;
            }
        }

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&target);
    }

    println!(
        "chaos: {cycles} cycles ({crash_windows} in the WAL/head crash window), \
         {recovered_records} recovered WAL records, \
         head mismatches {head_mismatches}, restore mismatches {restore_mismatches}, \
         continuation mismatches {continue_mismatches}"
    );
    assert_eq!(head_mismatches, 0, "kill -9 recovery must never mismatch");
    assert_eq!(restore_mismatches, 0, "restore must never mismatch");
    assert_eq!(
        continue_mismatches, 0,
        "a restored store must stay deterministic"
    );
    assert!(recovered_records > 0, "cycles must have produced records");
}
