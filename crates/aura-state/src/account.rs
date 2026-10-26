//! Account value: nonce, balance, code hash and storage root.

use aura_codec::{Decode, Encode, Reader, Writer, decode_canonical, encode_to_bytes};
use aura_types::{Amount, Hash, Nonce};

use crate::error::StateError;

/// The canonical account value stored under [`crate::StateKey::account`].
///
/// Wire order is fixed: `u64 nonce || u128 balance || 32-byte code_hash ||
/// 32-byte storage_root` (76 bytes, no length prefixes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account {
    /// Per-account sequence number.
    pub nonce: Nonce,
    /// Balance in smallest units (`aalcry`).
    pub balance: Amount,
    /// BLAKE3 hash of deployed code, or [`Account::NO_CODE`].
    pub code_hash: Hash,
    /// Root of the contract's own storage subtree, or
    /// [`Account::EMPTY_STORAGE_ROOT`].
    pub storage_root: Hash,
}

impl Account {
    /// Sentinel for an account with no deployed code.
    pub const NO_CODE: Hash = Hash::ZERO;

    /// Sentinel for an empty contract storage subtree.
    pub const EMPTY_STORAGE_ROOT: Hash = Hash::ZERO;

    /// A fresh externally owned account: zero nonce, zero balance, no code.
    pub fn new_empty() -> Self {
        Account {
            nonce: Nonce::new(0),
            balance: Amount::ZERO,
            code_hash: Account::NO_CODE,
            storage_root: Account::EMPTY_STORAGE_ROOT,
        }
    }

    /// Construct an account from explicit fields.
    pub fn new(nonce: Nonce, balance: Amount, code_hash: Hash, storage_root: Hash) -> Self {
        Account {
            nonce,
            balance,
            code_hash,
            storage_root,
        }
    }

    /// True when this account has deployed code.
    pub fn has_code(&self) -> bool {
        self.code_hash != Account::NO_CODE
    }

    /// Checked credit; overflow returns [`StateError::BalanceOverflow`].
    pub fn credit(&mut self, amount: Amount) -> Result<(), StateError> {
        self.balance = self
            .balance
            .checked_add(amount)
            .ok_or(StateError::BalanceOverflow)?;
        Ok(())
    }

    /// Checked debit; insufficient funds returns
    /// [`StateError::InsufficientBalance`].
    pub fn debit(&mut self, amount: Amount) -> Result<(), StateError> {
        self.balance = self
            .balance
            .checked_sub(amount)
            .ok_or(StateError::InsufficientBalance)?;
        Ok(())
    }

    /// Advance the nonce with checked `u64` arithmetic.
    pub fn bump_nonce(&mut self) -> Result<(), StateError> {
        self.nonce = self
            .nonce
            .checked_next()
            .map_err(|_| StateError::NonceOverflow)?;
        Ok(())
    }

    /// Canonical wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode_to_bytes(self)
    }

    /// Decode with full C2 canonicality.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, StateError> {
        Ok(decode_canonical::<Account>(bytes)?)
    }
}

impl Default for Account {
    fn default() -> Self {
        Account::new_empty()
    }
}

impl Encode for Account {
    fn encode(&self, writer: &mut Writer) {
        self.nonce.encode(writer);
        self.balance.encode(writer);
        self.code_hash.encode(writer);
        self.storage_root.encode(writer);
    }
}

impl Decode for Account {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, aura_codec::CodecError> {
        Ok(Account {
            nonce: Nonce::decode(reader)?,
            balance: Amount::decode(reader)?,
            code_hash: Hash::decode(reader)?,
            storage_root: Hash::decode(reader)?,
        })
    }
}
