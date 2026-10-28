//! S4 gate: snapshots against an independent Python reference.
//!
//! `fixtures/s4_golden.txt` comes from `tools/gen_s4_vectors.py` — hashlib
//! SHA-256, pure-Python BLAKE3, the C7 state-key digest and the S1 sparse tree.
//! Report: `docs/evidence/s4/report.md`.

use std::collections::BTreeMap;

use aura_state::{
    MAX_CHUNK_BYTES, MAX_CHUNKS, MAX_KEY_BYTES, MAX_VALUE_BYTES, SNAPSHOT_CHUNK_CONTEXT,
    SNAPSHOT_MAGIC, SNAPSHOT_MANIFEST_CONTEXT, SNAPSHOT_ROOT_CONTEXT, SNAPSHOT_VERSION,
    SnapshotError, SnapshotManifest, chunk_hash, chunk_root, decode_chunk, encode_chunk,
    export_snapshot, import_snapshot, manifest_bytes, parallel_chunk_hashes, sha256_bytes,
    state_root,
};
use aura_types::{ChainId, Hash, Height};

const GOLDEN: &str = include_str!("fixtures/s4_golden.txt");

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
fn vector_entries(count: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
    (0..count)
        .map(|index| {
            let mut key = vec![0x01u8];
            key.extend_from_slice(&[0u8; 12]);
            key.extend_from_slice(&(index as u64).to_be_bytes());
            (key, (index as u64).to_le_bytes().to_vec())
        })
        .collect()
}

fn chain(f: &BTreeMap<String, String>) -> ChainId {
    ChainId::new(&f["chain_id"]).expect("valid chain-id")
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(SNAPSHOT_MAGIC), f["snapshot_magic"]);
    assert_eq!(SNAPSHOT_VERSION.to_string(), f["snapshot_version"]);
    assert_eq!(to_hex(SNAPSHOT_MANIFEST_CONTEXT), f["manifest_context"]);
    assert_eq!(to_hex(SNAPSHOT_CHUNK_CONTEXT), f["chunk_context"]);
    assert_eq!(to_hex(SNAPSHOT_ROOT_CONTEXT), f["root_context"]);
    assert_eq!(MAX_CHUNKS, 1_000_000);
    assert_eq!(MAX_CHUNK_BYTES, 64 * 1024 * 1024);
    assert_eq!(MAX_KEY_BYTES, 1024);
    assert_eq!(MAX_VALUE_BYTES, 1024 * 1024);
}

#[test]
fn canonical_snapshot_matches_independent_python() {
    let f = parse(GOLDEN);
    let entries = vector_entries(f_usize(&f, "entry_count"));
    let root = state_root(&entries).expect("S1 state root");
    let snapshot = export_snapshot(
        &entries,
        chain(&f),
        Height::new(f_u64(&f, "height")),
        root,
        f_u64(&f, "chunk_size") as u32,
        4,
    )
    .expect("export succeeds");

    assert_eq!(root.to_hex(), f["state_root"]);
    assert_eq!(snapshot.manifest.state_root.to_hex(), f["state_root"]);
    assert_eq!(snapshot.manifest.chunk_root.to_hex(), f["chunk_root"]);
    assert_eq!(to_hex(&manifest_bytes(&snapshot)), f["manifest_bytes"]);
    assert_eq!(to_hex(&snapshot.data), f["data_bytes"]);
    assert_eq!(sha256_bytes(&snapshot.data).to_hex(), f["data_sha256"]);
    assert_eq!(snapshot.manifest.chunks.len().to_string(), f["chunk_count"]);

    let decoded = SnapshotManifest::from_canonical_bytes(&hx(&f["manifest_bytes"]))
        .expect("manifest decodes");
    assert_eq!(decoded, snapshot.manifest);
    for (index, descriptor) in snapshot.manifest.chunks.iter().enumerate() {
        assert_eq!(
            descriptor.offset.to_string(),
            f[&format!("chunk_{index}_offset")]
        );
        assert_eq!(
            descriptor.length.to_string(),
            f[&format!("chunk_{index}_length")]
        );
        assert_eq!(
            descriptor.sha256.to_hex(),
            f[&format!("chunk_{index}_sha256")]
        );
    }

    let imported =
        import_snapshot(&hx(&f["manifest_bytes"]), &snapshot.data, 4).expect("import succeeds");
    assert_eq!(imported, entries, "import must be byte-identical");
}

#[test]
fn chunk_encoding_is_canonical_and_rejects_bad_vectors() {
    let f = parse(GOLDEN);
    let entries = vector_entries(f_usize(&f, "entry_count"));
    let payload = encode_chunk(&entries);
    assert_eq!(decode_chunk(&payload).unwrap(), entries);
    assert_eq!(
        decode_chunk(&hx(&f["bad_unsorted_data"])),
        Err(SnapshotError::UnsortedKeys)
    );
    assert_eq!(
        decode_chunk(&hx(&f["bad_duplicate_data"])),
        Err(SnapshotError::UnsortedKeys)
    );
    assert!(decode_chunk(&hx(&f["bad_empty_data"])).is_err());
}

#[test]
fn chunk_hashes_and_root_match_independent_python() {
    let f = parse(GOLDEN);
    let entries = vector_entries(f_usize(&f, "entry_count"));
    let manifest = SnapshotManifest::from_canonical_bytes(&hx(&f["manifest_bytes"])).unwrap();
    let payloads: Vec<Vec<u8>> = entries
        .chunks(f_u64(&f, "chunk_size") as usize)
        .map(encode_chunk)
        .collect();
    let digests = parallel_chunk_hashes(&payloads, 4).unwrap();
    assert_eq!(chunk_root(&digests).to_hex(), f["chunk_root"]);
    for (index, _descriptor) in manifest.chunks.iter().enumerate() {
        assert_eq!(digests[index], chunk_hash(payloads[index].as_ref()));
        assert_eq!(digests[index].to_hex(), f[&format!("chunk_{index}_sha256")]);
    }
}

#[test]
fn parallel_hashing_is_deterministic_across_worker_counts() {
    let f = parse(GOLDEN);
    let entries = vector_entries(f_usize(&f, "parallel_entries"));
    let payloads: Vec<Vec<u8>> = entries
        .chunks(f_u64(&f, "parallel_chunk_size") as usize)
        .map(encode_chunk)
        .collect();
    let data: Vec<u8> = payloads.iter().flat_map(|p| p.iter().copied()).collect();
    assert_eq!(sha256_bytes(&data).to_hex(), f["parallel_data_sha256"]);
    for workers in [1usize, 2, 4, 8] {
        let digests = parallel_chunk_hashes(&payloads, workers).unwrap();
        assert_eq!(digests.len(), payloads.len(), "workers={workers}");
        for (index, digest) in digests.iter().enumerate() {
            assert_eq!(
                digest.to_hex(),
                f[&format!("parallel_chunk_{index}_sha256")],
                "workers={workers}, chunk={index}"
            );
            assert_eq!(*digest, chunk_hash(payloads[index].as_ref()));
        }
        assert_eq!(chunk_root(&digests).to_hex(), f["parallel_chunk_root"]);
    }
}

#[test]
fn million_key_snapshot_is_imported_byte_identically() {
    // S4 DoD gate: a snapshot of 1,000,000 keys reimports byte-identically.
    let f = parse(GOLDEN);
    let count = f_usize(&f, "million_entries");
    let entries = vector_entries(count);
    // The Python reference computes the S1 state root only for the small
    // snapshot; the 1M gate focuses on chunking/hashing/import. Use ZERO root.
    let snapshot = export_snapshot(
        &entries,
        chain(&f),
        Height::new(f_u64(&f, "height")),
        Hash::ZERO,
        f_u64(&f, "million_chunk_size") as u32,
        4,
    )
    .unwrap();

    assert_eq!(snapshot.data.len().to_string(), f["million_data_len"]);
    assert_eq!(
        sha256_bytes(&snapshot.data).to_hex(),
        f["million_data_sha256"]
    );
    assert_eq!(
        snapshot.manifest.chunks.len().to_string(),
        f["million_chunk_count"]
    );
    assert_eq!(
        snapshot.manifest.chunk_root.to_hex(),
        f["million_chunk_root"]
    );
    assert_eq!(snapshot.manifest.total_keys, count as u64);

    let manifest = manifest_bytes(&snapshot);
    let imported = import_snapshot(&manifest, &snapshot.data, 4).expect("import of 1M keys");
    assert_eq!(imported.len(), count);
    assert_eq!(imported, entries, "1M-key import must be byte-identical");
}

#[test]
fn substituted_chunk_is_rejected() {
    // S4 DoD NC: a substituted chunk must be rejected.
    let f = parse(GOLDEN);
    let manifest = hx(&f["bad_substituted_manifest"]);
    let data = hx(&f["bad_substituted_data"]);
    assert_eq!(
        import_snapshot(&manifest, &data, 4).unwrap_err(),
        SnapshotError::ChunkHashMismatch(2)
    );
}

#[test]
fn every_bad_vector_is_rejected() {
    let f = parse(GOLDEN);
    let expected: Vec<(&str, SnapshotError)> = vec![
        ("bad_manifest_checksum", SnapshotError::BadManifestChecksum),
        ("bad_manifest_magic", SnapshotError::BadMagic),
        ("bad_chunk_root_manifest", SnapshotError::ChunkRootMismatch),
    ];
    for (key, error) in &expected {
        assert_eq!(
            SnapshotManifest::from_canonical_bytes(&hx(&f[*key])),
            Err(error.clone()),
            "{key}"
        );
    }
    let key = "bad_chunk_count_manifest";
    assert!(
        SnapshotManifest::from_canonical_bytes(&hx(&f[key])).is_err(),
        "{key}"
    );
    let good_manifest = hx(&f["manifest_bytes"]);
    for key in ["bad_truncated_data", "bad_trailing_data"] {
        assert!(
            matches!(
                import_snapshot(&good_manifest, &hx(&f[key]), 4).unwrap_err(),
                SnapshotError::DataLengthMismatch(_, _)
            ),
            "{key}"
        );
    }
}
