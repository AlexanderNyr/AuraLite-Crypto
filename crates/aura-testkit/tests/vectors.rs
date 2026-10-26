//! C9 gate: 512 independent golden vectors for the canonical decoder.

use std::collections::BTreeMap;

use aura_codec::{CodecError, Reader, Writer, decode_canonical, read_varint, write_varint};

const GOLDEN: &str = include_str!("fixtures/c9_vectors.txt");

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

fn fixture_keys(f: &BTreeMap<String, String>, prefix: &str) -> Vec<String> {
    f.keys()
        .filter(|key| key.starts_with(prefix))
        .cloned()
        .collect()
}

#[test]
fn vector_inventory_is_512() {
    let f = parse(GOLDEN);
    assert_eq!(f["count"], "128");
    for prefix in ["varint_ok_", "varint_bad_", "trailing_", "truncated_"] {
        assert_eq!(fixture_keys(&f, prefix).len(), 128, "{prefix} count");
    }
    assert_eq!(fixture_keys(&f, "varint_ok_").len() * 4, 512);
}

#[test]
fn all_valid_varint_vectors_round_trip() {
    let f = parse(GOLDEN);
    for key in fixture_keys(&f, "varint_ok_") {
        let (value_hex, encoding_hex) = f[&key].split_once(':').expect("value:encoding");
        let expected = u64::from_str_radix(value_hex, 16).expect("u64 hex");
        let encoding = hx(encoding_hex);

        let mut writer = Writer::new();
        write_varint(&mut writer, expected);
        assert_eq!(to_hex(writer.as_slice()), encoding_hex, "{key} encoding");

        let mut reader = Reader::new(&encoding);
        assert_eq!(read_varint(&mut reader).unwrap(), expected, "{key} decode");
        assert_eq!(reader.remaining_len(), 0, "{key} fully consumed");
    }
}

#[test]
fn all_bad_varint_vectors_are_rejected() {
    let f = parse(GOLDEN);
    for key in fixture_keys(&f, "varint_bad_") {
        let bytes = hx(&f[&key]);
        let mut reader = Reader::new(&bytes);
        assert!(read_varint(&mut reader).is_err(), "{key} must be rejected");
    }
}

#[test]
fn all_trailing_byte_vectors_are_rejected() {
    let f = parse(GOLDEN);
    for key in fixture_keys(&f, "trailing_") {
        let bytes = hx(&f[&key]);
        assert_eq!(
            decode_canonical::<Vec<u8>>(&bytes),
            Err(CodecError::TrailingBytes(bytes.len() - 4)),
            "{key}"
        );
    }
}

#[test]
fn all_truncated_vectors_are_rejected() {
    let f = parse(GOLDEN);
    for key in fixture_keys(&f, "truncated_") {
        let bytes = hx(&f[&key]);
        assert!(
            decode_canonical::<Vec<u8>>(&bytes).is_err(),
            "{key} must be rejected"
        );
    }
}
