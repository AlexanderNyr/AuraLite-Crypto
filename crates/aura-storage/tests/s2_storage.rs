//! S2 gate: WAL framing, atomic RocksDB batches, fsync, reopen and `kill -9`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aura_codec::Reader;
use aura_storage::{
    CF_META, CF_NONCE, CF_STATE, CF_WAL_INDEX, S2_HEAD_CONTEXT, S2_KEY_CONTEXT, S2_ROOT_CONTEXT,
    S2_WAL_CONTEXT, Storage, StorageError, WAL_MAGIC, WalRecord, decode_record, encode_record,
    head_checksum, key_path, state_root, vector_changes,
};
use aura_types::Hash;

const GOLDEN: &str = include_str!("fixtures/s2_golden.txt");
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn parse(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .unwrap_or_else(|| panic!("malformed golden line: {trimmed}"));
        assert!(map.insert(key.to_owned(), value.to_owned()).is_none());
        assert!(!key.is_empty());
    }
    map
}

fn hx(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("invalid hex nibble"),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hash32(text: &str) -> Hash {
    Hash::from_slice(&hx(text)).expect("32-byte hash")
}

fn u64_f(f: &BTreeMap<String, String>, key: &str) -> u64 {
    f[key].parse().expect("u64")
}

fn temp_dir(tag: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("aura-s2-{tag}-{}-{unique}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn writer_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_s2-writer")
        .or(option_env!("CARGO_BIN_EXE_s2_writer"))
        .expect("s2-writer binary must be built")
}

fn golden_record(f: &BTreeMap<String, String>, index: u64) -> WalRecord {
    WalRecord {
        version: index,
        parent_root: hash32(&f[&format!("commit_{index}_parent")]),
        state_root: hash32(&f[&format!("commit_{index}_root")]),
        changes: vector_changes(index),
    }
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(WAL_MAGIC), f["wal_magic"]);
    assert_eq!(to_hex(S2_WAL_CONTEXT), f["wal_context"]);
    assert_eq!(to_hex(S2_ROOT_CONTEXT), f["root_context"]);
    assert_eq!(to_hex(S2_KEY_CONTEXT), f["key_context"]);
    assert_eq!(to_hex(S2_HEAD_CONTEXT), f["head_context"]);
    assert_eq!(u64_f(&f, "commit_count"), 11);
    assert_eq!(u64_f(&f, "kill_points"), 10);
}

#[test]
fn wal_records_match_independent_python() {
    let f = parse(GOLDEN);
    let committed = u64_f(&f, "commit_count");
    let mut state: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    for index in 1..=committed {
        for change in vector_changes(index) {
            match change.value {
                Some(value) => {
                    state.insert(change.key, value);
                }
                None => {
                    state.remove(&change.key);
                }
            }
        }
        let record = golden_record(&f, index);
        assert_eq!(
            to_hex(&encode_record(&record)),
            f[&format!("commit_{index}_wal")],
            "commit {index} WAL bytes"
        );
        let wal_bytes = hx(&f[&format!("commit_{index}_wal")]);
        let mut reader = Reader::new(&wal_bytes);
        assert_eq!(decode_record(&mut reader).unwrap(), record);
        assert_eq!(reader.remaining_len(), 0);
        assert_eq!(
            record.version.to_string(),
            f[&format!("commit_{index}_version")]
        );
        assert_eq!(
            record.changes.len().to_string(),
            f[&format!("commit_{index}_changes")]
        );
        assert_eq!(
            state.len().to_string(),
            f[&format!("commit_{index}_state_count")]
        );

        // Independently computed versioned state root.
        let leaves = state
            .iter()
            .map(|(key, value)| (key_path(key), value.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            state_root(index, &leaves).to_hex(),
            f[&format!("commit_{index}_root")]
        );
        assert_eq!(
            head_checksum(index, &record.state_root).to_hex(),
            f[&format!("commit_{index}_head_checksum")]
        );
        assert_eq!(
            u64_f(&f, &format!("commit_{index}_wal_offset")),
            record_offset(&f, index),
        );
    }
    assert_eq!(to_hex(&hx(&f["wal_file_all"])), f["wal_file_all"]);
    assert_eq!(hash32(&f["final_root"]).to_hex(), f["final_root"]);
    assert_eq!(state.len().to_string(), f["final_state_count"]);
}

fn record_offset(f: &BTreeMap<String, String>, index: u64) -> u64 {
    let mut offset = 0u64;
    for i in 1..index {
        offset += hx(&f[&format!("commit_{i}_wal")]).len() as u64;
    }
    offset
}

#[test]
fn commit_batch_and_reopen_match_golden() {
    let f = parse(GOLDEN);
    let dir = temp_dir("commit");
    let committed = u64_f(&f, "commit_count");
    {
        let mut storage = Storage::open(&dir).expect("open");
        assert_eq!(storage.head().version, 0);
        assert_eq!(storage.head().root, Hash::ZERO);
        for index in 1..=committed {
            let receipt = storage
                .commit(vector_changes(index))
                .unwrap_or_else(|e| panic!("commit {index}: {e}"));
            assert_eq!(receipt.version, index);
            assert_eq!(receipt.root.to_hex(), f[&format!("commit_{index}_root")]);
            assert_eq!(
                receipt.wal_offset,
                u64_f(&f, &format!("commit_{index}_wal_offset"))
            );
            assert_eq!(storage.head().version, index);
            assert_eq!(storage.head().root, receipt.root);
            assert_eq!(storage.wal_record_count() as u64, index);
            assert_eq!(storage.wal_index(index).unwrap(), Some(receipt.wal_offset));
            storage
                .verify_state_root()
                .unwrap_or_else(|e| panic!("verify {index}: {e}"));
            let first = &vector_changes(index)[0];
            assert_eq!(storage.get(&first.key).unwrap(), first.value);
            assert_eq!(storage.nonce(&first.key).unwrap(), Some(first.nonce));
            assert_eq!(
                storage.entries().unwrap().len() as u64,
                u64_f(&f, &format!("commit_{index}_state_count"))
            );
        }
        assert_eq!(
            fs::read(storage.wal_path()).unwrap(),
            hx(&f["wal_file_all"]),
            "WAL file bytes"
        );
    }
    let reopened = Storage::open(&dir).expect("reopen");
    assert_eq!(reopened.head().version, committed);
    assert_eq!(reopened.head().root, hash32(&f["final_root"]));
    assert_eq!(reopened.wal_record_count() as u64, committed);
    reopened.verify_state_root().expect("reopened root");
    assert_eq!(
        reopened.entries().unwrap().len(),
        hx_len(&f["final_state_count"])
    );
    for cf in [CF_STATE, CF_NONCE, CF_META, CF_WAL_INDEX] {
        assert!(!cf.is_empty());
    }
}

fn hx_len(value: &str) -> usize {
    value.parse().expect("usize")
}

#[test]
fn wal_only_record_is_replayed_on_reopen() {
    let f = parse(GOLDEN);
    let dir = temp_dir("walonly");
    {
        let mut storage = Storage::open(&dir).unwrap();
        for index in 1..=5 {
            storage.commit(vector_changes(index)).unwrap();
        }
        let receipt = storage.append_wal_only(vector_changes(6)).unwrap();
        assert_eq!(receipt.version, 6);
        assert_eq!(receipt.root.to_hex(), f["commit_6_root"]);
        assert_eq!(storage.head().version, 5, "head must not be published yet");
        assert_eq!(storage.wal_record_count(), 6);
    }
    let recovered = Storage::open(&dir).expect("recover");
    assert_eq!(recovered.head().version, 6);
    assert_eq!(recovered.head().root.to_hex(), f["commit_6_root"]);
    recovered.verify_state_root().expect("recovered root");
    assert_eq!(
        recovered.wal_index(6).unwrap(),
        Some(u64_f(&f, "commit_6_wal_offset"))
    );
}

#[test]
fn missing_wal_record_is_rejected() {
    let dir = temp_dir("missingwal");
    {
        let mut storage = Storage::open(&dir).unwrap();
        for index in 1..=3 {
            storage.commit(vector_changes(index)).unwrap();
        }
        let wal = fs::read(storage.wal_path()).unwrap();
        let cut = u64_f(&parse(GOLDEN), "commit_3_wal_offset") as usize;
        fs::write(storage.wal_path(), &wal[..cut]).unwrap();
    }
    assert_eq!(
        Storage::open(&dir).err(),
        Some(StorageError::MissingWalRecord)
    );
}

#[test]
fn bad_wal_vectors_are_rejected() {
    let f = parse(GOLDEN);
    let expected: Vec<(&str, StorageError)> = vec![
        ("bad_checksum_wal", StorageError::ChecksumMismatch),
        ("bad_magic_wal", StorageError::CorruptWal),
        ("bad_truncated_wal", StorageError::CorruptWal),
        ("bad_trailing_wal", StorageError::CorruptWal),
        ("bad_nonseq_wal", StorageError::NonSequentialWal),
        ("bad_parent_wal", StorageError::ParentRootMismatch),
        ("bad_root_wal", StorageError::RootMismatch),
        ("bad_value_wal", StorageError::RootMismatch),
    ];
    for (key, error) in &expected {
        let dir = temp_dir(key);
        fs::write(dir.join("wal.log"), hx(&f[*key])).unwrap();
        assert_eq!(Storage::open(&dir).err(), Some(error.clone()), "{key}");
    }
}

#[test]
fn kill_9_at_ten_commit_points() {
    let f = parse(GOLDEN);
    for count in 1..=10u64 {
        let dir = temp_dir("kill9");
        let status = std::process::Command::new(writer_bin())
            .args([
                "--dir",
                dir.to_str().unwrap(),
                "--commits",
                &count.to_string(),
            ])
            .status()
            .expect("spawn writer");
        assert!(!status.success(), "writer must be killed");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(9),
            "SIGKILL at commit {count}"
        );
        let recovered = Storage::open(&dir).unwrap_or_else(|e| panic!("reopen after {count}: {e}"));
        assert_eq!(recovered.head().version, count, "head after {count}");
        assert_eq!(
            recovered.head().root.to_hex(),
            f[&format!("commit_{count}_root")]
        );
        assert_eq!(recovered.wal_record_count() as u64, count);
        recovered.verify_state_root().unwrap();
        assert_eq!(
            recovered.entries().unwrap().len(),
            hx_len(&f[&format!("commit_{count}_state_count")])
        );
    }
}

#[test]
fn kill_9_between_wal_and_head_replays() {
    let f = parse(GOLDEN);
    for count in 1..=10u64 {
        let dir = temp_dir("kill9wal");
        let status = std::process::Command::new(writer_bin())
            .args([
                "--dir",
                dir.to_str().unwrap(),
                "--commits",
                &count.to_string(),
                "--crash-after-wal",
            ])
            .status()
            .expect("spawn writer");
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(9),
            "SIGKILL at crash window {count}"
        );
        let recovered = Storage::open(&dir).unwrap_or_else(|e| panic!("reopen after {count}: {e}"));
        assert_eq!(
            recovered.head().version,
            count + 1,
            "replayed head after {count}"
        );
        assert_eq!(
            recovered.head().root.to_hex(),
            f[&format!("commit_{}_root", count + 1)]
        );
        assert_eq!(recovered.wal_record_count() as u64, count + 1);
        recovered.verify_state_root().unwrap();
    }
}

#[test]
fn clean_writer_exits_zero_and_head_matches() {
    let f = parse(GOLDEN);
    let dir = temp_dir("clean");
    let output = std::process::Command::new(writer_bin())
        .args(["--dir", dir.to_str().unwrap(), "--commits", "4", "--clean"])
        .output()
        .expect("run clean writer");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("CLEAN 4"));
    let storage = Storage::open(&dir).unwrap();
    assert_eq!(storage.head().version, 4);
    assert_eq!(storage.head().root.to_hex(), f["commit_4_root"]);
}
