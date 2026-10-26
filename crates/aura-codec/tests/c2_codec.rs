//! C2 gate: canonical serialization against an independent encoder.
//!
//! `fixtures/c2_golden.txt` comes from `tools/gen_c2_vectors.py`, a separate
//! Python transcription of the wire format. These tests pin the exact bytes,
//! not just Rust round-trips.

use std::collections::BTreeMap;
use std::fmt::Debug;

use aura_codec::{
    CODEC_VERSION, CanonicalRecord, CodecError, Decode, Encode, MAX_ITEMS, MAX_LENGTH, Reader,
    Writer, decode_canonical, decode_from_slice, encode_to_bytes, read_varint, write_varint,
};
use aura_types::{Address, Amount, CanonicalMap, ChainId, Denom, Domain, Gas, Hash, Height, Nonce};

const GOLDEN: &str = include_str!("fixtures/c2_golden.txt");

fn fixtures() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in GOLDEN.lines() {
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

fn hex_bytes(text: &str) -> Vec<u8> {
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
        _ => panic!("invalid hex nibble in fixture"),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn ascending_32() -> [u8; 32] {
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = index as u8;
    }
    out
}

fn descending_32() -> [u8; 32] {
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = 255 - index as u8;
    }
    out
}

fn ascending_20() -> [u8; 20] {
    let mut out = [0u8; 20];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = index as u8;
    }
    out
}

fn record_1() -> CanonicalRecord {
    CanonicalRecord {
        version: CODEC_VERSION,
        domain: Domain::Tx,
        flag: false,
        chain: ChainId::stage1(),
        height: Height::new(0),
        nonce: Nonce::new(0),
        amount: Amount::ZERO,
        hash: Hash::ZERO,
        address: Address::ZERO,
        memo: None,
        tags: CanonicalMap::new(),
    }
}

fn record_2() -> CanonicalRecord {
    let mut tags = CanonicalMap::new();
    tags.insert("a".to_owned(), 1);
    tags.insert("b".to_owned(), 2);
    tags.insert("z".to_owned(), 9);
    CanonicalRecord {
        version: CODEC_VERSION,
        domain: Domain::Vote,
        flag: true,
        chain: ChainId::stage1(),
        height: Height::new(42),
        nonce: Nonce::new(7),
        amount: Amount::from_units(10u128.pow(18)),
        hash: Hash::from_bytes(ascending_32()),
        address: Address::from_bytes(ascending_20()),
        memo: Some("hello".to_owned()),
        tags,
    }
}

fn record_3() -> CanonicalRecord {
    let mut tags = CanonicalMap::new();
    tags.insert("alpha".to_owned(), 0);
    tags.insert("omega".to_owned(), u64::MAX);
    CanonicalRecord {
        version: CODEC_VERSION,
        domain: Domain::Block,
        flag: false,
        chain: ChainId::stage1(),
        height: Height::new(u64::MAX),
        nonce: Nonce::new(u64::MAX),
        amount: Amount::MAX,
        hash: Hash::from_bytes(descending_32()),
        address: Address::from_bytes([0xab; 20]),
        memo: Some("aura-\u{2713}".to_owned()),
        tags,
    }
}

fn golden_records() -> Vec<(&'static str, CanonicalRecord)> {
    vec![
        ("record_1", record_1()),
        ("record_2", record_2()),
        ("record_3", record_3()),
    ]
}

#[test]
fn golden_records_match_independent_encoder() {
    let f = fixtures();
    for (key, record) in golden_records() {
        let encoded = encode_to_bytes(&record);
        assert_eq!(to_hex(&encoded), f[key], "{key} bytes must match Python");
    }
}

#[test]
fn decode_canonical_round_trips_golden_records() {
    let f = fixtures();
    for (key, record) in golden_records() {
        let bytes = hex_bytes(&f[key]);
        let decoded: CanonicalRecord =
            decode_canonical(&bytes).unwrap_or_else(|e| panic!("{key} failed: {e}"));
        assert_eq!(decoded, record, "{key} round-trip");
        assert_eq!(encode_to_bytes(&decoded), bytes, "{key} re-encode");
    }
}

#[test]
fn every_noncanonical_vector_is_rejected() {
    let f = fixtures();
    const BAD: [&str; 12] = [
        "bad_version",
        "bad_domain_tag",
        "bad_bool",
        "bad_option_tag",
        "trailing_byte",
        "nonminimal_chain_len",
        "length_too_large",
        "invalid_utf8_memo",
        "unordered_map",
        "duplicate_map_key",
        "nonminimal_map_count",
        "varint_overflow",
    ];
    for key in BAD {
        let bytes = hex_bytes(&f[key]);
        assert!(
            decode_canonical::<CanonicalRecord>(&bytes).is_err(),
            "{key} must be rejected"
        );
    }
}

#[test]
fn noncanonical_vectors_return_the_specific_error() {
    let f = fixtures();
    let check = |key: &str, expected: CodecError| {
        let bytes = hex_bytes(&f[key]);
        assert_eq!(
            decode_from_slice::<CanonicalRecord>(&bytes),
            Err(expected),
            "{key} error kind"
        );
    };
    check("bad_version", CodecError::UnsupportedVersion(1, 2));
    check("bad_domain_tag", CodecError::InvalidDomainTag(5));
    check("bad_bool", CodecError::InvalidBool(2));
    check("bad_option_tag", CodecError::InvalidOptionTag(2));
    check("invalid_utf8_memo", CodecError::InvalidUtf8);
    check("length_too_large", CodecError::LengthTooLarge(1048577));
    check("unordered_map", CodecError::NonCanonicalMapOrder);
    check("duplicate_map_key", CodecError::DuplicateMapKey);
    check("nonminimal_chain_len", CodecError::NonCanonicalVarint);
    check("nonminimal_map_count", CodecError::NonCanonicalVarint);
    check("varint_overflow", CodecError::VarintOverflow);
    check("trailing_byte", CodecError::TrailingBytes(1));
}

#[test]
fn trailing_byte_rejected_by_reader_finish() {
    // This test pins Reader::finish() — NC-3 disables that guard.
    let f = fixtures();
    let bytes = hex_bytes(&f["trailing_byte"]);
    let mut reader = Reader::new(&bytes);
    let _record = CanonicalRecord::decode(&mut reader).expect("record itself decodes");
    assert_eq!(reader.finish(), Err(CodecError::TrailingBytes(1)));
}

#[test]
fn varint_rejects_nonminimal_encoding() {
    // This test pins varint_is_nonminimal() — NC-4 disables that guard.
    let mut reader = Reader::new(&[0x80, 0x00]);
    assert_eq!(
        read_varint(&mut reader),
        Err(CodecError::NonCanonicalVarint)
    );
}

#[test]
fn varint_round_trips_boundaries_with_minimal_length() {
    let mut writer = Writer::new();
    write_varint(&mut writer, 0);
    assert_eq!(writer.as_slice(), &[0x00u8][..], "zero must be one byte");

    for value in [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX] {
        let mut writer = Writer::new();
        write_varint(&mut writer, value);
        let bytes = writer.into_bytes();
        let mut reader = Reader::new(&bytes);
        assert_eq!(read_varint(&mut reader).unwrap(), value, "varint {value}");
        assert_eq!(reader.remaining_len(), 0);
    }
}

fn roundtrip<T>(value: T)
where
    T: Encode + Decode + PartialEq + Debug,
{
    let bytes = encode_to_bytes(&value);
    assert_eq!(bytes, encode_to_bytes(&value), "encoding is deterministic");
    let decoded: T = decode_from_slice(&bytes).expect("strict decode");
    assert_eq!(decoded, value);
    let canonical: T = decode_canonical(&bytes).expect("canonical decode");
    assert_eq!(canonical, value);
}

#[test]
fn all_supported_types_round_trip() {
    roundtrip(0u8);
    roundtrip(255u8);
    roundtrip(65_535u16);
    roundtrip(u32::MAX);
    roundtrip(u64::MAX);
    roundtrip(u128::MAX);
    roundtrip(true);
    roundtrip(false);
    roundtrip(Some(1_234u64));
    roundtrip(None::<u64>);
    roundtrip(vec![0u8, 1, 2, 255]);
    roundtrip("aura-\u{2713}".to_owned());
    roundtrip(Hash::from_bytes(ascending_32()));
    roundtrip(Address::from_bytes(ascending_20()));
    roundtrip(Amount::MAX);
    roundtrip(ChainId::stage1());
    roundtrip(Domain::Contract);
    roundtrip(Denom::SMALLEST);
    roundtrip(Nonce::new(9));
    roundtrip(Height::new(10));
    roundtrip(Gas::new(11));

    let mut map = CanonicalMap::new();
    map.insert("b".to_owned(), 2u64);
    map.insert("a".to_owned(), 1u64);
    roundtrip(map);
}

#[test]
fn denom_tags_are_strict() {
    assert_eq!(decode_from_slice::<Denom>(&[0]).unwrap(), Denom::NATIVE);
    assert_eq!(decode_from_slice::<Denom>(&[1]).unwrap(), Denom::SMALLEST);
    assert_eq!(
        decode_from_slice::<Denom>(&[2]),
        Err(CodecError::InvalidDenomTag(2))
    );
}

#[test]
fn map_encoding_is_sorted_and_decoding_rejects_disorder() {
    let mut map = CanonicalMap::new();
    map.insert("z".to_owned(), 3u64);
    map.insert("a".to_owned(), 1u64);
    map.insert("m".to_owned(), 2u64);
    let bytes = encode_to_bytes(&map);
    let decoded: CanonicalMap<String, u64> = decode_canonical(&bytes).unwrap();
    let keys: Vec<_> = decoded.keys().cloned().collect();
    assert_eq!(keys, vec!["a", "m", "z"]);

    let f = fixtures();
    let unordered = hex_bytes(&f["unordered_map"]);
    assert_eq!(
        decode_from_slice::<CanonicalRecord>(&unordered),
        Err(CodecError::NonCanonicalMapOrder)
    );
}

#[test]
fn codec_constants_match_the_independent_fixture() {
    let f = fixtures();
    assert_eq!(CODEC_VERSION.to_string(), f["codec_version"]);
    assert_eq!(MAX_LENGTH.to_string(), f["max_length"]);
    assert_eq!(MAX_ITEMS, 65536);
    assert_eq!(MAX_LENGTH, 1 << 20);
}
