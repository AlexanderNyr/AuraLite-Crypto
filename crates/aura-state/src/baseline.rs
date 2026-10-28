//! S0 baseline: a deliberately naive in-memory state.
//!
//! **This module is a measurement, not a design.** It exists so that S1 has a
//! reproducible "before" picture: an append-only write log whose lookup
//! returns the *last* write for a key. Consequences, all measured by the S0
//! gates and documented in `docs/evidence/s0/baseline-report.md`:
//!
//! * there is **no state root** — [`BaselineState::state_root`] returns
//!   [`BaselineError::NoRoot`];
//! * there is **no persistence** — [`BaselineState::persist`] and
//!   [`BaselineState::load`] return [`BaselineError::NoPersistence`];
//! * two nodes applying the same writes in a different order can end with
//!   **different balances** — [`BaselineReport::deterministic`] is `false`.
//!
//! S1 replaces this with a deterministic, versioned JMT-like Merkle tree that
//! uses [`crate::StateKey::digest`] as its path. Do not build consensus code
//! on this type.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use aura_codec::{Encode, Writer, encode_to_bytes, write_varint};
use aura_types::{Amount, Hash, Nonce};

use crate::account::Account;

use crate::key::StateKey;

/// Domain prefix of the final-view serialization.
///
/// NC target: `tools/nc_s0.py` changes this constant and requires the S0
/// golden vectors to go red.
pub const FINAL_VIEW_CONTEXT: &[u8] = b"AuraLite-Crypto/s0-final-view-v1\x00";

/// Errors that express the two things S0 deliberately does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineError {
    /// No Merkle tree, therefore no `state_root`.
    NoRoot,
    /// No storage backend, therefore no save/load.
    NoPersistence,
}

impl fmt::Display for BaselineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BaselineError::NoRoot => {
                f.write_str("S0 baseline has no state root (S1 owes the tree)")
            }
            BaselineError::NoPersistence => {
                f.write_str("S0 baseline has no persistence (S2 owes RocksDB + WAL)")
            }
        }
    }
}

impl std::error::Error for BaselineError {}

/// A single naive write: set `balance`/`nonce` for one account key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineWrite {
    /// Account key (C7 `KeySpace::Account`).
    pub key: StateKey,
    /// Nonce value to store.
    pub nonce: Nonce,
    /// Balance in smallest units to store.
    pub balance: Amount,
}

impl BaselineWrite {
    /// Construct one write.
    pub fn new(key: StateKey, nonce: Nonce, balance: Amount) -> Self {
        BaselineWrite {
            key,
            nonce,
            balance,
        }
    }
}

impl Encode for BaselineWrite {
    fn encode(&self, writer: &mut Writer) {
        write_varint(writer, self.key.as_bytes().len() as u64);
        writer.push_bytes(self.key.as_bytes());
        self.nonce.encode(writer);
        self.balance.encode(writer);
    }
}

/// Measured properties of the S0 baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaselineReport {
    /// Number of writes in the log.
    pub writes: usize,
    /// Number of distinct accounts in the final view.
    pub accounts: usize,
    /// Always `false` at S0: no root exists.
    pub has_root: bool,
    /// Always `false` at S0: nothing is saved.
    pub has_persistence: bool,
    /// Always `false` at S0: the result depends on write order.
    pub deterministic: bool,
}

/// An append-only, in-memory, non-deterministic state baseline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BaselineState {
    writes: Vec<BaselineWrite>,
}

impl BaselineState {
    /// Create an empty baseline.
    pub fn new() -> Self {
        BaselineState { writes: Vec::new() }
    }

    /// Append a write. Order of appends is significant — that is the defect.
    pub fn apply(&mut self, write: BaselineWrite) {
        self.writes.push(write);
    }

    /// Number of writes in the log.
    pub fn len(&self) -> usize {
        self.writes.len()
    }

    /// True if no writes have been applied.
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }

    /// Borrow the raw write log.
    pub fn writes(&self) -> &[BaselineWrite] {
        &self.writes
    }

    /// Last-write-wins lookup.
    pub fn get(&self, key: &StateKey) -> Option<Account> {
        self.writes
            .iter()
            .rev()
            .find(|write| &write.key == key)
            .map(|write| {
                Account::new(
                    write.nonce,
                    write.balance,
                    Account::NO_CODE,
                    Account::EMPTY_STORAGE_ROOT,
                )
            })
    }

    /// Final view as a `key bytes -> (nonce, balance)` map, sorted by key.
    pub fn final_view(&self) -> BTreeMap<Vec<u8>, (Nonce, Amount)> {
        let mut view = BTreeMap::new();
        for write in &self.writes {
            view.insert(write.key.as_bytes().to_vec(), (write.nonce, write.balance));
        }
        view
    }

    /// Canonical serialization of the final view (with a domain prefix).
    pub fn final_view_bytes(&self) -> Vec<u8> {
        let view = self.final_view();
        let mut writer = Writer::new();
        writer.push_bytes(FINAL_VIEW_CONTEXT);
        write_varint(&mut writer, view.len() as u64);
        for (key, (nonce, balance)) in &view {
            write_varint(&mut writer, key.len() as u64);
            writer.push_bytes(key);
            nonce.encode(&mut writer);
            balance.encode(&mut writer);
        }
        writer.into_bytes()
    }

    /// Canonical serialization of the whole write log.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode_to_bytes(self)
    }

    /// Always fails at S0 — there is no Merkle tree and therefore no root.
    pub fn state_root(&self) -> Result<Hash, BaselineError> {
        Err(BaselineError::NoRoot)
    }

    /// Always fails at S0 — there is no storage backend.
    pub fn persist(&self, _path: &Path) -> Result<(), BaselineError> {
        Err(BaselineError::NoPersistence)
    }

    /// Always fails at S0 — there is nothing to load from.
    pub fn load(_path: &Path) -> Result<Self, BaselineError> {
        Err(BaselineError::NoPersistence)
    }

    /// The measured S0 properties.
    pub fn report(&self) -> BaselineReport {
        BaselineReport {
            writes: self.writes.len(),
            accounts: self.final_view().len(),
            has_root: false,
            has_persistence: false,
            deterministic: false,
        }
    }
}

impl Encode for BaselineState {
    fn encode(&self, writer: &mut Writer) {
        self.writes.encode(writer);
    }
}
