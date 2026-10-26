//! Genesis value types, canonical encoding and commitments.

use aura_block::build_tree_from_leaf_hashes;
use aura_codec::{CodecError, Decode, Encode, Reader, Writer, decode_canonical, encode_to_bytes};
use aura_crypto::{blake3_hash, hash_domain};
use aura_state::{Account, StateKey};
use aura_types::{Address, Amount, ChainId, Domain, Hash, Nonce, PROTOCOL_VERSION};

use crate::error::GenesisError;

/// Context prefix inside the genesis hash payload.
///
/// NC target: `tools/nc_c8.py` changes this constant and requires the C8
/// golden hash vectors to go red.
pub const GENESIS_CONTEXT: &[u8] = b"AuraLite-Crypto/genesis-v1\x00";

/// Context prefix for the validator-set commitment.
///
/// NC target: `tools/nc_c8.py` changes this constant and requires the C8
/// validator-set vectors to go red.
pub const VALIDATOR_SET_CONTEXT: &[u8] = b"AuraLite-Crypto/validator-set-v1\x00";

/// Context prefix for one allocation leaf.
pub const ALLOCATION_LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/allocation-leaf-v1\x00";

/// One genesis validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Validator {
    /// 20-byte account address.
    pub address: Address,
    /// Voting power, must be non-zero.
    pub power: u64,
    /// 32-byte consensus public key (opaque at C8; BLS arrives in B2).
    pub public_key: [u8; 32],
}

/// One genesis balance allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocation {
    /// 20-byte account address.
    pub address: Address,
    /// Initial balance in smallest units.
    pub balance: Amount,
}

/// A deterministic genesis document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genesis {
    /// Chain this genesis starts.
    pub chain_id: ChainId,
    /// Protocol version, must equal [`PROTOCOL_VERSION`].
    pub protocol_version: u32,
    /// Consensus timestamp in milliseconds — never a local clock.
    pub genesis_time_ms: u64,
    /// Validators in **declared order** (order is consensus-significant).
    pub validators: Vec<Validator>,
    /// Initial allocations in declared order.
    pub allocations: Vec<Allocation>,
}

impl Genesis {
    /// Build from already-parsed fields and validate all invariants.
    pub fn new(
        chain_id: ChainId,
        protocol_version: u32,
        genesis_time_ms: u64,
        validators: Vec<Validator>,
        allocations: Vec<Allocation>,
    ) -> Result<Self, GenesisError> {
        if protocol_version != PROTOCOL_VERSION {
            return Err(GenesisError::UnsupportedProtocolVersion(protocol_version));
        }
        if validators.is_empty() {
            return Err(GenesisError::EmptyValidatorSet);
        }
        let mut seen_validators = std::collections::BTreeSet::new();
        for validator in &validators {
            if validator.power == 0 {
                return Err(GenesisError::ZeroValidatorPower);
            }
            if !seen_validators.insert(validator.address) {
                return Err(GenesisError::DuplicateValidator(
                    validator.address.to_bech32(),
                ));
            }
        }
        let mut seen_allocations = std::collections::BTreeSet::new();
        for allocation in &allocations {
            if allocation.balance == Amount::ZERO {
                return Err(GenesisError::ZeroAllocation);
            }
            if !seen_allocations.insert(allocation.address) {
                return Err(GenesisError::DuplicateAllocation(
                    allocation.address.to_bech32(),
                ));
            }
        }
        let genesis = Genesis {
            chain_id,
            protocol_version,
            genesis_time_ms,
            validators,
            allocations,
        };
        genesis.total_supply()?;
        Ok(genesis)
    }

    /// Canonical binary bytes (C2 encoding, no JSON).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        encode_to_bytes(self)
    }

    /// Decode canonical binary bytes with full C2 strictness, then re-check
    /// every structural invariant (a canonical byte string is not
    /// automatically a valid genesis).
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, GenesisError> {
        let decoded = decode_canonical::<Genesis>(bytes)?;
        Genesis::new(
            decoded.chain_id,
            decoded.protocol_version,
            decoded.genesis_time_ms,
            decoded.validators,
            decoded.allocations,
        )
    }

    /// The genesis id.
    pub fn hash(&self) -> Hash {
        let mut payload = Vec::with_capacity(GENESIS_CONTEXT.len() + 256);
        payload.extend_from_slice(GENESIS_CONTEXT);
        payload.extend_from_slice(&self.canonical_bytes());
        hash_domain(self.chain_id.as_str(), Domain::Genesis, &payload)
    }

    /// Commitment to the ordered validator set (fills C5's header field).
    pub fn validator_set_hash(&self) -> Hash {
        let mut writer = Writer::new();
        self.validators.encode(&mut writer);
        let mut input = Vec::with_capacity(VALIDATOR_SET_CONTEXT.len() + writer.as_slice().len());
        input.extend_from_slice(VALIDATOR_SET_CONTEXT);
        input.extend_from_slice(writer.as_slice());
        blake3_hash(&input)
    }

    /// Merkle commitment over initial accounts (C7 keys/values, C6 tree).
    ///
    /// State is a set, so allocations are sorted by address first — JSON
    /// array order cannot change this root. This is **not** the JMT
    /// `state_root` owed by S1.
    pub fn allocation_root(&self) -> Hash {
        let mut sorted: Vec<&Allocation> = self.allocations.iter().collect();
        sorted.sort_by(|left, right| left.address.as_bytes().cmp(right.address.as_bytes()));
        let leaves: Vec<Hash> = sorted
            .iter()
            .map(|allocation| {
                let key = StateKey::account(&allocation.address);
                let account = Account::new(
                    Nonce::new(0),
                    allocation.balance,
                    Account::NO_CODE,
                    Account::EMPTY_STORAGE_ROOT,
                );
                let value = account.to_bytes();
                let mut input =
                    Vec::with_capacity(ALLOCATION_LEAF_CONTEXT.len() + 32 + 8 + value.len());
                input.extend_from_slice(ALLOCATION_LEAF_CONTEXT);
                input.extend_from_slice(key.digest().as_bytes());
                input.extend_from_slice(&(value.len() as u64).to_le_bytes());
                input.extend_from_slice(&value);
                blake3_hash(&input)
            })
            .collect();
        build_tree_from_leaf_hashes(&leaves).root()
    }

    /// Sum of all allocations with checked `u128` arithmetic.
    pub fn total_supply(&self) -> Result<Amount, GenesisError> {
        let mut supply = Amount::ZERO;
        for allocation in &self.allocations {
            supply = supply
                .checked_add(allocation.balance)
                .ok_or(GenesisError::SupplyOverflow)?;
        }
        Ok(supply)
    }

    /// Number of validators.
    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }

    /// Number of allocations.
    pub fn allocation_count(&self) -> usize {
        self.allocations.len()
    }
}

impl Encode for Validator {
    fn encode(&self, writer: &mut Writer) {
        self.address.encode(writer);
        self.power.encode(writer);
        writer.push_bytes(&self.public_key);
    }
}

impl Decode for Validator {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let address = Address::decode(reader)?;
        let power = u64::decode(reader)?;
        let key_bytes = reader.read_exact(32)?;
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(key_bytes);
        Ok(Validator {
            address,
            power,
            public_key,
        })
    }
}

impl Encode for Allocation {
    fn encode(&self, writer: &mut Writer) {
        self.address.encode(writer);
        self.balance.encode(writer);
    }
}

impl Decode for Allocation {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Allocation {
            address: Address::decode(reader)?,
            balance: Amount::decode(reader)?,
        })
    }
}

impl Encode for Genesis {
    fn encode(&self, writer: &mut Writer) {
        self.chain_id.encode(writer);
        self.protocol_version.encode(writer);
        self.genesis_time_ms.encode(writer);
        self.validators.encode(writer);
        self.allocations.encode(writer);
    }
}

impl Decode for Genesis {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Genesis {
            chain_id: ChainId::decode(reader)?,
            protocol_version: u32::decode(reader)?,
            genesis_time_ms: u64::decode(reader)?,
            validators: Vec::<Validator>::decode(reader)?,
            allocations: Vec::<Allocation>::decode(reader)?,
        })
    }
}
