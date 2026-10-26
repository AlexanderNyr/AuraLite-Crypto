//! State key layout: namespaces, reserved prefixes and key digests.

use aura_codec::{Reader, Writer, read_varint, write_varint};
use aura_crypto::blake3_hash;
use aura_types::{Address, Hash};

use crate::error::StateError;

/// Domain tag for the JMT path hash of a state key.
///
/// NC target: `tools/nc_c7.py` changes this constant and requires the C7
/// golden digest vectors to go red.
pub const STATE_KEY_CONTEXT: &[u8] = b"AuraLite-Crypto/state-key-v1\x00";

/// Maximum encoded key length in bytes.
pub const MAX_STATE_KEY_LEN: usize = 128;

/// A protocol namespace. Tags `0x01`–`0x05` are the only valid ones;
/// `0x00` and `0x06`–`0xFF` are reserved for future protocol use and must
/// never be reachable from contract code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeySpace {
    /// `0x01` — account values.
    Account,
    /// `0x02` — contract bytecode.
    ContractCode,
    /// `0x03` — contract storage slots.
    ContractStorage,
    /// `0x04` — validator / staking metadata.
    Staking,
    /// `0x05` — transaction receipts and events.
    Receipt,
}

impl KeySpace {
    /// All known namespaces in tag order.
    pub const ALL: [KeySpace; 5] = [
        KeySpace::Account,
        KeySpace::ContractCode,
        KeySpace::ContractStorage,
        KeySpace::Staking,
        KeySpace::Receipt,
    ];

    /// Stable protocol tag.
    pub const fn tag(self) -> u8 {
        match self {
            KeySpace::Account => 0x01,
            KeySpace::ContractCode => 0x02,
            KeySpace::ContractStorage => 0x03,
            KeySpace::Staking => 0x04,
            KeySpace::Receipt => 0x05,
        }
    }

    /// Parse a protocol tag.
    pub const fn from_tag(tag: u8) -> Option<KeySpace> {
        match tag {
            0x01 => Some(KeySpace::Account),
            0x02 => Some(KeySpace::ContractCode),
            0x03 => Some(KeySpace::ContractStorage),
            0x04 => Some(KeySpace::Staking),
            0x05 => Some(KeySpace::Receipt),
            _ => None,
        }
    }

    /// True only for the five protocol namespaces.
    pub const fn is_known(tag: u8) -> bool {
        matches!(tag, 0x01..=0x05)
    }

    /// Required total key length, including the one-byte prefix.
    pub const fn expected_len(self) -> usize {
        match self {
            KeySpace::Account | KeySpace::ContractCode | KeySpace::Staking => 1 + 20,
            KeySpace::ContractStorage => 1 + 20 + 32,
            KeySpace::Receipt => 1 + 32,
        }
    }
}

/// A validated state key. Only [`KeySpace`] constructors or
/// [`StateKey::from_raw`] can produce one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StateKey(Vec<u8>);

impl StateKey {
    fn from_parts(prefix: u8, rest: &[&[u8]]) -> Self {
        let mut raw = Vec::with_capacity(1 + rest.iter().map(|part| part.len()).sum::<usize>());
        raw.push(prefix);
        for part in rest {
            raw.extend_from_slice(part);
        }
        StateKey(raw)
    }

    /// `0x01 || address[20]` — account value.
    pub fn account(address: &Address) -> Self {
        StateKey::from_parts(KeySpace::Account.tag(), &[address.as_bytes()])
    }

    /// `0x02 || address[20]` — contract bytecode.
    pub fn code(address: &Address) -> Self {
        StateKey::from_parts(KeySpace::ContractCode.tag(), &[address.as_bytes()])
    }

    /// `0x03 || contract[20] || slot[32]` — contract storage.
    pub fn storage(contract: &Address, slot: &[u8; 32]) -> Self {
        StateKey::from_parts(
            KeySpace::ContractStorage.tag(),
            &[contract.as_bytes(), slot],
        )
    }

    /// `0x04 || validator[20]` — staking metadata.
    pub fn staking(validator: &Address) -> Self {
        StateKey::from_parts(KeySpace::Staking.tag(), &[validator.as_bytes()])
    }

    /// `0x05 || txid[32]` — receipts and events.
    pub fn receipt(txid: &Hash) -> Self {
        StateKey::from_parts(KeySpace::Receipt.tag(), &[txid.as_bytes()])
    }

    /// Borrow the raw key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Namespace of this key (validated at construction).
    pub fn space(&self) -> KeySpace {
        KeySpace::from_tag(self.0[0]).expect("StateKey always has a known prefix")
    }

    /// Domain-separated digest used as the JMT path in S1.
    pub fn digest(&self) -> Hash {
        let mut input = Vec::with_capacity(STATE_KEY_CONTEXT.len() + 8 + self.0.len());
        input.extend_from_slice(STATE_KEY_CONTEXT);
        input.extend_from_slice(&(self.0.len() as u64).to_le_bytes());
        input.extend_from_slice(&self.0);
        blake3_hash(&input)
    }

    /// Canonical wire bytes: `varint(len) || raw`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        write_varint(&mut writer, self.0.len() as u64);
        writer.push_bytes(&self.0);
        writer.into_bytes()
    }

    /// Decode with full canonicality: minimal varint, no trailing bytes,
    /// known prefix, exact namespace length, and byte-identical re-encoding.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, StateError> {
        let mut reader = Reader::new(bytes);
        let len = read_varint(&mut reader).map_err(StateError::Decode)?;
        if len > MAX_STATE_KEY_LEN as u64 {
            return Err(StateError::KeyTooLong(len as usize));
        }
        let raw = reader
            .read_exact(len as usize)
            .map_err(StateError::Decode)?;
        reader.finish().map_err(StateError::Decode)?;
        let key = StateKey::from_raw(raw)?;
        if key.to_bytes() != bytes {
            return Err(StateError::NonCanonicalKeyEncoding);
        }
        Ok(key)
    }

    /// Validate raw key bytes (prefix + namespace-specific length).
    ///
    /// NC targets: `tools/nc_c7.py` disables the reserved-prefix check and
    /// the length check here, requiring the C7 vectors to go red.
    pub fn from_raw(raw: &[u8]) -> Result<Self, StateError> {
        if raw.is_empty() {
            return Err(StateError::EmptyKey);
        }
        if raw.len() > MAX_STATE_KEY_LEN {
            return Err(StateError::KeyTooLong(raw.len()));
        }
        let prefix = raw[0];
        if !KeySpace::is_known(prefix) {
            return Err(StateError::ReservedKeySpace(prefix));
        }
        let space = KeySpace::from_tag(prefix).expect("checked above");
        if raw.len() != space.expected_len() {
            return Err(StateError::InvalidKeyLength {
                space,
                expected: space.expected_len(),
                got: raw.len(),
            });
        }
        Ok(StateKey(raw.to_vec()))
    }
}

/// Enforce that `contract` may write `key`.
///
/// A contract can touch only `0x03 || its_own_address || slot`. It cannot
/// reach account/code/staking/receipt namespaces, another contract's storage,
/// or any reserved prefix (those keys cannot even be constructed).
///
/// NC targets: `tools/nc_c7.py` disables the namespace check and the
/// contract-address check here.
pub fn assert_contract_may_write(contract: &Address, key: &StateKey) -> Result<(), StateError> {
    if key.space() != KeySpace::ContractStorage {
        return Err(StateError::ForbiddenContractWrite);
    }
    if &key.as_bytes()[1..21] != contract.as_bytes().as_slice() {
        return Err(StateError::WrongContractStorage);
    }
    Ok(())
}
