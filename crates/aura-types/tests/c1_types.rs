//! C1 gate: canonical types against independent golden vectors.
//!
//! The fixtures in `fixtures/c1_golden.txt` are produced by
//! `tools/gen_c1_vectors.py`, a standalone BIP-173 implementation that does
//! not use this crate. If someone "fixes" the Rust bech32 encoder to match
//! itself, these tests go red.

use std::collections::BTreeMap;

use aura_types::{
    ADDRESS_LEN, Address, AddressError, Amount, BECH32_PREFIX, CHAIN_ID, CanonicalMap, ChainId,
    DENOM, Denom, Domain, Gas, HASH_LEN, Hash, HashError, Height, Nonce, PrimitiveError,
    SMALLEST_UNIT, units_per_coin,
};

const GOLDEN: &str = include_str!("fixtures/c1_golden.txt");

fn fixtures() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in GOLDEN.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .unwrap_or_else(|| panic!("malformed golden vector line: {trimmed}"));
        assert!(
            map.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate key {key}"
        );
    }
    map
}

fn bytes20(hex: &str) -> [u8; ADDRESS_LEN] {
    let raw = hex_to_bytes(hex);
    raw.try_into().expect("expected 20-byte address fixture")
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "even-length hex");
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let hi = hex_nibble(pair[0]);
            let lo = hex_nibble(pair[1]);
            (hi << 4) | lo
        })
        .collect()
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => panic!("invalid hex nibble in fixture"),
    }
}

#[test]
fn golden_hash_vectors_round_trip() {
    let f = fixtures();
    for key in ["hash_zero", "hash_asc", "hash_desc", "hash_pattern"] {
        let expected = f.get(key).expect("fixture key present");
        let hash = Hash::from_hex(expected).expect("valid 64-char hex");
        assert_eq!(hash.to_hex(), *expected, "{key} hex round-trip");
        assert_eq!(hash.to_string(), *expected, "{key} Display");
        assert_eq!(
            Hash::from_slice(hash.as_bytes()).unwrap(),
            hash,
            "{key} slice round-trip"
        );
    }
    assert_eq!(Hash::ZERO.to_hex(), f["hash_zero"]);
    assert_eq!(Hash::from_hex(&f["hash_zero"]).unwrap(), Hash::ZERO);
    assert_eq!(HASH_LEN, 32);
}

#[test]
fn golden_address_vectors_match_independent_reference() {
    let f = fixtures();
    for name in ["address_1", "address_2", "address_3", "address_4"] {
        let raw = bytes20(&f[&format!("{name}_bytes")]);
        let expected = &f[&format!("{name}_bech32")];

        let address = Address::from_bytes(raw);
        assert_eq!(
            address.to_bech32(),
            *expected,
            "{name} encode vs Python BIP-173"
        );
        assert_eq!(address.to_string(), *expected, "{name} Display");

        let decoded = Address::from_bech32(expected).expect("golden address must decode");
        assert_eq!(decoded, address, "{name} decode round-trip");
        assert_eq!(decoded.as_bytes(), &raw, "{name} raw bytes");
        assert!(
            expected.starts_with(&format!("{BECH32_PREFIX}1")),
            "{name} prefix"
        );
    }
}

#[test]
fn bad_checksum_vectors_are_rejected() {
    let f = fixtures();
    for key in ["bad_checksum_1", "bad_checksum_2"] {
        let result = Address::from_bech32(&f[key]);
        assert_eq!(result, Err(AddressError::InvalidChecksum), "{key}");
    }
}

#[test]
fn wrong_prefix_and_uppercase_are_rejected() {
    let f = fixtures();
    assert_eq!(
        Address::from_bech32(&f["wrong_prefix"]),
        Err(AddressError::InvalidPrefix)
    );
    assert_eq!(
        Address::from_bech32(&f["uppercase"]),
        Err(AddressError::MixedCase)
    );
    assert_eq!(
        Address::from_bech32("not_an_address"),
        Err(AddressError::MissingSeparator)
    );
}

#[test]
fn address_and_hash_reject_malformed_input() {
    assert_eq!(
        Address::from_slice(&[0u8; 19]),
        Err(AddressError::InvalidByteLength(19))
    );
    assert_eq!(
        Hash::from_slice(&[0u8; 31]),
        Err(HashError::InvalidByteLength(31))
    );
    assert_eq!(Hash::from_hex("zz"), Err(HashError::InvalidHexLength(2)));
    assert_eq!(
        Hash::from_hex(&format!("{}z0", "ab".repeat(31))),
        Err(HashError::InvalidHexCharacter)
    );
    assert_eq!(
        Address::from_bech32("aura1!!!!!!"),
        Err(AddressError::InvalidCharacter)
    );
    assert_eq!(
        Address::from_bech32("aura1qqqq"),
        Err(AddressError::TooShort)
    );
}

#[test]
fn amount_arithmetic_is_checked() {
    let f = fixtures();
    assert_eq!(Amount::MAX.units().to_string(), f["amount_max"]);
    assert_eq!(units_per_coin().to_string(), f["amount_units_per_coin"]);

    assert_eq!(
        Amount::MAX.checked_add(Amount::ONE),
        None,
        "u128 overflow must not wrap"
    );
    assert_eq!(
        Amount::ZERO.checked_sub(Amount::ONE),
        None,
        "underflow must not wrap"
    );
    assert_eq!(Amount::MAX.checked_mul(2), None);
    assert_eq!(Amount::ONE.checked_div(0), None);
    assert_eq!(
        Amount::from_coins(u128::MAX),
        None,
        "coins conversion must be checked"
    );
    assert_eq!(
        Amount::from_coins(1),
        Some(Amount::from_units(units_per_coin()))
    );
    assert_eq!(Amount::from_coins(0), Some(Amount::ZERO));
    assert!(Amount::from_units(42).to_string().contains(SMALLEST_UNIT));
}

#[test]
fn denom_accepts_only_native_names() {
    assert_eq!(Denom::parse(DENOM), Some(Denom::NATIVE));
    assert_eq!(Denom::parse(SMALLEST_UNIT), Some(Denom::SMALLEST));
    assert_eq!(Denom::parse("USDT"), None);
    assert_eq!(Denom::NATIVE.as_str(), DENOM);
    assert_eq!(Denom::SMALLEST.as_str(), SMALLEST_UNIT);
}

#[test]
fn scalar_overflows_are_errors() {
    assert_eq!(
        Nonce::new(u64::MAX).checked_next(),
        Err(PrimitiveError::NonceOverflow)
    );
    assert_eq!(
        Height::new(u64::MAX).checked_next(),
        Err(PrimitiveError::HeightOverflow)
    );
    assert_eq!(
        Height::new(0).checked_sub(1),
        Err(PrimitiveError::HeightUnderflow)
    );
    assert_eq!(
        Gas::new(u64::MAX).checked_add(1),
        Err(PrimitiveError::GasOverflow)
    );
    assert_eq!(
        Gas::new(0).checked_sub(1),
        Err(PrimitiveError::GasUnderflow)
    );
    assert_eq!(
        Gas::new(u64::MAX).checked_mul(2),
        Err(PrimitiveError::GasOverflow)
    );

    assert_eq!(Nonce::new(7).checked_next().unwrap().value(), 8);
    assert_eq!(Height::new(9).checked_sub(4).unwrap().value(), 5);
    assert_eq!(Gas::new(3).checked_mul(4).unwrap().value(), 12);
}

#[test]
fn chain_id_validation_is_strict() {
    let parsed = ChainId::new(CHAIN_ID).expect("locked chain-id must be valid");
    assert_eq!(parsed.as_str(), CHAIN_ID);
    assert_eq!(ChainId::stage1(), parsed);

    assert_eq!(ChainId::new("ab"), Err(PrimitiveError::ChainIdLength(2)));
    assert_eq!(
        ChainId::new("AuraLite-1"),
        Err(PrimitiveError::ChainIdCharset)
    );
    assert_eq!(
        ChainId::new("auralite_test_1"),
        Err(PrimitiveError::ChainIdCharset)
    );
    assert_eq!(ChainId::new("-auralite"), Err(PrimitiveError::ChainIdEdge));
    assert_eq!(ChainId::new("auralite-"), Err(PrimitiveError::ChainIdEdge));
    assert_eq!(
        ChainId::new(&"a".repeat(65)),
        Err(PrimitiveError::ChainIdLength(65))
    );
}

#[test]
fn domain_separation_is_stable_and_distinct() {
    assert_eq!(Domain::ALL.len(), 5);
    for domain in Domain::ALL {
        assert_eq!(
            domain.prefix(CHAIN_ID),
            domain.prefix(CHAIN_ID),
            "stable prefix"
        );
        assert_ne!(
            domain.prefix(CHAIN_ID),
            domain.prefix("auralite-1"),
            "chain-id separated"
        );
    }
    let tx = Domain::Tx.prefix(CHAIN_ID);
    let vote = Domain::Vote.prefix(CHAIN_ID);
    assert_ne!(tx, vote, "different domains must differ");
    assert!(tx.starts_with(CHAIN_ID.as_bytes()));
    assert!(tx.ends_with(&[b't', b'x', 0]));
}

#[test]
fn canonical_map_iteration_is_deterministic() {
    let mut map: CanonicalMap<u8, &str> = CanonicalMap::new();
    for (key, value) in [(3, "c"), (1, "a"), (2, "b"), (1, "a")] {
        map.insert(key, value);
    }
    let first: Vec<_> = map.iter().map(|(k, v)| (*k, *v)).collect();
    let second: Vec<_> = map.iter().map(|(k, v)| (*k, *v)).collect();
    assert_eq!(first, second, "iteration order must be reproducible");
    assert_eq!(first, vec![(1, "a"), (2, "b"), (3, "c")]);
}
