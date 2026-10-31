//! Pluggable state backends behind a common trait.
//!
//! The WAL is the durability authority; a backend only materialises state.
//! Two implementations exist:
//!
//! * [`MemoryBackend`] — pure Rust (`BTreeMap`), the default. It makes S2
//!   reproducible in any CI without a native library. State is rebuilt from
//!   the WAL on open.
//! * `RocksDbBackend` — real RocksDB column families and atomic
//!   `WriteBatch`, enabled by the Cargo feature `rocksdb`. It requires the
//!   system `librocksdb-dev` + `libclang`; see `CONTRIBUTING.md`.
//!
//! Both backends persist a checksummed head record. The WAL record is fsynced
//! BEFORE `publish` writes that head, so `kill -9` in between is recoverable.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use aura_types::Hash;

use crate::error::StorageError;
use crate::wal::{StateChange, WalRecord, head_checksum};

/// Column family holding `key -> value` state entries.
pub const CF_STATE: &str = "state";
/// Column family holding per-key write nonces.
pub const CF_NONCE: &str = "nonce";
/// Column family holding head/root/checksum metadata.
pub const CF_META: &str = "meta";
/// Column family mapping WAL version -> byte offset.
pub const CF_WAL_INDEX: &str = "wal_index";

const HEAD_MAGIC: &[u8] = b"ALHD";
#[cfg(feature = "rocksdb")]
const KEY_HEAD: &[u8] = b"head";
#[cfg(feature = "rocksdb")]
const KEY_ROOT: &[u8] = b"root";
#[cfg(feature = "rocksdb")]
const KEY_HEAD_CHECKSUM: &[u8] = b"head_checksum";

/// Published head of the durable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    /// Last published version (0 = empty).
    pub version: u64,
    /// Versioned state root at `version`.
    pub root: Hash,
}

/// Result of a successful commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Receipt {
    /// New published version.
    pub version: u64,
    /// New versioned state root.
    pub root: Hash,
    /// Byte offset of the WAL record for this version.
    pub wal_offset: u64,
}

/// Sorted `(key, value)` state entries.
pub type StateEntries = Vec<(Vec<u8>, Vec<u8>)>;

fn head_bytes(head: Head) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEAD_MAGIC.len() + 8 + 32 + 32);
    out.extend_from_slice(HEAD_MAGIC);
    out.extend_from_slice(&head.version.to_le_bytes());
    out.extend_from_slice(head.root.as_bytes());
    out.extend_from_slice(head_checksum(head.version, &head.root).as_bytes());
    out
}

fn parse_head(bytes: &[u8]) -> Result<Head, StorageError> {
    if bytes.len() != HEAD_MAGIC.len() + 8 + 32 + 32 {
        return Err(StorageError::CorruptWal);
    }
    let cursor = &mut &bytes[..];
    let mut magic = [0u8; 4];
    cursor.read_exact(&mut magic)?;
    if magic != HEAD_MAGIC {
        return Err(StorageError::CorruptWal);
    }
    let mut version = [0u8; 8];
    cursor.read_exact(&mut version)?;
    let version = u64::from_le_bytes(version);
    let mut root = [0u8; 32];
    cursor.read_exact(&mut root)?;
    let root = Hash::from_bytes(root);
    let mut stored = [0u8; 32];
    cursor.read_exact(&mut stored)?;
    if stored != *head_checksum(version, &root).as_bytes() {
        return Err(StorageError::ChecksumMismatch);
    }
    Ok(Head { version, root })
}

fn write_head_file(dir: &Path, head: Head) -> Result<(), StorageError> {
    let tmp = dir.join("head.bin.tmp");
    let final_path = dir.join("head.bin");
    {
        let mut file = File::create(&tmp)?;
        file.write_all(&head_bytes(head))?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &final_path)?;
    // Best-effort directory fsync so the rename itself is durable.
    let _ = File::open(dir).and_then(|handle| handle.sync_all());
    Ok(())
}

fn read_head_file(dir: &Path) -> Result<Head, StorageError> {
    let path = dir.join("head.bin");
    if !path.exists() {
        return Ok(Head {
            version: 0,
            root: Hash::ZERO,
        });
    }
    let mut bytes = Vec::new();
    File::open(&path)?.read_to_end(&mut bytes)?;
    parse_head(&bytes)
}

/// Interface every state backend must implement.
pub trait StateBackend: Send {
    /// Read and verify the published head.
    fn read_head(&self) -> Result<Head, StorageError>;
    /// Rebuild materialised state from WAL records `1..=upto`.
    fn replay_prefix(&mut self, records: &[WalRecord], upto: u64) -> Result<(), StorageError>;
    /// Read a state value.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError>;
    /// Read a per-key nonce.
    fn nonce(&self, key: &[u8]) -> Result<Option<u64>, StorageError>;
    /// All state entries sorted by key.
    fn entries(&self) -> Result<StateEntries, StorageError>;
    /// WAL offset recorded for `version`.
    fn wal_index(&self, version: u64) -> Result<Option<u64>, StorageError>;
    /// Atomically apply a record's changes and publish head/root/wal-index.
    fn publish(&mut self, record: &WalRecord, wal_offset: u64) -> Result<(), StorageError>;
    /// Backend name for evidence.
    fn name(&self) -> &'static str;
    /// Column family names.
    fn column_families(&self) -> Vec<&'static str>;
}

fn apply_changes(
    state: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    nonces: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    changes: &[StateChange],
) {
    for change in changes {
        match &change.value {
            Some(value) => {
                state.insert(change.key.clone(), value.clone());
                nonces.insert(change.key.clone(), change.nonce.to_le_bytes().to_vec());
            }
            None => {
                state.remove(&change.key);
                nonces.remove(&change.key);
            }
        }
    }
}

/// Pure-Rust backend: BTreeMaps rebuilt from the WAL on open.
pub struct MemoryBackend {
    dir: PathBuf,
    state: BTreeMap<Vec<u8>, Vec<u8>>,
    nonces: BTreeMap<Vec<u8>, Vec<u8>>,
    indices: BTreeMap<u64, u64>,
    head: Head,
}

impl MemoryBackend {
    /// Open (or create) a memory backend with a checksummed head file.
    pub fn open(dir: &Path) -> Result<Self, StorageError> {
        fs::create_dir_all(dir)?;
        let head = read_head_file(dir)?;
        Ok(MemoryBackend {
            dir: dir.to_path_buf(),
            state: BTreeMap::new(),
            nonces: BTreeMap::new(),
            indices: BTreeMap::new(),
            head,
        })
    }
}

impl StateBackend for MemoryBackend {
    fn read_head(&self) -> Result<Head, StorageError> {
        Ok(self.head)
    }

    fn replay_prefix(&mut self, records: &[WalRecord], upto: u64) -> Result<(), StorageError> {
        for record in records.iter().filter(|r| r.version <= upto) {
            apply_changes(&mut self.state, &mut self.nonces, &record.changes);
            self.indices.insert(record.version, 0);
        }
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.state.get(key).cloned())
    }

    fn nonce(&self, key: &[u8]) -> Result<Option<u64>, StorageError> {
        Ok(self.nonces.get(key).and_then(|bytes| {
            let mut array = [0u8; 8];
            if bytes.len() == 8 {
                array.copy_from_slice(bytes);
                Some(u64::from_le_bytes(array))
            } else {
                None
            }
        }))
    }

    fn entries(&self) -> Result<StateEntries, StorageError> {
        Ok(self
            .state
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect())
    }

    fn wal_index(&self, version: u64) -> Result<Option<u64>, StorageError> {
        Ok(self.indices.get(&version).copied())
    }

    fn publish(&mut self, record: &WalRecord, wal_offset: u64) -> Result<(), StorageError> {
        // All-or-nothing: build the next state first, then swap it in.
        let mut state = self.state.clone();
        let mut nonces = self.nonces.clone();
        apply_changes(&mut state, &mut nonces, &record.changes);
        let mut indices = self.indices.clone();
        indices.insert(record.version, wal_offset);
        let head = Head {
            version: record.version,
            root: record.state_root,
        };
        write_head_file(&self.dir, head)?;
        self.state = state;
        self.nonces = nonces;
        self.indices = indices;
        self.head = head;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "memory-wal"
    }

    fn column_families(&self) -> Vec<&'static str> {
        vec![CF_STATE, CF_NONCE, CF_META, CF_WAL_INDEX]
    }
}

/// Real RocksDB backend (feature `rocksdb`).
#[cfg(feature = "rocksdb")]
pub struct RocksDbBackend {
    db: rocksdb::DB,
    dir: PathBuf,
}

#[cfg(feature = "rocksdb")]
fn backend(error: rocksdb::Error) -> StorageError {
    StorageError::Backend(error.to_string())
}

#[cfg(feature = "rocksdb")]
impl RocksDbBackend {
    /// Open RocksDB with four column families.
    pub fn open(dir: &Path) -> Result<Self, StorageError> {
        let mut options = rocksdb::Options::default();
        options.create_if_missing(true);
        options.create_missing_column_families(true);
        let cfs = [CF_STATE, CF_NONCE, CF_META, CF_WAL_INDEX]
            .iter()
            .map(|name| rocksdb::ColumnFamilyDescriptor::new(*name, rocksdb::Options::default()))
            .collect::<Vec<_>>();
        let db = rocksdb::DB::open_cf_descriptors(&options, dir, cfs).map_err(backend)?;
        Ok(RocksDbBackend {
            db,
            dir: dir.to_path_buf(),
        })
    }

    fn cf(&self, name: &str) -> Result<&rocksdb::ColumnFamily, StorageError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| StorageError::Backend(format!("missing column family {name}")))
    }

    fn meta(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self
            .db
            .get_cf(self.cf(CF_META)?, key)
            .map_err(backend)?
            .map(|pinned| pinned.to_vec()))
    }
}

#[cfg(feature = "rocksdb")]
impl StateBackend for RocksDbBackend {
    fn read_head(&self) -> Result<Head, StorageError> {
        let version = match self.meta(KEY_HEAD)? {
            Some(bytes) => {
                let mut array = [0u8; 8];
                if bytes.len() != 8 {
                    return Err(StorageError::CorruptWal);
                }
                array.copy_from_slice(&bytes);
                u64::from_le_bytes(array)
            }
            None => 0,
        };
        let root = match self.meta(KEY_ROOT)? {
            Some(bytes) => Hash::from_slice(&bytes).map_err(|_| StorageError::CorruptWal)?,
            None => Hash::ZERO,
        };
        if version > 0 {
            let stored = self
                .meta(KEY_HEAD_CHECKSUM)?
                .ok_or(StorageError::MissingWalRecord)?;
            if stored != head_checksum(version, &root).as_bytes().to_vec() {
                return Err(StorageError::ChecksumMismatch);
            }
        }
        Ok(Head { version, root })
    }

    fn replay_prefix(&mut self, _records: &[WalRecord], _upto: u64) -> Result<(), StorageError> {
        // RocksDB already persists state; recovery replays only unpublished
        // records via `publish`.
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self
            .db
            .get_cf(self.cf(CF_STATE)?, key)
            .map_err(backend)?
            .map(|pinned| pinned.to_vec()))
    }

    fn nonce(&self, key: &[u8]) -> Result<Option<u64>, StorageError> {
        let Some(bytes) = self.db.get_cf(self.cf(CF_NONCE)?, key).map_err(backend)? else {
            return Ok(None);
        };
        let mut array = [0u8; 8];
        if bytes.len() != 8 {
            return Err(StorageError::CorruptWal);
        }
        array.copy_from_slice(&bytes);
        Ok(Some(u64::from_le_bytes(array)))
    }

    fn entries(&self) -> Result<StateEntries, StorageError> {
        let mut out: StateEntries = Vec::new();
        for item in self
            .db
            .iterator_cf(self.cf(CF_STATE)?, rocksdb::IteratorMode::Start)
        {
            let (key, value) = item.map_err(backend)?;
            out.push((key.to_vec(), value.to_vec()));
        }
        Ok(out)
    }

    fn wal_index(&self, version: u64) -> Result<Option<u64>, StorageError> {
        let Some(bytes) = self
            .db
            .get_cf(self.cf(CF_WAL_INDEX)?, version.to_le_bytes())
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let mut array = [0u8; 8];
        if bytes.len() != 8 {
            return Err(StorageError::CorruptWal);
        }
        array.copy_from_slice(&bytes);
        Ok(Some(u64::from_le_bytes(array)))
    }

    fn publish(&mut self, record: &WalRecord, wal_offset: u64) -> Result<(), StorageError> {
        use rocksdb::{WriteBatch, WriteOptions};

        let mut batch = WriteBatch::default();
        let state_cf = self.cf(CF_STATE)?;
        let nonce_cf = self.cf(CF_NONCE)?;
        let meta_cf = self.cf(CF_META)?;
        let index_cf = self.cf(CF_WAL_INDEX)?;
        for change in &record.changes {
            match &change.value {
                Some(value) => {
                    batch.put_cf(state_cf, &change.key, value);
                    batch.put_cf(nonce_cf, &change.key, change.nonce.to_le_bytes());
                }
                None => {
                    batch.delete_cf(state_cf, &change.key);
                    batch.delete_cf(nonce_cf, &change.key);
                }
            }
        }
        batch.put_cf(meta_cf, KEY_HEAD, record.version.to_le_bytes());
        batch.put_cf(meta_cf, KEY_ROOT, record.state_root.as_bytes());
        batch.put_cf(
            meta_cf,
            KEY_HEAD_CHECKSUM,
            head_checksum(record.version, &record.state_root).as_bytes(),
        );
        batch.put_cf(
            index_cf,
            record.version.to_le_bytes(),
            wal_offset.to_le_bytes(),
        );
        let mut write_options = WriteOptions::default();
        write_options.set_sync(true);
        self.db.write_opt(batch, &write_options).map_err(backend)?;
        // Mirror the head file so recovery paths are backend-independent.
        write_head_file(
            &self.dir,
            Head {
                version: record.version,
                root: record.state_root,
            },
        )?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "rocksdb"
    }

    fn column_families(&self) -> Vec<&'static str> {
        vec![CF_STATE, CF_NONCE, CF_META, CF_WAL_INDEX]
    }
}

/// Open the compile-time default backend.
#[cfg(feature = "rocksdb")]
pub fn open_backend(dir: &Path) -> Result<Box<dyn StateBackend>, StorageError> {
    Ok(Box::new(RocksDbBackend::open(dir)?))
}

/// Open the compile-time default backend.
#[cfg(not(feature = "rocksdb"))]
pub fn open_backend(dir: &Path) -> Result<Box<dyn StateBackend>, StorageError> {
    Ok(Box::new(MemoryBackend::open(dir)?))
}

/// Explicitly open the pure-Rust backend (always available).
pub fn open_memory_backend(dir: &Path) -> Result<Box<dyn StateBackend>, StorageError> {
    Ok(Box::new(MemoryBackend::open(dir)?))
}

/// Explicitly open the RocksDB backend (feature-gated).
#[cfg(feature = "rocksdb")]
pub fn open_rocksdb_backend(dir: &Path) -> Result<Box<dyn StateBackend>, StorageError> {
    Ok(Box::new(RocksDbBackend::open(dir)?))
}
