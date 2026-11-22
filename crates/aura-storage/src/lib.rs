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

/// T2 allocation: genesis split (validators/treasury/community/team) with
/// vesting cliffs and lockups (ECONOMICS_PLAN T2).
pub mod allocation;
mod backend;
mod backup;
/// T0 economics baseline: deterministic supply simulation with invariants
/// and no token (ECONOMICS_PLAN T0).
pub mod econ;
mod error;
/// E2 chain explorer: read-only views over the E1 indexer plus a
/// single-page web UI (ECOSYSTEM_PLAN E2).
pub mod explorer;
/// E3 rate-limited faucet: 1 payout per address per day, testnet only
/// (ECOSYSTEM_PLAN E3).
pub mod faucet;
/// T3 fees: EIP-1559-like `base_fee` + `tip` market, burned base part
/// and elastic gas target (ECONOMICS_PLAN T3).
pub mod fees;
pub mod governance;
/// E1 chain indexer: idempotent ingest + reorg rollback over a local
/// row store (ECOSYSTEM_PLAN E1; Postgres replaced by deviation).
pub mod indexer;
/// T1 issuance model: genesis supply, halving curve, hard cap and target
/// staked ratio for `ALCRY` (ECONOMICS_PLAN T1, decision D-T1).
pub mod issuance;
mod prune;
/// E3 client SDK (Rust) for the explorer and faucet HTTP APIs.
pub mod sdk;
/// T4 staking: min stake, delegation with shares, commission, 21-day
/// unbonding and a slashing period (ECONOMICS_PLAN T4).
pub mod slashing;
pub mod staking;
mod store;
pub mod treasury;
mod wal;
pub mod whitepaper;

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
