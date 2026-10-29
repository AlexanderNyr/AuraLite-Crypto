//! S3: execution overlay (state view, canonical diff, rollback).
//!
//! `StateView` is layered on an immutable S1 [`MerkleTree`] base. Writes go
//! to a `BTreeMap` overlay; the base tree is never mutated. [`execute`] runs a
//! closure and, on **any** error, calls [`StateView::rollback`] so the
//! overlay, events and gas are discarded. A successful execution produces a
//! [`StateDiff`] whose canonical bytes are the only thing that may be committed.
//! `apply_diff` re-verifies the diff's base and result roots before
//! materialising a new tree.
//!
//! Golden vectors: `tools/gen_s3_vectors.py` (independent Python).

use std::collections::BTreeMap;

use aura_codec::{Decode, Encode, Reader, Writer, read_varint, write_varint};
use aura_crypto::blake3_hash;
use aura_types::{ChainId, Hash};

use crate::error::OverlayError;
use crate::key::StateKey;
use crate::tree::{MerkleTree, state_path, versioned_root};

/// Magic prefix of every canonical diff payload.
pub const DIFF_MAGIC: &[u8] = b"ALD3";
/// Domain tag for diff checksums. (NC target: `tools/nc_s3.py`.)
pub const S3_DIFF_CONTEXT: &[u8] = b"AuraLite-Crypto/s3-diff-v1\x00";
/// Domain tag for the event commitment. (NC target: `tools/nc_s3.py`.)
pub const S3_EVENT_CONTEXT: &[u8] = b"AuraLite-Crypto/s3-event-v1\x00";

/// Maximum number of state changes in one diff.
pub const MAX_WRITES: usize = 65_536;
/// Maximum number of events in one diff.
pub const MAX_EVENTS: usize = 4_096;
/// Maximum byte length of a stored value or event.
pub const MAX_VALUE_LEN: usize = 1 << 20;

/// The canonical, verifiable result of one execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDiff {
    /// Version this diff commits (`base.version() + 1`).
    pub version: u64,
    /// Versioned root of the base tree.
    pub base_root: Hash,
    /// Versioned root after the diff is applied.
    pub result_root: Hash,
    /// Gas consumed by this execution.
    pub gas_used: u64,
    /// Emitted events, in order.
    pub events: Vec<Vec<u8>>,
    /// `(path, value)` changes sorted ascending by path; `None` = delete.
    pub changes: Vec<(Hash, Option<Vec<u8>>)>,
}

impl StateDiff {
    /// Canonical wire bytes: `payload || BLAKE3(S3_DIFF_CONTEXT || payload)`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.push_bytes(DIFF_MAGIC);
        self.version.encode(&mut writer);
        self.base_root.encode(&mut writer);
        self.result_root.encode(&mut writer);
        self.gas_used.encode(&mut writer);
        write_varint(&mut writer, self.events.len() as u64);
        for event in &self.events {
            write_varint(&mut writer, event.len() as u64);
            writer.push_bytes(event);
        }
        write_varint(&mut writer, self.changes.len() as u64);
        for (path, value) in &self.changes {
            path.encode(&mut writer);
            match value {
                Some(bytes) => {
                    writer.push_bytes(&[1]);
                    write_varint(&mut writer, bytes.len() as u64);
                    writer.push_bytes(bytes);
                }
                None => writer.push_bytes(&[0]),
            }
        }
        let payload = writer.into_bytes();
        let mut out = payload.clone();
        let checksum = blake3_hash(&[S3_DIFF_CONTEXT, &payload].concat());
        out.extend_from_slice(checksum.as_bytes());
        out
    }

    /// Strict canonical decode: checksum, magic, minimal varints, sorted
    /// changes, caps, no trailing bytes, byte-identical re-encode.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, OverlayError> {
        if bytes.len() < 32 {
            return Err(OverlayError::UnexpectedEof);
        }
        let split = bytes.len() - 32;
        let (payload, checksum_bytes) = bytes.split_at(split);
        let expected = blake3_hash(&[S3_DIFF_CONTEXT, payload].concat());
        if expected.as_bytes() != checksum_bytes {
            return Err(OverlayError::BadChecksum);
        }
        let mut reader = Reader::new(payload);
        let magic = reader.read_exact(4)?;
        if magic != DIFF_MAGIC {
            return Err(OverlayError::BadMagic);
        }
        let version = u64::decode(&mut reader)?;
        let base_root = Hash::decode(&mut reader)?;
        let result_root = Hash::decode(&mut reader)?;
        let gas_used = u64::decode(&mut reader)?;

        let event_count = read_varint(&mut reader)?;
        if event_count > MAX_EVENTS as u64 {
            return Err(OverlayError::TooManyEvents(event_count as usize));
        }
        let mut events = Vec::with_capacity(event_count as usize);
        for _ in 0..event_count {
            let len = read_varint(&mut reader)? as usize;
            if len > MAX_VALUE_LEN {
                return Err(OverlayError::ValueTooLarge(len));
            }
            events.push(reader.read_exact(len)?.to_vec());
        }

        let change_count = read_varint(&mut reader)?;
        if change_count > MAX_WRITES as u64 {
            return Err(OverlayError::TooManyWrites(change_count as usize));
        }
        let mut changes: Vec<(Hash, Option<Vec<u8>>)> = Vec::with_capacity(change_count as usize);
        for _ in 0..change_count {
            let path = Hash::decode(&mut reader)?;
            if changes.last().is_some_and(|(prev, _)| *prev >= path) {
                return Err(OverlayError::UnsortedChanges);
            }
            let tag = reader.read_exact(1)?[0];
            let value = match tag {
                0 => None,
                1 => {
                    let len = read_varint(&mut reader)? as usize;
                    if len > MAX_VALUE_LEN {
                        return Err(OverlayError::ValueTooLarge(len));
                    }
                    Some(reader.read_exact(len)?.to_vec())
                }
                other => return Err(OverlayError::InvalidOptionTag(other)),
            };
            changes.push((path, value));
        }
        reader.finish()?;
        let diff = StateDiff {
            version,
            base_root,
            result_root,
            gas_used,
            events,
            changes,
        };
        if diff.to_bytes().as_slice() != bytes {
            return Err(OverlayError::Decode(
                "re-encoding did not reproduce the input".to_owned(),
            ));
        }
        Ok(diff)
    }

    /// Commitment to the emitted events.
    pub fn events_root(&self) -> Hash {
        let mut writer = Writer::new();
        writer.push_bytes(S3_EVENT_CONTEXT);
        write_varint(&mut writer, self.events.len() as u64);
        for event in &self.events {
            write_varint(&mut writer, event.len() as u64);
            writer.push_bytes(event);
        }
        blake3_hash(writer.as_slice())
    }

    /// Materialise this diff on `base`, re-verifying both roots.
    pub fn apply(&self, base: &MerkleTree) -> Result<MerkleTree, OverlayError> {
        let expected_base = versioned_root(base.version(), &base.tree_root());
        if expected_base != self.base_root {
            return Err(OverlayError::BaseRootMismatch);
        }
        let mut entries: BTreeMap<Hash, Vec<u8>> = base.entries().into_iter().collect();
        for (path, value) in &self.changes {
            match value {
                Some(bytes) => {
                    entries.insert(*path, bytes.clone());
                }
                None => {
                    entries.remove(path);
                }
            }
        }
        let ordered: Vec<(Hash, Vec<u8>)> = entries.into_iter().collect();
        let tree = MerkleTree::from_entries(self.version, &ordered);
        let computed = versioned_root(self.version, &tree.tree_root());
        if computed != self.result_root {
            return Err(OverlayError::RootMismatch);
        }
        Ok(tree)
    }
}

/// Copy-on-write view of an S1 Merkle tree.
///
/// NC target: `tools/nc_s3.py` removes the rollback call in [`execute`].
/// The `revert does not change root` gate must go red when that happens.
#[derive(Debug, Clone)]
pub struct StateView {
    base: MerkleTree,
    changes: BTreeMap<Hash, Option<Vec<u8>>>,
    gas_limit: u64,
    gas_used: u64,
    events: Vec<Vec<u8>>,
}

impl StateView {
    /// Create a view over an immutable base tree.
    pub fn new(base: MerkleTree, gas_limit: u64) -> Self {
        StateView {
            base,
            changes: BTreeMap::new(),
            gas_limit,
            gas_used: 0,
            events: Vec::new(),
        }
    }

    /// Chain identifier this view belongs to (Stage 1 identity).
    pub fn chain_id(&self) -> ChainId {
        ChainId::stage1()
    }

    /// Versioned root of the immutable base.
    pub fn base_root(&self) -> Hash {
        versioned_root(self.base.version(), &self.base.tree_root())
    }

    /// Read through the overlay; a deleted key returns `None`.
    pub fn get(&self, key: &StateKey) -> Option<&[u8]> {
        self.get_path(&state_path(key))
    }

    /// Read through the overlay using a precomputed path.
    pub fn get_path(&self, path: &Hash) -> Option<&[u8]> {
        match self.changes.get(path) {
            Some(value) => value.as_deref(),
            None => self.base.get_digest(path),
        }
    }

    /// Write a value into the overlay.
    pub fn set(&mut self, key: &StateKey, value: Vec<u8>) {
        self.set_path(state_path(key), value);
    }

    /// Write a value using a precomputed path.
    pub fn set_path(&mut self, path: Hash, value: Vec<u8>) {
        self.changes.insert(path, Some(value));
    }

    /// Delete a key in the overlay.
    pub fn delete(&mut self, key: &StateKey) {
        self.delete_path(state_path(key));
    }

    /// Delete a key using a precomputed path.
    pub fn delete_path(&mut self, path: Hash) {
        self.changes.insert(path, None);
    }

    /// Version of the immutable base tree.
    pub fn base_version(&self) -> u64 {
        self.base.version()
    }

    /// Emit an event into the overlay.
    pub fn emit(&mut self, event: Vec<u8>) {
        self.events.push(event);
    }

    /// Charge gas; on `OutOfGas` the caller must roll back the whole view.
    pub fn charge_gas(&mut self, needed: u64) -> Result<(), OverlayError> {
        let used = self
            .gas_used
            .checked_add(needed)
            .ok_or(OverlayError::Overflow)?;
        if used > self.gas_limit {
            return Err(OverlayError::OutOfGas {
                limit: self.gas_limit,
                used: self.gas_used,
                needed,
            });
        }
        self.gas_used = used;
        Ok(())
    }

    /// Gas consumed so far.
    pub fn gas_used(&self) -> u64 {
        self.gas_used
    }

    /// Gas still available to the current execution.
    ///
    /// X2 uses this as the *single* budget: the instruction meter is reset to
    /// this number at every host-call boundary, so instruction fuel and host
    /// charges are two views of one counter and can never disagree.
    pub fn gas_left(&self) -> u64 {
        self.gas_limit.saturating_sub(self.gas_used)
    }

    /// Events emitted so far.
    pub fn events(&self) -> &[Vec<u8>] {
        &self.events
    }

    /// Number of pending changes.
    pub fn pending_len(&self) -> usize {
        self.changes.len()
    }

    /// Apply the overlay to a copy of the base tree without mutating it.
    pub fn materialize(&self, version: u64) -> MerkleTree {
        let mut entries: BTreeMap<Hash, Vec<u8>> = self.base.entries().into_iter().collect();
        for (path, value) in &self.changes {
            match value {
                Some(bytes) => {
                    entries.insert(*path, bytes.clone());
                }
                None => {
                    entries.remove(path);
                }
            }
        }
        let ordered: Vec<(Hash, Vec<u8>)> = entries.into_iter().collect();
        MerkleTree::from_entries(version, &ordered)
    }

    /// Versioned root of the materialised view.
    pub fn root(&self, version: u64) -> Hash {
        versioned_root(version, &self.materialize(version).tree_root())
    }

    /// Discard every uncommitted change, event and gas unit. This is the
    /// rollback that must happen on `OutOfGas` or `Revert`.
    pub fn rollback(&mut self) {
        self.changes.clear();
        self.events.clear();
        self.gas_used = 0;
    }

    /// Consume the view into a canonical diff at `version`.
    pub fn commit(self, version: u64) -> StateDiff {
        self.diff(version)
    }

    /// Snapshot the overlay as a canonical diff at `version`.
    pub fn diff(&self, version: u64) -> StateDiff {
        StateDiff {
            version,
            base_root: self.base_root(),
            result_root: self.root(version),
            gas_used: self.gas_used,
            events: self.events.clone(),
            changes: self
                .changes
                .iter()
                .map(|(path, value)| (*path, value.clone()))
                .collect(),
        }
    }
}

/// Run `f` against a view; on error the view is rolled back.
pub fn execute<T, F>(
    view: &mut StateView,
    version: u64,
    f: F,
) -> Result<(T, StateDiff), OverlayError>
where
    F: FnOnce(&mut StateView) -> Result<T, OverlayError>,
{
    let result = f(view);
    match result {
        Ok(value) => Ok((value, view.diff(version))),
        Err(error) => {
            view.rollback();
            Err(error)
        }
    }
}

/// Verify a diff against a base tree and materialise it.
pub fn apply_diff(base: &MerkleTree, diff: &StateDiff) -> Result<MerkleTree, OverlayError> {
    diff.apply(base)
}
