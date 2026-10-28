//! AuraLite-Crypto account model and state key layout (C7).
//!
//! This is the **first real API** of `aura-state`. It fixes the account value
//! ([`Account`]) and the byte layout of state keys ([`StateKey`],
//! [`KeySpace`]). S0 adds [`BaselineState`], a deliberately naive in-memory
//! baseline whose purpose is to MEASURE the absence of a state root, of
//! persistence and of deterministic write ordering. S1 adds [`MerkleTree`],
//! a deterministic, versioned sparse binary Merkle tree with inclusion
//! proofs. S3 adds [`StateView`] — a copy-on-write execution overlay with a
//! canonical [`StateDiff`] and mandatory rollback on `OutOfGas`/`Revert`.
//! S4 adds deterministic snapshots, S5 adds state sync (quorum of heads +
//! compact delta), S6 adds [`HistoryTree`] — an incrementally maintained
//! versioned tree with S1-identical roots, needed because S1 rebuilds the
//! whole arena per commit. Pruning/archive/compaction themselves live in
//! `aura-storage::prune`; backup/recovery (S7) remains open.
//! C7 is deliberately a *layout* phase: no storage backend, no root
//! computation, no fake `Ok(())`.
//!
//! Invariants:
//! * five known key namespaces (`0x01`–`0x05`); everything else — including
//!   `0x00` and `0x06`–`0xFF` — is **reserved** and cannot be decoded into a
//!   [`StateKey`];
//! * every namespace has one fixed key length, so two accounts can never
//!   collide and a key cannot alias another namespace;
//! * a contract may write only under its own `0x03 || contract || slot`
//!   prefix, checked by [`assert_contract_may_write`];
//! * [`StateKey::digest`] is domain-separated with an explicit length, ready
//!   for the JMT path hashing in S1;
//! * all account arithmetic is checked (`u128` balance, `u64` nonce).
//!
//! Full specification: `docs/spec/account-model.md`.
//! Golden vectors: `tools/gen_c7_vectors.py` (independent Python).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod account;
mod baseline;
mod error;
mod history;
mod key;
mod overlay;
mod snapshot;
mod sync;
mod tree;

pub use account::Account;
pub use baseline::{
    BaselineError, BaselineReport, BaselineState, BaselineWrite, FINAL_VIEW_CONTEXT,
};
pub use error::{OverlayError, SnapshotError, StateError, SyncError};
pub use history::HistoryTree;
pub use key::{
    KeySpace, MAX_STATE_KEY_LEN, STATE_KEY_CONTEXT, StateKey, assert_contract_may_write,
};
pub use overlay::{
    DIFF_MAGIC, MAX_EVENTS, MAX_VALUE_LEN, MAX_WRITES, S3_DIFF_CONTEXT, S3_EVENT_CONTEXT,
    StateDiff, StateView, apply_diff, execute,
};
pub use snapshot::{
    ChunkDescriptor, MAX_CHUNK_BYTES, MAX_CHUNKS, MAX_KEY_BYTES, MAX_VALUE_BYTES,
    SNAPSHOT_CHUNK_CONTEXT, SNAPSHOT_MAGIC, SNAPSHOT_MANIFEST_CONTEXT, SNAPSHOT_ROOT_CONTEXT,
    SNAPSHOT_VERSION, Snapshot, SnapshotManifest, StateEntries, chunk_hash, chunk_root,
    decode_chunk, encode_chunk, export_snapshot, import_snapshot, manifest_bytes,
    parallel_chunk_hashes, sha256_bytes, state_root,
};
pub use sync::{
    DELTA_MAGIC, HEAD_MAGIC, MAX_BLOB_BYTES, MAX_DELTA_CHANGES, PACKAGE_MAGIC, S5_DELTA_CONTEXT,
    S5_HEAD_CONTEXT, S5_PACKAGE_CONTEXT, SYNC_QUORUM, SYNC_VERSION, SyncDelta, SyncHead,
    SyncPackage, SyncResult,
};
pub use tree::{
    MerkleProof, MerkleTree, S1_ADDR_CONTEXT, S1_EMPTY_CONTEXT, S1_LEAF_CONTEXT, S1_NODE_CONTEXT,
    S1_ROOT_CONTEXT, S1_VALUE_CONTEXT, TreeError, empty_hash, key_bit, leaf_hash, node_hash,
    state_path, vector_address, vector_value, verify_inclusion, verify_proof, versioned_root,
};
