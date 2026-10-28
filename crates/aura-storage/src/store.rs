//! Durable state store: WAL + pluggable backend with crash recovery.
//!
//! Commit protocol (fsync before head publication):
//! 1. compute the new versioned state root over the post-batch state;
//! 2. append a framed, checksummed WAL record and `fsync` it;
//! 3. publish state/head/root/wal-index in ONE atomic backend operation.
//!
//! A `kill -9` between steps 2 and 3 leaves a valid WAL record that has not
//! been published. [`Storage::open`] replays it, so head and root always
//! match after restart. This is the S2 DoD gate.

use std::collections::BTreeMap;
use std::path::Path;

use aura_types::Hash;

use crate::error::StorageError;
use crate::wal::{StateChange, Wal, WalRecord, key_path, state_root};

pub use crate::backend::{CF_META, CF_NONCE, CF_STATE, CF_WAL_INDEX, Head, Receipt, StateEntries};

/// Durable state store.
pub struct Storage {
    wal: Wal,
    backend: Box<dyn crate::backend::StateBackend>,
    head: Head,
}

impl Storage {
    /// Open with the compile-time default backend (`rocksdb` feature selects
    /// RocksDB; otherwise the pure-Rust WAL backend).
    pub fn open(dir: &Path) -> Result<Self, StorageError> {
        Storage::open_with(dir, crate::backend::open_backend(dir)?)
    }

    /// Open with the pure-Rust backend explicitly.
    pub fn open_memory(dir: &Path) -> Result<Self, StorageError> {
        Storage::open_with(dir, crate::backend::open_memory_backend(dir)?)
    }

    /// Open with the RocksDB backend explicitly (feature `rocksdb`).
    #[cfg(feature = "rocksdb")]
    pub fn open_rocksdb(dir: &Path) -> Result<Self, StorageError> {
        Storage::open_with(dir, crate::backend::open_rocksdb_backend(dir)?)
    }

    fn open_with(
        dir: &Path,
        mut backend: Box<dyn crate::backend::StateBackend>,
    ) -> Result<Self, StorageError> {
        let _ = dir;
        let wal = Wal::open(dir)?;
        let mut head = backend.read_head()?;
        backend.replay_prefix(wal.records(), head.version)?;

        // Replay WAL records fsynced but never published (crash between WAL
        // append and the atomic backend publish).
        let pending = wal
            .records()
            .iter()
            .filter(|record| record.version > head.version)
            .cloned()
            .collect::<Vec<_>>();
        for record in pending {
            let offset = wal
                .offsets()
                .get(record.version.saturating_sub(1) as usize)
                .copied()
                .unwrap_or_else(|| wal.len_bytes());
            backend.publish(&record, offset)?;
            head = Head {
                version: record.version,
                root: record.state_root,
            };
        }

        // The published head must be backed by a WAL record with the same
        // root. A head without a record means state was published before the
        // WAL was fsynced — forbidden.
        if head.version as usize > wal.len() {
            return Err(StorageError::MissingWalRecord);
        }
        if head.version > 0 {
            let backed = &wal.records()[head.version as usize - 1];
            if backed.state_root != head.root {
                return Err(StorageError::RootMismatch);
            }
        }
        let store = Storage { wal, backend, head };
        store.verify_state_root()?;
        Ok(store)
    }

    /// Published head.
    pub fn head(&self) -> Head {
        self.head
    }

    /// All verified WAL records, in order. Used by S7 backup and by audits.
    pub fn wal_records(&self) -> &[WalRecord] {
        self.wal.records()
    }

    /// Number of verified WAL records.
    pub fn wal_record_count(&self) -> usize {
        self.wal.len()
    }

    /// Path of the WAL file.
    pub fn wal_path(&self) -> &Path {
        self.wal.path()
    }

    /// Backend name for evidence.
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// Column family names exposed by the backend.
    pub fn column_families(&self) -> Vec<&'static str> {
        self.backend.column_families()
    }

    /// Read a state value.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        self.backend.get(key)
    }

    /// Read a per-key nonce.
    pub fn nonce(&self, key: &[u8]) -> Result<Option<u64>, StorageError> {
        self.backend.nonce(key)
    }

    /// All state entries sorted by key.
    pub fn entries(&self) -> Result<StateEntries, StorageError> {
        self.backend.entries()
    }

    /// WAL offset recorded for `version`.
    pub fn wal_index(&self, version: u64) -> Result<Option<u64>, StorageError> {
        self.backend.wal_index(version)
    }

    /// Recompute the versioned state root and compare with the published one.
    pub fn verify_state_root(&self) -> Result<(), StorageError> {
        if self.head.version == 0 {
            return if self.head.root == Hash::ZERO && self.entries()?.is_empty() {
                Ok(())
            } else {
                Err(StorageError::RootMismatch)
            };
        }
        let leaves = self
            .entries()?
            .into_iter()
            .map(|(key, value)| (key_path(&key), value))
            .collect::<Vec<_>>();
        if state_root(self.head.version, &leaves) != self.head.root {
            return Err(StorageError::RootMismatch);
        }
        Ok(())
    }

    /// Append a WAL record WITHOUT publishing it (crash-window simulation).
    pub fn append_wal_only(&mut self, changes: Vec<StateChange>) -> Result<Receipt, StorageError> {
        let record = self.next_record(changes)?;
        let offset = self.wal.len_bytes();
        self.wal.append(record.clone())?;
        Ok(Receipt {
            version: record.version,
            root: record.state_root,
            wal_offset: offset,
        })
    }

    /// Commit a batch: WAL append + fsync, then one atomic publish.
    pub fn commit(&mut self, changes: Vec<StateChange>) -> Result<Receipt, StorageError> {
        let record = self.next_record(changes)?;
        let offset = self.wal.len_bytes();
        self.wal.append(record.clone())?;
        self.backend.publish(&record, offset)?;
        self.head = Head {
            version: record.version,
            root: record.state_root,
        };
        Ok(Receipt {
            version: record.version,
            root: record.state_root,
            wal_offset: offset,
        })
    }

    fn next_record(&self, changes: Vec<StateChange>) -> Result<WalRecord, StorageError> {
        let version = self.head.version + 1;
        let mut state: BTreeMap<Vec<u8>, Vec<u8>> = self.entries()?.into_iter().collect();
        for change in &changes {
            match &change.value {
                Some(value) => {
                    state.insert(change.key.clone(), value.clone());
                }
                None => {
                    state.remove(&change.key);
                }
            }
        }
        let leaves = state
            .into_iter()
            .map(|(key, value)| (key_path(&key), value))
            .collect::<Vec<_>>();
        let root = state_root(version, &leaves);
        Ok(WalRecord {
            version,
            parent_root: self.head.root,
            state_root: root,
            changes,
        })
    }
}
