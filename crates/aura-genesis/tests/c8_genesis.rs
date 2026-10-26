//! C8 gate: deterministic genesis against an independent Python generator.
//!
//! `fixtures/c8_golden.txt` comes from `tools/gen_c8_vectors.py`: pure-Python
//! BLAKE3 (official-vector self-check), independent bech32, and a C8 wire
//! transcription. JSON documents are hex-encoded UTF-8.

use std::collections::BTreeMap;

use aura_codec::CodecError;
use aura_genesis::{
    ALLOCATION_LEAF_CONTEXT, GENESIS_CONTEXT, GenesisError, VALIDATOR_SET_CONTEXT, from_json_str,
};
use aura_types::{CHAIN_ID, PROTOCOL_VERSION};

const GOLDEN: &str = include_str!("fixtures/c8_golden.txt");
const SAMPLE_JSON: &str = include_str!("../../../docs/spec/genesis-testnet.json");

fn parse(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .unwrap_or_else(|| panic!("malformed golden line: {trimmed}"));
        assert!(
            map.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate golden key {key}"
        );
    }
    map
}

fn hx(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0, "even-length hex");
    text.as_bytes()
        .chunks(2)
        .map(|pair| (nibble(pair[0]) << 4) | nibble(pair[1]))
        .collect()
}

fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => panic!("invalid hex nibble in fixture"),
    }
}

fn json_doc(f: &BTreeMap<String, String>, key: &str) -> String {
    String::from_utf8(hx(&f[key])).unwrap_or_else(|e| panic!("{key} is not UTF-8: {e}"))
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn genesis(f: &BTreeMap<String, String>, key: &str) -> aura_genesis::Genesis {
    from_json_str(&json_doc(f, key)).unwrap_or_else(|e| panic!("{key} failed: {e}"))
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(GENESIS_CONTEXT), f["genesis_context"]);
    assert_eq!(to_hex(VALIDATOR_SET_CONTEXT), f["validator_set_context"]);
    assert_eq!(
        to_hex(ALLOCATION_LEAF_CONTEXT),
        f["allocation_leaf_context"]
    );
    assert_eq!(CHAIN_ID, f["chain_id"]);
    assert_eq!(PROTOCOL_VERSION.to_string(), f["protocol_version"]);
}

#[test]
fn same_genesis_same_hash_across_three_hosts() {
    // C8 DoD: three independent textual representations of the SAME config
    // (minified, reordered object keys, pretty-printed) must give one hash.
    let f = parse(GOLDEN);
    let hosts = ["json_min", "json_keys_reordered", "json_pretty"];
    let mut hashes = Vec::new();
    let mut canonical = Vec::new();
    for key in hosts {
        let genesis = genesis(&f, key);
        assert_eq!(genesis.chain_id.as_str(), CHAIN_ID);
        assert_eq!(
            genesis.validator_count().to_string(),
            f["validator_count_1"]
        );
        assert_eq!(
            genesis.allocation_count().to_string(),
            f["allocation_count_1"]
        );
        assert_eq!(
            genesis.total_supply().unwrap().units().to_string(),
            f["total_supply_1"]
        );
        canonical.push(to_hex(&genesis.canonical_bytes()));
        hashes.push(genesis.hash().to_hex());
        assert_eq!(
            genesis.validator_set_hash().to_hex(),
            f["validator_set_hash_1"],
            "{key}"
        );
        assert_eq!(
            genesis.allocation_root().to_hex(),
            f["allocation_root_1"],
            "{key}"
        );
    }
    assert!(hashes.iter().all(|hash| hash == &hashes[0]));
    assert!(canonical.iter().all(|bytes| bytes == &canonical[0]));
    assert_eq!(hashes[0], f["genesis_1_hash"]);
    assert_eq!(canonical[0], f["genesis_1_canonical"]);
    assert_eq!(hashes.len(), 3, "three hosts measured");
}

#[test]
fn validator_order_changes_the_hash() {
    // C8 DoD / NC target: declared validator order is consensus-significant.
    let f = parse(GOLDEN);
    let base = genesis(&f, "json_min");
    let reordered = genesis(&f, "json_validators_reordered");
    assert_ne!(base.hash(), reordered.hash());
    assert_ne!(base.validator_set_hash(), reordered.validator_set_hash());
    assert_eq!(base.validator_count(), reordered.validator_count());
    assert_eq!(base.allocation_root(), reordered.allocation_root());
    assert_eq!(base.hash().to_hex(), f["genesis_1_hash"]);
}

#[test]
fn canonical_bytes_and_commitments_match_independent_python() {
    let f = parse(GOLDEN);
    for (index, key) in [(1, "json_min"), (2, "json_second")] {
        let genesis = genesis(&f, key);
        assert_eq!(
            to_hex(&genesis.canonical_bytes()),
            f[&format!("genesis_{index}_canonical")]
        );
        assert_eq!(genesis.hash().to_hex(), f[&format!("genesis_{index}_hash")]);
        assert_eq!(
            genesis.validator_set_hash().to_hex(),
            f[&format!("validator_set_hash_{index}")]
        );
        assert_eq!(
            genesis.allocation_root().to_hex(),
            f[&format!("allocation_root_{index}")]
        );
        assert_eq!(
            genesis.total_supply().unwrap().units().to_string(),
            f[&format!("total_supply_{index}")]
        );
        assert_eq!(
            genesis.validator_count().to_string(),
            f[&format!("validator_count_{index}")]
        );
        assert_eq!(
            genesis.allocation_count().to_string(),
            f[&format!("allocation_count_{index}")]
        );

        let bytes = genesis.canonical_bytes();
        assert_eq!(
            aura_genesis::Genesis::from_canonical_bytes(&bytes).unwrap(),
            genesis
        );
    }
}

#[test]
fn allocation_root_is_order_independent_but_genesis_hash_is_not() {
    // State is a set: allocation_root cannot depend on JSON array order.
    // The genesis document itself does preserve declared order.
    let f = parse(GOLDEN);
    let base = genesis(&f, "json_min");
    let reordered = genesis(&f, "json_allocations_reordered");
    assert_eq!(base.allocation_root(), reordered.allocation_root());
    assert_ne!(base.hash(), reordered.hash());
    assert_eq!(base.allocation_root().to_hex(), f["allocation_root_1"]);
}

#[test]
fn shipped_genesis_config_matches_the_golden_hash() {
    let f = parse(GOLDEN);
    let genesis = from_json_str(SAMPLE_JSON).expect("shipped genesis config parses");
    assert_eq!(genesis.hash().to_hex(), f["genesis_1_hash"]);
    assert_eq!(
        genesis.validator_set_hash().to_hex(),
        f["validator_set_hash_1"]
    );
    assert_eq!(genesis.allocation_root().to_hex(), f["allocation_root_1"]);
}

#[test]
fn every_bad_json_document_is_rejected() {
    let f = parse(GOLDEN);
    let bad: Vec<&str> = f
        .keys()
        .filter(|key| key.starts_with("json_bad_"))
        .map(String::as_str)
        .collect();
    assert_eq!(bad.len(), 13);
    for key in bad {
        assert!(
            from_json_str(&json_doc(&f, key)).is_err(),
            "{key} must be rejected"
        );
    }
}

#[test]
fn bad_json_documents_return_the_specific_error() {
    let f = parse(GOLDEN);
    let error = |key: &str| from_json_str(&json_doc(&f, key)).unwrap_err();

    assert_eq!(
        error("json_bad_unknown_field"),
        GenesisError::UnknownField("network".to_owned())
    );
    assert_eq!(
        error("json_bad_wrong_chain"),
        GenesisError::WrongChain("auralite-1".to_owned())
    );
    assert_eq!(
        error("json_bad_version"),
        GenesisError::UnsupportedProtocolVersion(2)
    );
    assert!(matches!(
        error("json_bad_duplicate_validator"),
        GenesisError::DuplicateValidator(_)
    ));
    assert_eq!(
        error("json_bad_zero_power"),
        GenesisError::ZeroValidatorPower
    );
    assert_eq!(
        error("json_bad_float_balance"),
        GenesisError::WrongType("balance")
    );
    assert!(matches!(
        error("json_bad_address"),
        GenesisError::InvalidAddress(_)
    ));
    assert_eq!(
        error("json_bad_empty_validators"),
        GenesisError::EmptyValidatorSet
    );
    assert!(matches!(
        error("json_bad_pubkey"),
        GenesisError::InvalidPublicKey(_)
    ));
    assert_eq!(
        error("json_bad_supply_overflow"),
        GenesisError::SupplyOverflow
    );
    assert_eq!(
        error("json_bad_missing_field"),
        GenesisError::MissingField("chain_id")
    );
    assert_eq!(
        error("json_bad_unknown_validator_field"),
        GenesisError::UnknownNestedField("name".to_owned())
    );
    assert!(matches!(
        error("json_bad_duplicate_allocation"),
        GenesisError::DuplicateAllocation(_)
    ));
}

#[test]
fn binary_genesis_is_canonical() {
    let f = parse(GOLDEN);
    let bytes = hx(&f["genesis_1_canonical"]);
    assert_eq!(
        aura_genesis::Genesis::from_canonical_bytes(&bytes)
            .unwrap()
            .canonical_bytes(),
        bytes
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        aura_genesis::Genesis::from_canonical_bytes(&trailing),
        Err(GenesisError::Decode(CodecError::TrailingBytes(1)))
    );
    assert!(aura_genesis::Genesis::from_canonical_bytes(&bytes[..bytes.len() - 1]).is_err());
    // Non-minimal chain-id length varint (0x0f -> 0x80 0x00 is a different
    // value, so use a mutated first byte that keeps the length but adds a
    // continuation bit).
    let mut nonminimal = vec![0x8f, 0x00];
    nonminimal.extend_from_slice(&bytes[1..]);
    assert!(aura_genesis::Genesis::from_canonical_bytes(&nonminimal).is_err());
}

#[test]
fn summary_is_complete_and_deterministic() {
    let f = parse(GOLDEN);
    let genesis = genesis(&f, "json_min");
    let summary = genesis.summary();
    for key in [
        "chain_id",
        "protocol_version",
        "genesis_time_ms",
        "validators",
        "allocations",
        "total_supply",
        "genesis_hash",
        "validator_set_hash",
        "allocation_root",
    ] {
        assert!(summary.contains_key(key), "summary missing {key}");
    }
    assert_eq!(summary["genesis_hash"], f["genesis_1_hash"]);
    assert_eq!(summary, genesis.summary());
}
