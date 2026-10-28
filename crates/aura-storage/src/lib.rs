//! AuraLite-Crypto durable state storage (S2).
//!
//! `aura-storage` owns the WAL + RocksDB persistence layer. It is the first
//! crate in the workspace with a **native** dependency: `rocksdb 0.23.0`,
//! linked dynamically against the system `librocksdb-dev` (see
//! `CONTRIBUTING.md` for the required environment variables). The bundled
//! C++ build is deliberately disabled — it needs gigabytes of RAM and is not
//! reproducible in CI. No `serde`, no cargo-fuzz target, no unsafe code here.
//!
//! Commit protocol: a framed, BLAKE3-checksummed WAL record is appended and
//! fsynced **before** a single atomic RocksDB `WriteBatch` publishes the state,
//! head, root and WAL index with `sync = true`. On reopen, WAL records newer
//! than the published head are replayed, so `kill -9` at any point leaves a
//! consistent head and root.
//!
//! Column families: `state`, `nonce`, `meta`, `wal_index`.
//!
//! S6 adds [`prune`]: version retention with prune mode (bounded disk),
//! archive mode (full history) and compaction. It reuses the S2 WAL record
//! shape for its version deltas but has its own magic/domain tags.
//!
//! S7 adds [`backup`]: a self-verifying backup bundle (manifest + WAL blob),
//! a restore that replays every record through the real commit path, and the
//! chaos gate that survives 50 `kill -9` + restore cycles.
//!
//! Golden vectors: `tools/gen_s2_vectors.py` (independent Python).
//! Report: `docs/evidence/s2/report.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod backend;
mod backup;
mod error;
mod prune;
mod store;
mod wal;

pub use backup::{
    BACKUP_FORMAT_VERSION, BackupBundle, BackupManifest, MANIFEST_MAGIC, MAX_BLOB_BYTES,
    MAX_CHAIN_ID_LEN, MAX_MANIFEST_BYTES, RestoreReport, S7_MANIFEST_CONTEXT, create_backup,
    manifest_checksum, restore_backup,
};
pub use error::{BackupError, PruneError, StorageError};
pub use prune::{
    BASE_MAGIC, CompactionReport, DEFAULT_KEEP_VERSIONS, MAX_KEEP_VERSIONS, MAX_RECORD_CHANGES,
    RECORD_MAGIC, RetentionMode, RetentionPolicy, RetentionReceipt, RetentionStore,
    S6_BASE_CONTEXT, S6_RECORD_CONTEXT, VersionRecord, base_checksum, encode_base, record_checksum,
};
pub use store::{CF_META, CF_NONCE, CF_STATE, CF_WAL_INDEX, Head, Receipt, StateEntries, Storage};
pub use wal::{
    S2_HEAD_CONTEXT, S2_KEY_CONTEXT, S2_ROOT_CONTEXT, S2_WAL_CONTEXT, StateChange, WAL_MAGIC, Wal,
    WalRecord, decode_record, encode_record, head_checksum, key_path, state_root, vector_changes,
    wal_checksum,
};
