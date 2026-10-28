//! S6: version retention — prune mode, archive mode and compaction.
//!
//! A chain that keeps every version forever is not runnable; a chain that
//! keeps nothing cannot serve a light client. S6 adds the missing layer: a
//! [`RetentionStore`] that owns a [`HistoryTree`] (S6, `aura-state`) plus the
//! version records, and applies a [`RetentionPolicy`]:
//!
//! * **Archive** — every version record is retained from genesis. Any
//!   historical root and any historical inclusion proof can be served.
//! * **Prune** — the store keeps a materialised *base* snapshot plus the last
//!   `keep_versions` deltas. Versions older than the base are gone and asking
//!   for them returns [`PruneError::Pruned`] — a loud error, never a silent
//!   wrong answer.
//!
//! Bytes are accounted exactly: every version record and the base snapshot are
//! canonically encoded (S6 formats below) and `disk_bytes()` is the sum of the
//! encoded sizes. The S6 gate checks the accounting against an independent
//! Python model over 100 000 blocks, so "disk size bounded in prune mode" is a
//! measurement, not a claim.
//!
//! ## Wire formats
//!
//! ```text
//! record payload = "ALP6" || u64 version || 32 parent_root || 32 state_root ||
//!                  varint change_count ||
//!                  (varint key_len || key || tag(0=delete,1=put) ||
//!                   [varint value_len || value] || u64 nonce)*
//! record bytes   = payload || BLAKE3(S6_RECORD_CONTEXT || payload)
//!
//! base payload   = "ALB6" || u64 version || varint entry_count ||
//!                  (32 path || varint value_len || value)*
//! base bytes     = payload || BLAKE3(S6_BASE_CONTEXT || payload)
//! ```
//!
//! Root commitments are S1: `versioned_root(version, tree_root)`.
//!
//! Golden vectors: `tools/gen_s6_vectors.py` (independent Python).
//! Report: `docs/evidence/s6/report.md`.

use std::collections::{BTreeMap, VecDeque};

use aura_codec::{Decode, Encode, Reader, Writer, read_varint, write_varint};
use aura_crypto::blake3_hash;
use aura_state::{HistoryTree, MerkleProof, MerkleTree, TreeError};
use aura_types::Hash;

use crate::error::PruneError;
use crate::wal::{StateChange, key_path};

/// Magic prefix of a version record (S6).
pub const RECORD_MAGIC: &[u8] = b"ALP6";
/// Magic prefix of a base snapshot (S6).
pub const BASE_MAGIC: &[u8] = b"ALB6";
/// Domain tag for version-record checksums.
///
/// NC target: `tools/nc_s6.py` changes this constant and requires the S6
/// golden-vector test to go red.
pub const S6_RECORD_CONTEXT: &[u8] = b"AuraLite-Crypto/s6-record-v1\x00";
/// Domain tag for base-snapshot checksums.
///
/// NC target: `tools/nc_s6.py` changes this constant and requires the S6
/// golden-vector test to go red.
pub const S6_BASE_CONTEXT: &[u8] = b"AuraLite-Crypto/s6-base-v1\x00";
/// Default number of recent versions kept in prune mode.
pub const DEFAULT_KEEP_VERSIONS: u64 = 128;
/// Hard cap on `keep_versions` (guards the memory of an operator typo).
pub const MAX_KEEP_VERSIONS: u64 = 1_000_000;
/// Hard cap on the changes in one version record.
pub const MAX_RECORD_CHANGES: usize = 1_000_000;

/// How much history a store keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionMode {
    /// Keep only the base snapshot plus the last `keep_versions` deltas.
    Prune,
    /// Keep every version from genesis (an archive node).
    Archive,
}

/// The retention rule applied after every commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    mode: RetentionMode,
    keep_versions: u64,
}

impl RetentionPolicy {
    /// Prune mode keeping `keep_versions` recent deltas. `0` is rejected:
    /// a store that keeps no delta can serve neither proofs nor sync.
    pub fn prune(keep_versions: u64) -> Result<Self, PruneError> {
        if keep_versions == 0 || keep_versions > MAX_KEEP_VERSIONS {
            return Err(PruneError::InvalidKeepVersions(keep_versions));
        }
        Ok(RetentionPolicy {
            mode: RetentionMode::Prune,
            keep_versions,
        })
    }

    /// Archive mode: retain every version.
    pub fn archive() -> Self {
        RetentionPolicy {
            mode: RetentionMode::Archive,
            keep_versions: MAX_KEEP_VERSIONS,
        }
    }

    /// The mode.
    pub fn mode(&self) -> RetentionMode {
        self.mode
    }

    /// Number of recent deltas prune mode keeps (unused in archive mode).
    pub fn keep_versions(&self) -> u64 {
        self.keep_versions
    }
}

/// One retained version: the delta that produced it plus both roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRecord {
    /// Monotonic version (1-based).
    pub version: u64,
    /// State root of the previous version (zero at version 1).
    pub parent_root: Hash,
    /// State root after applying `changes`.
    pub state_root: Hash,
    /// Ordered state changes.
    pub changes: Vec<StateChange>,
    encoded_len: u64,
}

impl VersionRecord {
    /// Build a record and compute its canonical encoded size.
    pub fn new(
        version: u64,
        parent_root: Hash,
        state_root: Hash,
        changes: Vec<StateChange>,
    ) -> Self {
        let mut record = VersionRecord {
            version,
            parent_root,
            state_root,
            changes,
            encoded_len: 0,
        };
        record.encoded_len = record.encode().len() as u64;
        record
    }

    /// Number of bytes this record occupies on disk (payload + checksum).
    pub fn encoded_len(&self) -> u64 {
        self.encoded_len
    }

    /// Canonical encoded bytes.
    pub fn encode(&self) -> Vec<u8> {
        let payload = self.payload();
        let mut bytes = Vec::with_capacity(payload.len() + 32);
        bytes.extend_from_slice(&payload);
        bytes.extend_from_slice(record_checksum(&payload).as_bytes());
        bytes
    }

    fn payload(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.push_bytes(RECORD_MAGIC);
        self.version.encode(&mut writer);
        writer.push_bytes(self.parent_root.as_bytes());
        writer.push_bytes(self.state_root.as_bytes());
        write_varint(&mut writer, self.changes.len() as u64);
        for change in &self.changes {
            write_varint(&mut writer, change.key.len() as u64);
            writer.push_bytes(&change.key);
            match &change.value {
                Some(value) => {
                    writer.push_bytes(&[1]);
                    write_varint(&mut writer, value.len() as u64);
                    writer.push_bytes(value);
                }
                None => writer.push_bytes(&[0]),
            }
            writer.push_bytes(&change.nonce.to_le_bytes());
        }
        writer.into_bytes()
    }

    /// Strict canonical decode: checksum, magic, caps, no trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, PruneError> {
        if bytes.len() < RECORD_MAGIC.len() + 32 {
            return Err(PruneError::CorruptRecord);
        }
        let (payload, checksum) = bytes.split_at(bytes.len() - 32);
        if payload.len() < RECORD_MAGIC.len() || &payload[..RECORD_MAGIC.len()] != RECORD_MAGIC {
            return Err(PruneError::BadMagic);
        }
        if record_checksum(payload).as_bytes() != checksum {
            return Err(PruneError::ChecksumMismatch);
        }
        let mut reader = Reader::new(payload);
        let _ = reader
            .read_exact(RECORD_MAGIC.len())
            .map_err(|_| PruneError::CorruptRecord)?;
        let version = u64::decode(&mut reader).map_err(|_| PruneError::CorruptRecord)?;
        let parent_root = read_hash(&mut reader)?;
        let state_root = read_hash(&mut reader)?;
        let count = read_varint(&mut reader).map_err(|_| PruneError::CorruptRecord)? as usize;
        if count > MAX_RECORD_CHANGES {
            return Err(PruneError::CorruptRecord);
        }
        let mut changes = Vec::with_capacity(count);
        for _ in 0..count {
            let key_len = read_varint(&mut reader).map_err(|_| PruneError::CorruptRecord)? as usize;
            let key = reader
                .read_exact(key_len)
                .map_err(|_| PruneError::CorruptRecord)?
                .to_vec();
            let tag = reader
                .read_exact(1)
                .map_err(|_| PruneError::CorruptRecord)?[0];
            let value = match tag {
                0 => None,
                1 => {
                    let value_len =
                        read_varint(&mut reader).map_err(|_| PruneError::CorruptRecord)? as usize;
                    Some(
                        reader
                            .read_exact(value_len)
                            .map_err(|_| PruneError::CorruptRecord)?
                            .to_vec(),
                    )
                }
                _ => return Err(PruneError::CorruptRecord),
            };
            let nonce_bytes = reader
                .read_exact(8)
                .map_err(|_| PruneError::CorruptRecord)?;
            let mut nonce = [0u8; 8];
            nonce.copy_from_slice(nonce_bytes);
            changes.push(StateChange {
                key,
                value,
                nonce: u64::from_le_bytes(nonce),
            });
        }
        reader.finish().map_err(|_| PruneError::CorruptRecord)?;
        let mut record = VersionRecord {
            version,
            parent_root,
            state_root,
            changes,
            encoded_len: 0,
        };
        record.encoded_len = record.encode().len() as u64;
        if record.encoded_len != bytes.len() as u64 || record.encode() != bytes {
            return Err(PruneError::NonCanonical);
        }
        Ok(record)
    }
}

fn read_hash(reader: &mut Reader<'_>) -> Result<Hash, PruneError> {
    let bytes = reader
        .read_exact(32)
        .map_err(|_| PruneError::CorruptRecord)?;
    Hash::from_slice(bytes).map_err(|_| PruneError::CorruptRecord)
}

/// Checksum of a version-record payload.
pub fn record_checksum(payload: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(S6_RECORD_CONTEXT.len() + payload.len());
    input.extend_from_slice(S6_RECORD_CONTEXT);
    input.extend_from_slice(payload);
    blake3_hash(&input)
}

/// Checksum of a base-snapshot payload.
pub fn base_checksum(payload: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(S6_BASE_CONTEXT.len() + payload.len());
    input.extend_from_slice(S6_BASE_CONTEXT);
    input.extend_from_slice(payload);
    blake3_hash(&input)
}

/// Canonical base-snapshot bytes for `entries` (sorted by path).
pub fn encode_base(version: u64, entries: &[(Hash, Vec<u8>)]) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.push_bytes(BASE_MAGIC);
    version.encode(&mut writer);
    write_varint(&mut writer, entries.len() as u64);
    for (path, value) in entries {
        writer.push_bytes(path.as_bytes());
        write_varint(&mut writer, value.len() as u64);
        writer.push_bytes(value);
    }
    let payload = writer.into_bytes();
    let mut bytes = Vec::with_capacity(payload.len() + 32);
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(base_checksum(&payload).as_bytes());
    bytes
}

/// Canonical base-snapshot size without materialising it.
///
/// `entries_bytes` is the tracked sum of `32 + varint_len(value_len) +
/// value_len` over all base entries. `u64` fields are fixed-width little
/// endian (aura-codec `Encode`), so only the entry count needs a varint.
fn base_len(_version: u64, entry_count: u64, entries_bytes: u64) -> u64 {
    BASE_MAGIC.len() as u64 + 8 + varint_len(entry_count) + entries_bytes + 32
}

fn varint_len(value: u64) -> u64 {
    let mut remaining = value;
    let mut len = 1;
    while remaining >= 0x80 {
        remaining >>= 7;
        len += 1;
    }
    len
}

/// Bytes one base entry occupies: path + length prefix + value.
fn base_entry_len(value_len: usize) -> u64 {
    32 + varint_len(value_len as u64) + value_len as u64
}

/// Receipt of one committed version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionReceipt {
    /// New version.
    pub version: u64,
    /// New state root.
    pub root: Hash,
    /// Disk bytes after retention was applied.
    pub disk_bytes: u64,
    /// Deltas currently retained.
    pub retained_versions: u64,
    /// Deltas dropped by this commit's retention pass.
    pub pruned_by_this_commit: u64,
}

/// What a compaction did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionReport {
    /// Disk bytes before compaction.
    pub before_bytes: u64,
    /// Disk bytes after compaction.
    pub after_bytes: u64,
    /// Deltas folded into the base.
    pub folded_records: u64,
    /// Version the base now represents.
    pub base_version: u64,
    /// Head state root (must be unchanged by compaction).
    pub root: Hash,
}

/// A versioned state store with a retention policy.
///
/// Memory layout: the live state is a [`HistoryTree`] (S6) and the base
/// snapshot is an ordinary map, exactly like a real pruner's working set.
/// `disk_bytes()` counts *encoded* bytes, i.e. what a node would actually
/// write.
pub struct RetentionStore {
    policy: RetentionPolicy,
    tree: HistoryTree,
    head: u64,
    root: Hash,
    base: BTreeMap<Hash, Vec<u8>>,
    base_version: u64,
    base_root: Hash,
    base_entries_bytes: u64,
    records: VecDeque<VersionRecord>,
    records_bytes: u64,
    peak_disk_bytes: u64,
    max_record_bytes: u64,
    pruned_versions: u64,
    pruned_bytes: u64,
    compaction_count: u64,
}

impl RetentionStore {
    /// Create an empty store under `policy`.
    pub fn new(policy: RetentionPolicy) -> Self {
        RetentionStore {
            policy,
            tree: HistoryTree::new(),
            head: 0,
            root: Hash::ZERO,
            base: BTreeMap::new(),
            base_version: 0,
            base_root: Hash::ZERO,
            base_entries_bytes: 0,
            records: VecDeque::new(),
            records_bytes: 0,
            peak_disk_bytes: 0,
            max_record_bytes: 0,
            pruned_versions: 0,
            pruned_bytes: 0,
            compaction_count: 0,
        }
    }

    /// The retention policy.
    pub fn policy(&self) -> RetentionPolicy {
        self.policy
    }

    /// Head version.
    pub fn head(&self) -> u64 {
        self.head
    }

    /// Head state root.
    pub fn root(&self) -> Hash {
        self.root
    }

    /// Oldest version whose state can still be reconstructed.
    pub fn oldest_served_version(&self) -> u64 {
        self.base_version
    }

    /// Current version.
    pub fn version(&self) -> u64 {
        self.head
    }

    /// Retained delta records.
    pub fn retained_versions(&self) -> u64 {
        self.records.len() as u64
    }

    /// Number of versions dropped by pruning since start.
    pub fn pruned_versions(&self) -> u64 {
        self.pruned_versions
    }

    /// Largest encoded version record seen so far.
    pub fn max_record_bytes(&self) -> u64 {
        self.max_record_bytes
    }

    /// State root the base snapshot represents.
    pub fn base_root(&self) -> Hash {
        self.base_root
    }

    /// Bytes dropped by pruning since start.
    pub fn pruned_bytes(&self) -> u64 {
        self.pruned_bytes
    }

    /// Highest observed `disk_bytes()`.
    pub fn peak_disk_bytes(&self) -> u64 {
        self.peak_disk_bytes
    }

    /// Compactions performed.
    pub fn compaction_count(&self) -> u64 {
        self.compaction_count
    }

    /// Bytes the retained deltas occupy.
    pub fn records_bytes(&self) -> u64 {
        self.records_bytes
    }

    /// Bytes the base snapshot occupies.
    pub fn base_bytes(&self) -> u64 {
        base_len(
            self.base_version,
            self.base.len() as u64,
            self.base_entries_bytes,
        )
    }

    /// Total disk bytes: base snapshot + retained deltas.
    pub fn disk_bytes(&self) -> u64 {
        self.base_bytes() + self.records_bytes
    }

    /// Base entries (live state at `oldest_served_version`).
    pub fn base_entries(&self) -> Vec<(Hash, Vec<u8>)> {
        self.base
            .iter()
            .map(|(path, value)| (*path, value.clone()))
            .collect()
    }

    /// Live state at head.
    pub fn head_entries(&self) -> Vec<(Hash, Vec<u8>)> {
        self.tree.entries()
    }

    /// Number of BLAKE3 hashes the incremental tree has performed.
    pub fn tree_hashes(&self) -> u64 {
        self.tree.hashes()
    }

    /// Commit one version.
    pub fn commit(&mut self, changes: Vec<StateChange>) -> Result<RetentionReceipt, PruneError> {
        if changes.len() > MAX_RECORD_CHANGES {
            return Err(PruneError::TooManyChanges(changes.len()));
        }
        let version = self.head + 1;
        for change in &changes {
            let path = key_path(&change.key);
            match &change.value {
                Some(value) => self.tree.insert_digest(path, value.clone()),
                None => {
                    self.tree.remove_digest(&path);
                }
            }
        }
        let root = self.tree.state_root(version);
        let record = VersionRecord::new(version, self.root, root, changes);
        self.max_record_bytes = self.max_record_bytes.max(record.encoded_len());
        self.records_bytes += record.encoded_len();
        self.records.push_back(record);
        self.head = version;
        self.root = root;

        let before = self.pruned_versions;
        self.apply_retention();
        let receipt = RetentionReceipt {
            version,
            root,
            disk_bytes: self.disk_bytes(),
            retained_versions: self.retained_versions(),
            pruned_by_this_commit: self.pruned_versions - before,
        };
        self.peak_disk_bytes = self.peak_disk_bytes.max(receipt.disk_bytes);
        Ok(receipt)
    }

    /// Fold everything older than `head - keep_versions` into the base.
    fn apply_retention(&mut self) {
        if self.policy.mode != RetentionMode::Prune {
            return;
        }
        let target = self.head.saturating_sub(self.policy.keep_versions);
        while self.base_version < target {
            let Some(record) = self.records.pop_front() else {
                return;
            };
            debug_assert_eq!(record.version, self.base_version + 1);
            for change in &record.changes {
                let path = key_path(&change.key);
                match &change.value {
                    Some(value) => self.base_put(path, value.clone()),
                    None => self.base_remove(&path),
                }
            }
            self.base_version = record.version;
            self.base_root = record.state_root;
            self.records_bytes -= record.encoded_len();
            self.pruned_bytes += record.encoded_len();
            self.pruned_versions += 1;
        }
    }

    fn base_put(&mut self, path: Hash, value: Vec<u8>) {
        if let Some(previous) = self.base.insert(path, value.clone()) {
            self.base_entries_bytes -= base_entry_len(previous.len());
        }
        self.base_entries_bytes += base_entry_len(value.len());
    }

    fn base_remove(&mut self, path: &Hash) {
        if let Some(previous) = self.base.remove(path) {
            self.base_entries_bytes -= base_entry_len(previous.len());
        }
    }

    /// Fold every retained delta into the base and drop the records.
    ///
    /// Archive nodes refuse: compaction there would destroy the history they
    /// exist to serve.
    pub fn compact(&mut self) -> Result<CompactionReport, PruneError> {
        if self.policy.mode == RetentionMode::Archive {
            return Err(PruneError::ArchiveCompactionForbidden);
        }
        let before = self.disk_bytes();
        let mut folded = 0u64;
        while let Some(record) = self.records.pop_front() {
            for change in &record.changes {
                let path = key_path(&change.key);
                match &change.value {
                    Some(value) => self.base_put(path, value.clone()),
                    None => self.base_remove(&path),
                }
            }
            self.base_version = record.version;
            self.base_root = record.state_root;
            self.records_bytes -= record.encoded_len();
            self.pruned_bytes += record.encoded_len();
            self.pruned_versions += 1;
            folded += 1;
        }
        self.compaction_count += 1;
        Ok(CompactionReport {
            before_bytes: before,
            after_bytes: self.disk_bytes(),
            folded_records: folded,
            base_version: self.base_version,
            root: self.root,
        })
    }

    /// State root of `version`, if it can still be served.
    pub fn root_at(&self, version: u64) -> Result<Hash, PruneError> {
        if version == 0 {
            return Ok(Hash::ZERO);
        }
        if version > self.head {
            return Err(PruneError::VersionNotFound(version));
        }
        if version < self.base_version {
            return Err(PruneError::Pruned {
                version,
                oldest: self.base_version,
            });
        }
        if version == self.head {
            return Ok(self.root);
        }
        if version == self.base_version {
            return Ok(self.base_root);
        }
        self.records
            .iter()
            .find(|record| record.version == version)
            .map(|record| record.state_root)
            .ok_or(PruneError::VersionNotFound(version))
    }

    /// Reconstruct the full state at `version`.
    ///
    /// Prune mode only serves versions at or above the base; anything older is
    /// pruned, and this returns [`PruneError::Pruned`] rather than a
    /// plausible-looking wrong state.
    pub fn state_at(&self, version: u64) -> Result<Vec<(Hash, Vec<u8>)>, PruneError> {
        if version > self.head {
            return Err(PruneError::VersionNotFound(version));
        }
        if version < self.base_version {
            return Err(PruneError::Pruned {
                version,
                oldest: self.base_version,
            });
        }
        if version == self.head {
            return Ok(self.tree.entries());
        }
        let mut state = self.base.clone();
        for record in &self.records {
            if record.version > version {
                break;
            }
            apply_changes(&mut state, &record.changes);
        }
        Ok(state.into_iter().collect())
    }

    /// Recompute the state root at `version` from scratch (S1 rebuild) and
    /// compare it with the stored root. Used by the S6 gate.
    pub fn verify_root_at(&self, version: u64) -> Result<Hash, PruneError> {
        let entries = self.state_at(version)?;
        let tree = MerkleTree::from_entries(version, &entries);
        let recomputed = tree.state_root();
        if version == self.head && recomputed != self.root {
            // Cross-check the incremental tree against the S1 rebuild.
            return Err(PruneError::RootMismatch);
        }
        if version == self.base_version && recomputed != self.base_root {
            return Err(PruneError::RootMismatch);
        }
        if self.root_at(version)? != recomputed {
            return Err(PruneError::RootMismatch);
        }
        Ok(recomputed)
    }

    /// Inclusion proof for `path` at `version`.
    pub fn proof_at(&self, version: u64, path: &Hash) -> Result<MerkleProof, PruneError> {
        let entries = self.state_at(version)?;
        let tree = MerkleTree::from_entries(version, &entries);
        tree.prove_at(version, path).map_err(PruneError::Tree)
    }
}

fn apply_changes(state: &mut BTreeMap<Hash, Vec<u8>>, changes: &[StateChange]) {
    for change in changes {
        let path = key_path(&change.key);
        match &change.value {
            Some(value) => {
                state.insert(path, value.clone());
            }
            None => {
                state.remove(&path);
            }
        }
    }
}

impl From<TreeError> for PruneError {
    fn from(error: TreeError) -> Self {
        PruneError::Tree(error)
    }
}
