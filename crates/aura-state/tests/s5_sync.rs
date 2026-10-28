//! S5 gate: state sync against an independent Python reference.
//!
//! `fixtures/s5_golden.txt` comes from `tools/gen_s5_vectors.py` — hashlib
//! SHA-256, pure-Python BLAKE3, the C7 state-key digest, the S1 sparse Merkle
//! tree and the S4 snapshot format.
//! Phased report: `docs/evidence/s5/report.md`.

use std::collections::BTreeMap;
use std::time::Duration;

use aura_state::{
    DELTA_MAGIC, HEAD_MAGIC, MAX_DELTA_CHANGES, PACKAGE_MAGIC, S5_DELTA_CONTEXT, S5_HEAD_CONTEXT,
    S5_PACKAGE_CONTEXT, SYNC_QUORUM, SYNC_VERSION, SyncDelta, SyncError, SyncHead, SyncPackage,
    sha256_bytes, state_root,
};
use aura_types::{ChainId, Hash, Height};

const GOLDEN: &str = include_str!("fixtures/s5_golden.txt");

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

fn f_u64(f: &BTreeMap<String, String>, key: &str) -> u64 {
    f[key].parse().expect("u64")
}

fn f_usize(f: &BTreeMap<String, String>, key: &str) -> usize {
    f[key].parse().expect("usize")
}

/// Deterministic C7 account keys and compact values, identical to the Python
/// generator (`\x01 || 20-byte big-endian index` and `u64 LE`).
fn state_at(height: u64, count: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
    (0..count)
        .map(|index| {
            let mut key = vec![0x01u8];
            key.extend_from_slice(&[0u8; 12]);
            key.extend_from_slice(&(index as u64).to_be_bytes());
            (key, (height * 1000 + index as u64).to_le_bytes().to_vec())
        })
        .collect()
}

fn chain(f: &BTreeMap<String, String>) -> ChainId {
    ChainId::new(&f["chain_id"]).expect("valid chain-id")
}

fn package(f: &BTreeMap<String, String>) -> SyncPackage {
    SyncPackage::from_canonical_bytes(&hx(&f["package_bytes"])).expect("package decodes")
}

fn heads(f: &BTreeMap<String, String>) -> Vec<SyncHead> {
    (0..SYNC_QUORUM)
        .map(|index| {
            SyncHead::from_canonical_bytes(&hx(&f[&format!("head_{index}_bytes")])).unwrap()
        })
        .collect()
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(HEAD_MAGIC), f["head_magic"]);
    assert_eq!(to_hex(DELTA_MAGIC), f["delta_magic"]);
    assert_eq!(to_hex(PACKAGE_MAGIC), f["package_magic"]);
    assert_eq!(to_hex(S5_HEAD_CONTEXT), f["head_context"]);
    assert_eq!(to_hex(S5_DELTA_CONTEXT), f["delta_context"]);
    assert_eq!(to_hex(S5_PACKAGE_CONTEXT), f["package_context"]);
    assert_eq!(SYNC_VERSION.to_string(), f["version"]);
    assert_eq!(SYNC_QUORUM.to_string(), f["quorum"]);
    assert_eq!(MAX_DELTA_CHANGES, 1_000_000);
}

#[test]
fn golden_heads_delta_and_package_match_independent_python() {
    let f = parse(GOLDEN);
    let decoded_heads = heads(&f);
    for (index, head) in decoded_heads.iter().enumerate() {
        assert_eq!(to_hex(&head.to_bytes()), f[&format!("head_{index}_bytes")]);
        assert_eq!(head.chain_id, chain(&f));
        assert_eq!(head.height.value().to_string(), f["target_height"]);
        assert_eq!(head.state_root.to_hex(), f["state_root"]);
        assert_eq!(
            head.snapshot_height.value().to_string(),
            f["snapshot_height"]
        );
        assert_eq!(head.snapshot_root.to_hex(), f["snapshot_root"]);
    }
    assert!(decoded_heads.iter().all(|head| head == &decoded_heads[0]));

    let delta = SyncDelta::from_canonical_bytes(&hx(&f["delta_bytes"])).expect("delta decodes");
    assert_eq!(to_hex(&delta.to_bytes()), f["delta_bytes"]);
    assert_eq!(delta.from_height.to_string(), f["snapshot_height"]);
    assert_eq!(delta.to_height.to_string(), f["target_height"]);
    assert_eq!(delta.changes.len().to_string(), f["state_keys"]);

    let pkg = package(&f);
    assert_eq!(to_hex(&pkg.to_bytes()), f["package_bytes"]);
    assert_eq!(sha256_bytes(&pkg.data).to_hex(), f["data_sha256"]);
    assert_eq!(sha256_bytes(&pkg.to_bytes()).to_hex(), f["package_sha256"]);
    assert_eq!(pkg.to_bytes().len().to_string(), f["package_len"]);
    assert_eq!(pkg.heads.len(), SYNC_QUORUM);
    assert_eq!(sha256_bytes(&pkg.data).to_hex(), f["data_sha256"]);
}

#[test]
fn new_node_reaches_the_same_root_as_three_old_nodes_without_history() {
    // S5 DoD gate: a fresh node downloads ONLY the package, not all history.
    let f = parse(GOLDEN);
    let pkg = package(&f);
    let result = pkg
        .sync(
            chain(&f),
            Height::new(f_u64(&f, "target_height")),
            Height::new(f_u64(&f, "snapshot_height")),
            4,
            f_u64(&f, "history_bytes"),
        )
        .expect("sync succeeds");

    let expected = state_at(f_u64(&f, "target_height"), f_usize(&f, "state_keys"));
    assert_eq!(result.state_root.to_hex(), f["state_root"]);
    assert_eq!(
        result.entries, expected,
        "new node must reach identical state"
    );
    assert_eq!(result.height.value().to_string(), f["target_height"]);
    assert_eq!(result.downloaded_bytes.to_string(), f["package_len"]);
    assert!(result.history_bytes_saved > 0);
    assert!(
        result.downloaded_bytes < f_u64(&f, "history_bytes"),
        "sync must be smaller than full history"
    );
    assert_eq!(
        state_root(&result.entries).unwrap().to_hex(),
        f["state_root"]
    );
}

#[test]
fn substituted_diff_chunk_is_rejected() {
    // S5 DoD NC: a substituted chunk (data or delta) must be rejected.
    let f = parse(GOLDEN);
    let bad_data = SyncPackage::from_canonical_bytes(&hx(&f["bad_sub_package_bytes"])).unwrap();
    assert_eq!(
        bad_data
            .sync(
                chain(&f),
                Height::new(f_u64(&f, "target_height")),
                Height::new(f_u64(&f, "snapshot_height")),
                4,
                f_u64(&f, "history_bytes"),
            )
            .unwrap_err(),
        SyncError::Snapshot(aura_state::SnapshotError::ChunkHashMismatch(3))
    );
    let bad_delta =
        SyncPackage::from_canonical_bytes(&hx(&f["bad_sub_delta_package_bytes"])).unwrap();
    assert_eq!(
        bad_delta
            .sync(
                chain(&f),
                Height::new(f_u64(&f, "target_height")),
                Height::new(f_u64(&f, "snapshot_height")),
                4,
                f_u64(&f, "history_bytes"),
            )
            .unwrap_err(),
        SyncError::RootMismatch
    );
}

#[test]
fn quorum_mismatch_is_rejected() {
    let f = parse(GOLDEN);
    let pkg = SyncPackage::from_canonical_bytes(&hx(&f["bad_quorum_package_bytes"])).unwrap();
    assert_eq!(
        pkg.sync(
            chain(&f),
            Height::new(f_u64(&f, "target_height")),
            Height::new(f_u64(&f, "snapshot_height")),
            4,
            f_u64(&f, "history_bytes"),
        )
        .unwrap_err(),
        SyncError::QuorumMismatch
    );
}

#[test]
fn wrong_chain_and_height_are_rejected() {
    let f = parse(GOLDEN);
    let pkg = package(&f);
    assert_eq!(
        pkg.sync(
            ChainId::new("auralite-1").unwrap(),
            Height::new(f_u64(&f, "target_height")),
            Height::new(f_u64(&f, "snapshot_height")),
            2,
            f_u64(&f, "history_bytes"),
        )
        .unwrap_err(),
        SyncError::WrongChainId
    );
    assert_eq!(
        pkg.sync(
            chain(&f),
            Height::new(101),
            Height::new(f_u64(&f, "snapshot_height")),
            2,
            f_u64(&f, "history_bytes"),
        )
        .unwrap_err(),
        SyncError::HeightMismatch
    );
}

#[test]
fn every_bad_vector_is_rejected() {
    let f = parse(GOLDEN);
    let mismatches: Vec<(&str, SyncError)> = vec![
        ("bad_package_checksum", SyncError::ChecksumMismatch),
        ("bad_head_checksum", SyncError::ChecksumMismatch),
        ("bad_delta_checksum", SyncError::ChecksumMismatch),
        ("bad_head_magic", SyncError::BadMagic),
        ("bad_delta_magic", SyncError::BadMagic),
    ];
    for (key, error) in &mismatches {
        let bytes = hx(&f[*key]);
        match *key {
            "bad_package_checksum" | "bad_trailing_package" | "bad_truncated_package" => {
                assert_eq!(
                    SyncPackage::from_canonical_bytes(&bytes),
                    Err(error.clone()),
                    "{key}"
                );
            }
            "bad_head_checksum" | "bad_head_magic" => {
                assert_eq!(
                    SyncHead::from_canonical_bytes(&bytes),
                    Err(error.clone()),
                    "{key}"
                );
            }
            _ => {
                assert_eq!(
                    SyncDelta::from_canonical_bytes(&bytes),
                    Err(error.clone()),
                    "{key}"
                );
            }
        }
    }
    for key in ["bad_trailing_package", "bad_truncated_package"] {
        assert!(
            SyncPackage::from_canonical_bytes(&hx(&f[key])).is_err(),
            "{key}"
        );
    }
}

#[test]
fn sync_completes_within_the_measured_budget() {
    let f = parse(GOLDEN);
    let pkg = package(&f);
    let start = std::time::Instant::now();
    let result = pkg
        .sync(
            chain(&f),
            Height::new(f_u64(&f, "target_height")),
            Height::new(f_u64(&f, "snapshot_height")),
            4,
            f_u64(&f, "history_bytes"),
        )
        .unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(f_u64(&f, "max_sync_seconds")),
        "sync took too long"
    );
    assert_eq!(result.state_root.to_hex(), f["state_root"]);
    assert!(!result.entries.is_empty());
    assert!(Hash::ZERO != result.state_root);
}
