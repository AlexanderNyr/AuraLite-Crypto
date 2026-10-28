//! S6 — pruning, archive mode and compaction (DoD gate).
//!
//! The DoD line is: *100 000 blocks: disk size bounded in prune mode; archive
//! serves history*. Both halves are measured here against
//! `tests/fixtures/s6_golden.txt`, an INDEPENDENT Python model
//! (`tools/gen_s6_vectors.py`) that rebuilds the S1 tree from scratch and
//! computes the canonical byte accounting without any Rust code.
//!
//! Nothing in this file is a simulation of a network: one process, one store,
//! real hashing. `validators / blocks / tx` stay 0 / 0 / 0.

use std::collections::BTreeMap;

use aura_state::{MerkleTree, StateKey, vector_address, vector_value, verify_proof};
use aura_storage::{
    BASE_MAGIC, DEFAULT_KEEP_VERSIONS, PruneError, RECORD_MAGIC, RetentionMode, RetentionPolicy,
    RetentionStore, S6_BASE_CONTEXT, S6_RECORD_CONTEXT, StateChange, VersionRecord, encode_base,
    key_path,
};
use aura_types::Hash;

const BLOCKS: u64 = 100_000;
const SCHEDULE_BLOCKS: u64 = 1_000;
const WRITES_PER_BLOCK: u64 = 4;
const LIVE_KEYS: u64 = 1_024;

const FIXTURE: &str = include_str!("fixtures/s6_golden.txt");

fn fixture() -> &'static BTreeMap<String, String> {
    static PARSED: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        FIXTURE
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    })
}

/// Versions the independent Python model publishes roots for.
const CHECKPOINTS: [u64; 12] = [1, 100, 200, 300, 400, 500, 600, 700, 800, 900, 999, 1000];

fn expected(key: &str) -> u64 {
    fixture()
        .get(key)
        .unwrap_or_else(|| panic!("fixture has no {key}"))
        .parse()
        .unwrap_or_else(|_| panic!("fixture {key} is not a number"))
}

fn expected_root(version: u64) -> Hash {
    let key = format!("checkpoint_root_{version}");
    let text = fixture()
        .get(&key)
        .unwrap_or_else(|| panic!("fixture has no {key}"))
        .clone();
    Hash::from_hex(&text).expect("fixture root is hex")
}

fn schedule(block: u64) -> Vec<u64> {
    (0..WRITES_PER_BLOCK)
        .map(|slot| (block * 31 + slot * 17) % LIVE_KEYS)
        .collect()
}

fn changes(block: u64) -> Vec<StateChange> {
    schedule(block)
        .into_iter()
        .enumerate()
        .map(|(slot, index)| StateChange {
            key: StateKey::account(&vector_address(index))
                .as_bytes()
                .to_vec(),
            value: Some(vector_value(block * WRITES_PER_BLOCK + slot as u64)),
            nonce: block,
        })
        .collect()
}

fn path_of(index: u64) -> Hash {
    key_path(StateKey::account(&vector_address(index)).as_bytes())
}

/// Everything the two 100 000-block runs share.
struct Run {
    store: RetentionStore,
    head_root: Hash,
    elapsed_ms: u128,
    checkpoints: BTreeMap<u64, Hash>,
}

fn run_blocks(policy: RetentionPolicy, blocks: u64) -> Run {
    let mut store = RetentionStore::new(policy);
    let mut checkpoints = BTreeMap::new();
    let started = std::time::Instant::now();
    for block in 1..=blocks {
        store.commit(changes(block)).expect("commit must succeed");
        if CHECKPOINTS.contains(&block) {
            checkpoints.insert(block, store.root());
        }
    }
    let elapsed_ms = started.elapsed().as_millis();
    let head_root = store.root();
    Run {
        store,
        head_root,
        elapsed_ms,
        checkpoints,
    }
}

/// Rebuild the head state with S1 (`MerkleTree`), a different algorithm from
/// the S6 incremental tree, and compare roots.
fn s1_rebuild_root(store: &RetentionStore) -> Hash {
    let entries = store.head_entries();
    MerkleTree::from_entries(store.head(), &entries).state_root()
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[test]
fn canonical_bytes_match_the_independent_model() {
    // Byte-for-byte agreement with the Python model on the S6 wire formats:
    // a version record (payload + checksum, parent and state roots included)
    // and a materialised base snapshot.
    let mut store = RetentionStore::new(RetentionPolicy::archive());
    for block in 1..=1_000 {
        store.commit(changes(block)).unwrap();
    }
    let record_1 = VersionRecord::new(1, Hash::ZERO, store.root_at(1).unwrap(), changes(1));
    assert_eq!(
        to_hex(&record_1.encode()),
        fixture()["record_hex_1"],
        "version-1 record bytes disagree with the independent model"
    );
    assert_eq!(VersionRecord::decode(&record_1.encode()).unwrap(), record_1);

    let record_1000 = VersionRecord::new(
        1_000,
        store.root_at(999).unwrap(),
        store.root_at(1_000).unwrap(),
        changes(1_000),
    );
    assert_eq!(
        to_hex(&record_1000.encode()),
        fixture()["record_hex_1000"],
        "version-1000 record bytes disagree with the independent model"
    );

    // Base snapshot: keep = 2 over 5 blocks -> base version 3.
    let mut small = RetentionStore::new(RetentionPolicy::prune(2).unwrap());
    for block in 1..=5 {
        small.commit(changes(block)).unwrap();
    }
    assert_eq!(small.oldest_served_version(), 3);
    let base_bytes = encode_base(3, &small.base_entries());
    assert_eq!(
        to_hex(&base_bytes),
        fixture()["base_hex_prune2_head5"],
        "base snapshot bytes disagree with the independent model"
    );
    assert_eq!(base_bytes.len() as u64, small.base_bytes());
}

#[test]
fn archive_100k_blocks_serves_history() {
    let archive = run_blocks(RetentionPolicy::archive(), BLOCKS);
    let store = &archive.store;

    assert_eq!(store.head(), BLOCKS);
    assert_eq!(
        store.retained_versions(),
        BLOCKS,
        "archive keeps every delta"
    );
    assert_eq!(
        store.disk_bytes(),
        expected("archive_total_bytes"),
        "archive byte accounting must match the independent model"
    );
    assert_eq!(store.base_bytes(), expected("archive_base_bytes"));
    assert_eq!(store.pruned_versions(), 0, "archive must never prune");

    // The head root recomputed by the S1 rebuild must equal the incremental
    // root at 100 000 blocks — two independent algorithms over the same state.
    assert_eq!(
        s1_rebuild_root(store),
        archive.head_root,
        "incremental root diverged from the S1 rebuild at 100 000 blocks"
    );

    // Historical roots served from retained deltas, against Python.
    for version in CHECKPOINTS {
        assert_eq!(
            store.root_at(version).expect("archive serves history"),
            expected_root(version),
            "root at version {version} disagrees with the Python rebuild"
        );
        assert_eq!(
            archive.checkpoints.get(&version).copied(),
            Some(expected_root(version)),
            "root observed at commit {version} disagrees with the Python rebuild"
        );
    }
    // Recompute whole historical states from genesis and re-derive their roots.
    for version in [1u64, 250, 500, 750, 1_000] {
        assert_eq!(
            store
                .verify_root_at(version)
                .expect("historical root verifies"),
            store.root_at(version).unwrap(),
            "state at version {version} does not recompute to its stored root"
        );
    }
    assert_eq!(SCHEDULE_BLOCKS, 1_000);

    // Historical inclusion proofs verify against the historical root.
    let mut proven = 0u64;
    for index in 0..LIVE_KEYS {
        if index % 128 != 0 {
            continue;
        }
        let path = path_of(index);
        let version = 1_000u64;
        let proof = store.proof_at(version, &path).expect("historical proof");
        verify_proof(&store.root_at(version).unwrap(), &proof)
            .expect("historical proof must verify");
        proven += 1;
    }
    assert_eq!(
        proven,
        LIVE_KEYS / 128,
        "proved a sample of historical keys"
    );

    // Compaction is forbidden: it would destroy what an archive exists for.
    let head_root = archive.head_root;
    let elapsed_ms = archive.elapsed_ms;
    let mut archive = archive.store;
    assert_eq!(
        archive.compact(),
        Err(PruneError::ArchiveCompactionForbidden)
    );
    println!(
        "archive: {BLOCKS} blocks, {} bytes retained, head {} in {elapsed_ms} ms",
        archive.disk_bytes(),
        head_root.to_hex()
    );
}

#[test]
fn prune_100k_blocks_keeps_disk_bounded() {
    let keep = expected("keep_versions");
    assert_eq!(keep, DEFAULT_KEEP_VERSIONS);
    let policy = RetentionPolicy::prune(keep).expect("valid policy");
    let pruned = run_blocks(policy, BLOCKS);
    let store = &pruned.store;

    assert_eq!(store.head(), BLOCKS);
    assert_eq!(store.retained_versions(), expected("prune_retained"));
    assert_eq!(
        store.retained_versions(),
        keep,
        "exactly keep_versions deltas"
    );
    assert_eq!(
        store.oldest_served_version(),
        expected("prune_final_base_version")
    );
    assert_eq!(store.pruned_versions(), expected("prune_pruned_versions"));
    assert_eq!(store.pruned_bytes(), expected("prune_pruned_bytes"));
    assert_eq!(store.base_bytes(), expected("prune_final_base_bytes"));
    assert_eq!(store.records_bytes(), expected("prune_final_records_bytes"));
    assert_eq!(store.disk_bytes(), expected("prune_final_disk_bytes"));
    assert_eq!(store.peak_disk_bytes(), expected("prune_peak_disk_bytes"));

    // The bound the DoD asks for: base + keep * largest record. Measured peak
    // must sit under it, not merely "grow slower than archive".
    let max_record = store.max_record_bytes();
    assert!(max_record > 0);
    let bound = store.base_bytes() + keep * max_record;
    assert!(
        store.peak_disk_bytes() <= bound,
        "peak {} bytes exceeds the analytic bound {} bytes",
        store.peak_disk_bytes(),
        bound
    );

    // Disk stays bounded while the archive run of the same schedule grows to
    // ~36 MB: the peak is >300x smaller (fixture: 36500045 vs 113326).
    let ratio = expected("archive_total_bytes") as f64 / store.peak_disk_bytes() as f64;
    assert!(
        ratio > 300.0,
        "prune/archive ratio {ratio:.1} is below 300x"
    );

    // The head root at 100 000 blocks is an S1 rebuild away from being proven.
    assert_eq!(
        s1_rebuild_root(store),
        pruned.head_root,
        "pruned head root diverged from the S1 rebuild"
    );
    // ... and the roots observed while the run was still young match Python:
    // pruning does not change history, it only stops serving it.
    for version in CHECKPOINTS {
        assert_eq!(
            pruned.checkpoints.get(&version).copied(),
            Some(expected_root(version)),
            "root observed at commit {version} disagrees with the Python rebuild"
        );
    }

    println!(
        "prune: {} blocks in {} ms, peak {} bytes (bound {}), {} pruned, ratio {:.1}x",
        BLOCKS,
        pruned.elapsed_ms,
        store.peak_disk_bytes(),
        bound,
        store.pruned_versions(),
        ratio
    );
}

#[test]
fn pruned_versions_are_rejected_not_faked() {
    let mut store = RetentionStore::new(RetentionPolicy::prune(8).unwrap());
    for block in 1..=40 {
        store.commit(changes(block)).unwrap();
    }
    // keep 8 at head 40 -> base is version 32, deltas 33..=40.
    assert_eq!(store.oldest_served_version(), 32);
    assert_eq!(store.retained_versions(), 8);

    match store.root_at(4) {
        Err(PruneError::Pruned { version, oldest }) => {
            assert_eq!(version, 4);
            assert_eq!(oldest, 32);
        }
        other => panic!("expected Pruned(4), got {other:?}"),
    }
    let path = path_of(31);
    assert!(matches!(
        store.proof_at(4, &path),
        Err(PruneError::Pruned { .. })
    ));
    assert!(matches!(store.state_at(4), Err(PruneError::Pruned { .. })));
    // Pruned, but still consistent at/after the base.
    assert!(store.verify_root_at(32).is_ok());
    assert!(store.verify_root_at(40).is_ok());
    assert_eq!(store.root_at(41), Err(PruneError::VersionNotFound(41)));
    // Archive mode serves the very same versions, including the pruned ones.
    let mut archive = RetentionStore::new(RetentionPolicy::archive());
    for block in 1..=40 {
        archive.commit(changes(block)).unwrap();
    }
    for version in [1u64, 4, 32, 40] {
        let served = archive.root_at(version).expect("archive serves genesis");
        if version >= store.oldest_served_version() {
            assert_eq!(
                served,
                store.root_at(version).unwrap(),
                "prune and archive must agree on version {version}"
            );
        }
    }
    let genesis_path = path_of(schedule(1)[0]);
    assert!(
        archive.proof_at(1, &genesis_path).is_ok(),
        "archive serves proofs from genesis"
    );
}

#[test]
fn compaction_folds_history_and_preserves_the_root() {
    let mut store = RetentionStore::new(RetentionPolicy::prune(64).unwrap());
    for block in 1..=5_000 {
        store.commit(changes(block)).unwrap();
    }
    let before_disk = store.disk_bytes();
    let before_root = store.root();
    let retained = store.retained_versions();
    assert_eq!(retained, 64);

    let report = store.compact().expect("prune mode may compact");
    assert_eq!(report.folded_records, retained);
    assert_eq!(report.before_bytes, before_disk);
    assert_eq!(report.after_bytes, store.disk_bytes());
    assert!(
        report.after_bytes < report.before_bytes,
        "compaction must shrink"
    );
    assert_eq!(
        report.root, before_root,
        "compaction must not move the root"
    );
    assert_eq!(store.root(), before_root);
    assert_eq!(store.retained_versions(), 0);
    assert_eq!(store.oldest_served_version(), store.head());
    assert_eq!(store.disk_bytes(), store.base_bytes());
    assert_eq!(store.base_bytes(), report.after_bytes);
    assert_eq!(store.compaction_count(), 1);
    // The compacted base still reproduces the head root.
    assert_eq!(s1_rebuild_root(&store), before_root);
    // ... and commits keep working after a compaction.
    store.commit(changes(5_001)).unwrap();
    assert_eq!(store.head(), 5_001);
    assert_eq!(s1_rebuild_root(&store), store.root());
    // Shrinkage is real, not a rounding artefact.
    assert!(
        report.after_bytes * 100 < report.before_bytes * 90,
        "compaction removed only {} of {} bytes",
        report.before_bytes - report.after_bytes,
        report.before_bytes
    );
}

#[test]
fn retention_boundary_is_exact() {
    assert_eq!(
        RetentionPolicy::prune(0),
        Err(PruneError::InvalidKeepVersions(0))
    );
    assert!(RetentionPolicy::prune(1_000_001).is_err());
    assert_eq!(RetentionPolicy::prune(1).unwrap().keep_versions(), 1);
    assert_eq!(RetentionPolicy::archive().mode(), RetentionMode::Archive);

    let mut store = RetentionStore::new(RetentionPolicy::prune(1).unwrap());
    for block in 1..=10 {
        let receipt = store.commit(changes(block)).unwrap();
        // With keep = 1 the base is always head - 1 and one delta is retained.
        assert_eq!(receipt.retained_versions, 1);
        assert_eq!(store.oldest_served_version(), block - 1);
        assert_eq!(store.pruned_versions(), block - 1);
        assert_eq!(
            receipt.disk_bytes,
            store.base_bytes() + store.records_bytes()
        );
        assert_eq!(receipt.pruned_by_this_commit, if block > 1 { 1 } else { 0 });
    }
    // Boundary: the base version is served, base - 1 is not.
    assert!(store.root_at(9).is_ok());
    assert!(matches!(store.root_at(8), Err(PruneError::Pruned { .. })));
}

#[test]
fn version_records_round_trip_and_reject_tampering() {
    let record = VersionRecord::new(7, Hash::ZERO, Hash::from_bytes([9u8; 32]), changes(7));
    let bytes = record.encode();
    assert_eq!(bytes.len() as u64, record.encoded_len());
    assert_eq!(&bytes[..4], RECORD_MAGIC);
    assert_eq!(VersionRecord::decode(&bytes).unwrap(), record);
    assert_eq!(
        record.encoded_len(),
        expected("record_len_first"),
        "record size must match the independent model (same schedule shape)"
    );

    let mut bad_checksum = bytes.clone();
    let last = bad_checksum.len() - 1;
    bad_checksum[last] ^= 1;
    assert_eq!(
        VersionRecord::decode(&bad_checksum),
        Err(PruneError::ChecksumMismatch)
    );

    let mut bad_magic = bytes.clone();
    bad_magic[..4].copy_from_slice(b"XXXX");
    assert_eq!(VersionRecord::decode(&bad_magic), Err(PruneError::BadMagic));

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        VersionRecord::decode(&trailing),
        Err(PruneError::ChecksumMismatch) | Err(PruneError::NonCanonical)
    ));

    assert_eq!(
        VersionRecord::decode(&bytes[..bytes.len() - 8]),
        Err(PruneError::ChecksumMismatch)
    );
    assert_eq!(VersionRecord::decode(&[]), Err(PruneError::CorruptRecord));

    // A delete change (tag 0) also round-trips canonically.
    let mut deletes = changes(9);
    deletes[0].value = None;
    let delete_record = VersionRecord::new(9, Hash::ZERO, Hash::ZERO, deletes);
    assert_eq!(
        VersionRecord::decode(&delete_record.encode()).unwrap(),
        delete_record
    );
    assert!(delete_record.encoded_len() < record.encoded_len());
}

#[test]
fn base_accounting_matches_the_canonical_encoding() {
    let mut store = RetentionStore::new(RetentionPolicy::prune(16).unwrap());
    for block in 1..=200 {
        store.commit(changes(block)).unwrap();
    }
    let entries = store.base_entries();
    let encoded = encode_base(store.oldest_served_version(), &entries);
    assert_eq!(&encoded[..4], BASE_MAGIC);
    assert_eq!(
        encoded.len() as u64,
        store.base_bytes(),
        "tracked base accounting must equal the encoded size"
    );
    // The archive-mode base (version 0, no entries) is the smallest snapshot.
    assert_eq!(
        encode_base(0, &[]).len() as u64,
        expected("archive_base_bytes")
    );
    // Record and base checksums are domain separated.
    assert_ne!(S6_BASE_CONTEXT, S6_RECORD_CONTEXT);
    assert_ne!(RECORD_MAGIC, BASE_MAGIC);
}

#[test]
fn pruning_never_changes_the_state() {
    // The same schedule under prune and archive must produce identical roots
    // and identical live state at the head, at every checkpoint.
    let mut pruned = RetentionStore::new(RetentionPolicy::prune(4).unwrap());
    let mut archive = RetentionStore::new(RetentionPolicy::archive());
    for block in 1..=500 {
        pruned.commit(changes(block)).unwrap();
        archive.commit(changes(block)).unwrap();
        assert_eq!(pruned.root(), archive.root(), "roots diverged at {block}");
        if block % 100 == 0 {
            let served = pruned.state_at(pruned.oldest_served_version()).unwrap();
            let archived = archive.state_at(pruned.oldest_served_version()).unwrap();
            assert_eq!(served, archived, "state diverged at {block}");
            assert_eq!(pruned.head_entries(), archive.head_entries());
        }
    }
    assert_eq!(pruned.root(), archive.root());
    assert!(
        archive.disk_bytes() > 2 * pruned.disk_bytes(),
        "prune {} vs archive {} bytes",
        pruned.disk_bytes(),
        archive.disk_bytes()
    );
    assert!(pruned.tree_hashes() > 0 && archive.tree_hashes() > 0);
}
