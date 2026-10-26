//! Strict JSON config parsing for genesis.
//!
//! This is the **only** place `serde_json` is used. The parser accepts a
//! small, fully specified schema and rejects everything else — unknown
//! keys, wrong types, floats for balances, duplicate addresses, zero power
//! and supply overflow. It never guesses or silently defaults.

use std::collections::BTreeMap;

use aura_types::{Address, Amount, ChainId, is_our_chain};
use serde_json::{Map, Value};

use crate::error::GenesisError;
use crate::types::{Allocation, Genesis, Validator};

const ALLOWED_FIELDS: [&str; 5] = [
    "chain_id",
    "protocol_version",
    "genesis_time_ms",
    "validators",
    "allocations",
];
const ALLOWED_VALIDATOR_FIELDS: [&str; 3] = ["address", "power", "public_key"];
const ALLOWED_ALLOCATION_FIELDS: [&str; 2] = ["address", "balance"];

type Object<'a> = &'a Map<String, Value>;

fn required<'a>(object: Object<'a>, key: &'static str) -> Result<&'a Value, GenesisError> {
    object.get(key).ok_or(GenesisError::MissingField(key))
}

fn string_field<'a>(object: Object<'a>, key: &'static str) -> Result<&'a str, GenesisError> {
    required(object, key)?
        .as_str()
        .ok_or(GenesisError::WrongType(key))
}

fn u64_field(object: Object<'_>, key: &'static str) -> Result<u64, GenesisError> {
    required(object, key)?
        .as_u64()
        .ok_or(GenesisError::WrongType(key))
}

fn check_keys(object: Object<'_>, allowed: &[&str]) -> Result<(), GenesisError> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(GenesisError::UnknownNestedField(key.clone()));
        }
    }
    Ok(())
}

fn hex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = [0u8; 32];
    for (index, pair) in bytes.chunks(2).enumerate() {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out[index] = ((hi << 4) | lo) as u8;
    }
    Some(out)
}

fn parse_validator(value: &Value) -> Result<Validator, GenesisError> {
    let object = value
        .as_object()
        .ok_or(GenesisError::WrongType("validators[]"))?;
    check_keys(object, &ALLOWED_VALIDATOR_FIELDS)?;
    let address_text = string_field(object, "address")?;
    let address = Address::from_bech32(address_text)
        .map_err(|_| GenesisError::InvalidAddress(address_text.to_owned()))?;
    let power = u64_field(object, "power")?;
    if power == 0 {
        return Err(GenesisError::ZeroValidatorPower);
    }
    let key_text = string_field(object, "public_key")?;
    let public_key =
        hex32(key_text).ok_or_else(|| GenesisError::InvalidPublicKey(key_text.to_owned()))?;
    Ok(Validator {
        address,
        power,
        public_key,
    })
}

fn parse_validator_array(value: &Value) -> Result<Vec<Validator>, GenesisError> {
    let array = value
        .as_array()
        .ok_or(GenesisError::WrongType("validators"))?;
    array.iter().map(parse_validator).collect()
}

fn parse_allocation(value: &Value) -> Result<Allocation, GenesisError> {
    let object = value
        .as_object()
        .ok_or(GenesisError::WrongType("allocations[]"))?;
    check_keys(object, &ALLOWED_ALLOCATION_FIELDS)?;
    let address_text = string_field(object, "address")?;
    let address = Address::from_bech32(address_text)
        .map_err(|_| GenesisError::InvalidAddress(address_text.to_owned()))?;
    // Balances are decimal STRINGS: JSON numbers/floats are rejected because
    // u128 amounts must never round-trip through f64.
    let balance_text = string_field(object, "balance")?;
    let units: u128 = balance_text
        .parse()
        .map_err(|_| GenesisError::InvalidBalance(balance_text.to_owned()))?;
    let balance = Amount::from_units(units);
    if balance == Amount::ZERO {
        return Err(GenesisError::ZeroAllocation);
    }
    Ok(Allocation { address, balance })
}

fn parse_allocation_array(value: &Value) -> Result<Vec<Allocation>, GenesisError> {
    let array = value
        .as_array()
        .ok_or(GenesisError::WrongType("allocations"))?;
    array.iter().map(parse_allocation).collect()
}

/// Parse a genesis JSON config into a validated [`Genesis`].
///
/// Top-level object key order and whitespace are irrelevant; arrays keep
/// their declared order. `validator` order therefore changes the hash.
pub fn from_json_str(text: &str) -> Result<Genesis, GenesisError> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| GenesisError::InvalidJson(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or(GenesisError::WrongType("genesis"))?;

    // NC target: disabling this makes unknown fields silently accepted.
    for key in object.keys() {
        if !ALLOWED_FIELDS.contains(&key.as_str()) {
            return Err(GenesisError::UnknownField(key.clone()));
        }
    }

    let raw_chain = string_field(object, "chain_id")?;
    let chain_id =
        ChainId::new(raw_chain).map_err(|_| GenesisError::InvalidChainId(raw_chain.to_owned()))?;

    // NC target: disabling this lets a Stage 2 mainnet genesis start here.
    if !is_our_chain(raw_chain) {
        return Err(GenesisError::WrongChain(raw_chain.to_owned()));
    }

    let version_raw = u64_field(object, "protocol_version")?;
    let protocol_version =
        u32::try_from(version_raw).map_err(|_| GenesisError::WrongType("protocol_version"))?;
    let genesis_time_ms = u64_field(object, "genesis_time_ms")?;

    // NC target: sorting here would erase the consensus-significant order.
    let validators = parse_validator_array(required(object, "validators")?)?;
    let allocations = parse_allocation_array(required(object, "allocations")?)?;

    Genesis::new(
        chain_id,
        protocol_version,
        genesis_time_ms,
        validators,
        allocations,
    )
}

/// Parse a genesis JSON config from a file path.
pub fn from_json_file(path: &std::path::Path) -> Result<Genesis, GenesisError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| GenesisError::InvalidJson(format!("{}: {error}", path.display())))?;
    from_json_str(&text)
}

impl Genesis {
    /// A short human-readable summary used by the CLI.
    pub fn summary(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        out.insert("chain_id".to_owned(), self.chain_id.as_str().to_owned());
        out.insert(
            "protocol_version".to_owned(),
            self.protocol_version.to_string(),
        );
        out.insert(
            "genesis_time_ms".to_owned(),
            self.genesis_time_ms.to_string(),
        );
        out.insert("validators".to_owned(), self.validator_count().to_string());
        out.insert(
            "allocations".to_owned(),
            self.allocation_count().to_string(),
        );
        out.insert(
            "total_supply".to_owned(),
            self.total_supply()
                .map(|amount| amount.units().to_string())
                .unwrap_or_else(|_| "overflow".to_owned()),
        );
        out.insert("genesis_hash".to_owned(), self.hash().to_hex());
        out.insert(
            "validator_set_hash".to_owned(),
            self.validator_set_hash().to_hex(),
        );
        out.insert(
            "allocation_root".to_owned(),
            self.allocation_root().to_hex(),
        );
        out
    }
}
