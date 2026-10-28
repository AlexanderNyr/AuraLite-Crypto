//! Write-ahead log: framed, checksummed commit records.
//!
//! The WAL is the durability authority. A record is appended and `fsync`ed
//! **before** the RocksDB head/root is published, so a crash between the two
//! leaves a replayable record rather than a half-committed state.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use aura_codec::{Decode, Encode, Reader, Writer, read_varint, write_varint};
use aura_crypto::blake3_hash;
use aura_types::Hash;

use crate::error::StorageError;

/// Magic prefix of every WAL record.
pub const WAL_MAGIC: &[u8] = b"ALW1";
/// Domain tag for WAL checksums.
///
/// NC target: `tools/nc_s2.py` changes this constant and requires the S2
/// golden WAL vectors to go red.
pub const S2_WAL_CONTEXT: &[u8] = b"AuraLite-Crypto/s2-wal-v1\x00";
/// Domain tag for state-root commitments.
pub const S2_ROOT_CONTEXT: &[u8] = b"AuraLite-Crypto/s2-root-v1\x00";
/// Domain tag for state key hashing inside the tree.
pub const S2_KEY_CONTEXT: &[u8] = b"AuraLite-Crypto/s2-key-v1\x00";
/// Domain tag for head records.
pub const S2_HEAD_CONTEXT: &[u8] = b"AuraLite-Crypto/s2-head-v1\x00";

/// One state mutation inside a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateChange {
    /// State key (namespace-prefixed bytes).
    pub key: Vec<u8>,
    /// `Some` = put, `None` = delete.
    pub value: Option<Vec<u8>>,
    /// Per-key write nonce, used by the S2 vector harness.
    pub nonce: u64,
}

/// One framed WAL record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalRecord {
    /// Monotonic version (1-based).
    pub version: u64,
    /// Root of the previous version (zero for version 1).
    pub parent_root: Hash,
    /// Root after applying `changes`.
    pub state_root: Hash,
    /// Ordered state changes in this commit.
    pub changes: Vec<StateChange>,
}

/// Domain-separated hash of a state key — the Merkle-tree path.
pub fn key_path(key: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(S2_KEY_CONTEXT.len() + 8 + key.len());
    input.extend_from_slice(S2_KEY_CONTEXT);
    input.extend_from_slice(&(key.len() as u64).to_le_bytes());
    input.extend_from_slice(key);
    blake3_hash(&input)
}

/// Versioned state root over a sparse Merkle tree of `(path, value)` leaves.
///
/// NC target: `tools/nc_s2.py` changes `S2_ROOT_CONTEXT` and requires the S2
/// golden roots to go red.
pub fn state_root(version: u64, leaves: &[(Hash, Vec<u8>)]) -> Hash {
    let mut tree = aura_state::MerkleTree::new();
    for (path, value) in leaves {
        tree.insert_digest(*path, value.clone());
    }
    let tree_root = tree.tree_root();
    let mut input = Vec::with_capacity(S2_ROOT_CONTEXT.len() + 8 + 32);
    input.extend_from_slice(S2_ROOT_CONTEXT);
    input.extend_from_slice(&version.to_le_bytes());
    input.extend_from_slice(tree_root.as_bytes());
    blake3_hash(&input)
}

/// Checksum of a WAL payload.
pub fn wal_checksum(payload: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(S2_WAL_CONTEXT.len() + payload.len());
    input.extend_from_slice(S2_WAL_CONTEXT);
    input.extend_from_slice(payload);
    blake3_hash(&input)
}

/// Checksum of a published head record (version + root).
pub fn head_checksum(version: u64, root: &Hash) -> Hash {
    let mut input = Vec::with_capacity(S2_HEAD_CONTEXT.len() + 8 + 32);
    input.extend_from_slice(S2_HEAD_CONTEXT);
    input.extend_from_slice(&version.to_le_bytes());
    input.extend_from_slice(root.as_bytes());
    blake3_hash(&input)
}

fn encode_payload(record: &WalRecord) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.push_bytes(WAL_MAGIC);
    record.version.encode(&mut writer);
    writer.push_bytes(record.parent_root.as_bytes());
    writer.push_bytes(record.state_root.as_bytes());
    write_varint(&mut writer, record.changes.len() as u64);
    for change in &record.changes {
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

fn read_hash(reader: &mut Reader<'_>) -> Result<Hash, StorageError> {
    let bytes = reader
        .read_exact(32)
        .map_err(|_| StorageError::CorruptWal)?;
    Hash::from_slice(bytes).map_err(|_| StorageError::CorruptWal)
}

fn decode_payload(payload: &[u8]) -> Result<WalRecord, StorageError> {
    let mut reader = Reader::new(payload);
    let magic = reader
        .read_exact(WAL_MAGIC.len())
        .map_err(|_| StorageError::CorruptWal)?;
    if magic != WAL_MAGIC {
        return Err(StorageError::CorruptWal);
    }
    let version = u64::decode(&mut reader).map_err(|_| StorageError::CorruptWal)?;
    let parent_root = read_hash(&mut reader)?;
    let state_root = read_hash(&mut reader)?;
    let count = read_varint(&mut reader).map_err(|_| StorageError::CorruptWal)?;
    if count > 1_000_000 {
        return Err(StorageError::CorruptWal);
    }
    let mut changes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key_len = read_varint(&mut reader).map_err(|_| StorageError::CorruptWal)? as usize;
        let key = reader
            .read_exact(key_len)
            .map_err(|_| StorageError::CorruptWal)?
            .to_vec();
        let tag = reader.read_exact(1).map_err(|_| StorageError::CorruptWal)?[0];
        let value = match tag {
            0 => None,
            1 => {
                let value_len =
                    read_varint(&mut reader).map_err(|_| StorageError::CorruptWal)? as usize;
                Some(
                    reader
                        .read_exact(value_len)
                        .map_err(|_| StorageError::CorruptWal)?
                        .to_vec(),
                )
            }
            _ => return Err(StorageError::CorruptWal),
        };
        let nonce_bytes = reader.read_exact(8).map_err(|_| StorageError::CorruptWal)?;
        let mut nonce = [0u8; 8];
        nonce.copy_from_slice(nonce_bytes);
        changes.push(StateChange {
            key,
            value,
            nonce: u64::from_le_bytes(nonce),
        });
    }
    reader.finish().map_err(|_| StorageError::CorruptWal)?;
    Ok(WalRecord {
        version,
        parent_root,
        state_root,
        changes,
    })
}

/// Canonical bytes of a WAL record (framing + payload + checksum).
pub fn encode_record(record: &WalRecord) -> Vec<u8> {
    let payload = encode_payload(record);
    let checksum = wal_checksum(&payload);
    let mut writer = Writer::new();
    write_varint(&mut writer, payload.len() as u64);
    writer.push_bytes(&payload);
    writer.push_bytes(checksum.as_bytes());
    writer.into_bytes()
}

/// Decode one framed record from a reader positioned at its length prefix.
pub fn decode_record(reader: &mut Reader<'_>) -> Result<WalRecord, StorageError> {
    let length = read_varint(reader).map_err(|_| StorageError::CorruptWal)? as usize;
    if length > 64 * 1024 * 1024 {
        return Err(StorageError::CorruptWal);
    }
    let payload = reader
        .read_exact(length)
        .map_err(|_| StorageError::CorruptWal)?;
    let checksum = read_hash(reader)?;
    if wal_checksum(payload) != checksum {
        return Err(StorageError::ChecksumMismatch);
    }
    decode_payload(payload)
}

/// Append-only WAL file.
#[derive(Debug)]
pub struct Wal {
    path: PathBuf,
    file: File,
    records: Vec<WalRecord>,
    offsets: Vec<u64>,
}

impl Wal {
    /// Open (or create) a WAL and load all existing records, verifying every
    /// checksum and the version/parent-root chain.
    pub fn open(dir: &Path) -> Result<Self, StorageError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("wal.log");
        let mut bytes = Vec::new();
        if path.exists() {
            File::open(&path)?.read_to_end(&mut bytes)?;
        }
        let mut reader = Reader::new(&bytes);
        let mut records = Vec::new();
        let mut offsets = Vec::new();
        let total = bytes.len() as u64;
        let mut expected_version = 1u64;
        let mut expected_parent = Hash::ZERO;
        while reader.remaining_len() > 0 {
            let offset = total - reader.remaining_len() as u64;
            let record = decode_record(&mut reader)?;
            if record.version != expected_version {
                return Err(StorageError::NonSequentialWal);
            }
            if record.parent_root != expected_parent {
                return Err(StorageError::ParentRootMismatch);
            }
            expected_version += 1;
            expected_parent = record.state_root;
            offsets.push(offset);
            records.push(record);
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Wal {
            path,
            file,
            records,
            offsets,
        })
    }

    /// Path of the WAL file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// All verified records loaded so far.
    pub fn records(&self) -> &[WalRecord] {
        &self.records
    }

    /// Byte offsets of each record in the WAL file.
    pub fn offsets(&self) -> &[u64] {
        &self.offsets
    }

    /// Append a record and fsync it. This MUST happen before publishing head.
    pub fn append(&mut self, record: WalRecord) -> Result<(), StorageError> {
        let offset = self.len_bytes();
        let bytes = encode_record(&record);
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.offsets.push(offset);
        self.records.push(record);
        Ok(())
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True if the WAL is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Current WAL file length in bytes (offset for the next record).
    pub fn len_bytes(&self) -> u64 {
        self.file.metadata().map(|meta| meta.len()).unwrap_or(0)
    }
}

/// Deterministic state changes for the S2 vector/kill harness.
pub fn vector_changes(index: u64) -> Vec<StateChange> {
    let mut out = Vec::new();
    for slot in 0..4u64 {
        let mut seed = index
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(slot.wrapping_mul(0x2545_F491_4F6C_DD1D));
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let digest_seed = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let key_input = format!("s2-key-{index}-{slot}");
        let key = blake3_hash(key_input.as_bytes()).as_bytes()[..20].to_vec();
        let value_input = format!("s2-value-{index}-{slot}-{digest_seed}");
        let value = blake3_hash(value_input.as_bytes()).as_bytes().to_vec();
        out.push(StateChange {
            key,
            value: Some(value),
            nonce: index * 10 + slot,
        });
    }
    if index.is_multiple_of(3) {
        out.push(StateChange {
            key: b"deleted-marker".to_vec(),
            value: None,
            nonce: index * 10 + 9,
        });
    }
    out
}
