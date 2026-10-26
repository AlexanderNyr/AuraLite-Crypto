//! C3 gate: crypto primitives against independent references.
//!
//! Sources: official BLAKE3 test vectors, RFC 8032 §7.1, and a pure-Python
//! secp256k1/RFC6979 implementation (verified with OpenSSL). None of these
//! fixtures were produced by the Rust code under test.

use std::collections::BTreeMap;

use aura_crypto::{
    HASH_CONTEXT, SecretBytes, blake3_hash, constant_time_eq, domain_message, ed25519_public_key,
    hash_domain, secp_public_key, sign, sign_prehash_raw, sign_raw, verify, verify_prehash_raw,
    verify_raw,
};
use aura_types::{CHAIN_ID, Domain};

const GOLDEN: &str = include_str!("fixtures/c3_golden.txt");

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

fn arr32(text: &str) -> [u8; 32] {
    hx(text).try_into().expect("expected 32 bytes")
}

fn arr64(text: &str) -> [u8; 64] {
    hx(text).try_into().expect("expected 64 bytes")
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn tag_to_domain(tag: &str) -> Domain {
    match tag {
        "tx" => Domain::Tx,
        "vote" => Domain::Vote,
        "block" => Domain::Block,
        "contract" => Domain::Contract,
        "genesis" => Domain::Genesis,
        _ => panic!("unknown domain tag {tag}"),
    }
}

#[test]
fn blake3_matches_official_vectors() {
    let f = fixtures();
    // Input pattern from BLAKE3-team/BLAKE3 test_vectors.json.
    for key in f.keys().filter(|k| k.starts_with("b3_official_")) {
        let length: usize = key
            .strip_prefix("b3_official_")
            .unwrap()
            .parse()
            .expect("numeric length");
        let input: Vec<u8> = (0..length).map(|i| (i % 251) as u8).collect();
        assert_eq!(blake3_hash(&input).to_hex(), f[key], "{key}");
    }
    assert_eq!(blake3_hash(b"").to_hex(), f["b3_official_0"]);
}

#[test]
fn hash_context_is_pinned() {
    let f = fixtures();
    assert_eq!(to_hex(HASH_CONTEXT), f["hash_context"]);
}

#[test]
fn domain_messages_and_hashes_match_independent_reference() {
    let f = fixtures();
    for index in 1..=5 {
        let chain = &f[&format!("domain_{index}_chain")];
        let tag = &f[&format!("domain_{index}_tag")];
        let payload = hx(&f[&format!("domain_{index}_payload")]);
        let domain = tag_to_domain(tag);

        let message = domain_message(chain, domain, &payload);
        assert_eq!(
            to_hex(&message),
            f[&format!("domain_{index}_message")],
            "domain_{index} framing"
        );
        assert_eq!(
            hash_domain(chain, domain, &payload).to_hex(),
            f[&format!("domain_{index}_hash")],
            "domain_{index} BLAKE3"
        );
    }
}

#[test]
fn domain_separation_changes_the_hash() {
    let payload = b"same payload";
    let tx = hash_domain(CHAIN_ID, Domain::Tx, payload);
    let vote = hash_domain(CHAIN_ID, Domain::Vote, payload);
    let other_chain = hash_domain("auralite-1", Domain::Tx, payload);
    let other_payload = hash_domain(CHAIN_ID, Domain::Tx, b"same payload!");

    assert_ne!(tx, vote, "domain tag must change the hash");
    assert_ne!(tx, other_chain, "chain-id must change the hash");
    assert_ne!(tx, other_payload, "payload must change the hash");
    assert_eq!(
        tx,
        hash_domain(CHAIN_ID, Domain::Tx, payload),
        "deterministic"
    );
}

#[test]
fn rfc8032_ed25519_vectors_match() {
    let f = fixtures();
    for index in 1..=3 {
        let seed = arr32(&f[&format!("ed{index}_secret")]);
        let expected_public = arr32(&f[&format!("ed{index}_public")]);
        let message = hx(&f[&format!("ed{index}_message")]);
        let expected_signature = arr64(&f[&format!("ed{index}_signature")]);

        assert_eq!(
            ed25519_public_key(&seed),
            expected_public,
            "ed{index} public"
        );
        let signature = sign_raw(&seed, &message);
        assert_eq!(signature, expected_signature, "ed{index} signature");
        assert!(
            verify_raw(&expected_public, &message, &expected_signature).is_ok(),
            "ed{index} verify"
        );
    }
}

#[test]
fn ed25519_rejects_tampering() {
    // Pins NC-4 (verification bypass).
    let f = fixtures();
    let seed = arr32(&f["ed2_secret"]);
    let public = ed25519_public_key(&seed);
    let message = hx(&f["ed2_message"]);
    let mut signature = sign_raw(&seed, &message);

    assert!(verify_raw(&public, &message, &signature).is_ok());
    signature[0] ^= 0x01;
    assert!(
        verify_raw(&public, &message, &signature).is_err(),
        "tampered sig"
    );
    assert!(
        verify_raw(&public, b"different message", &signature).is_err(),
        "tampered message"
    );
}

#[test]
fn domain_signed_messages_verify_only_in_their_domain() {
    let seed = [0x42u8; 32];
    let public = ed25519_public_key(&seed);
    let payload = b"transfer 10 ALCRY";
    let signature = sign(&seed, CHAIN_ID, Domain::Tx, payload);

    assert!(verify(&public, CHAIN_ID, Domain::Tx, payload, &signature).is_ok());
    assert!(verify(&public, CHAIN_ID, Domain::Vote, payload, &signature).is_err());
    assert!(verify(&public, "auralite-1", Domain::Tx, payload, &signature).is_err());
    assert!(
        verify(
            &public,
            CHAIN_ID,
            Domain::Tx,
            b"transfer 11 ALCRY",
            &signature
        )
        .is_err()
    );

    let mut tampered = signature;
    tampered[63] ^= 0x80;
    assert!(verify(&public, CHAIN_ID, Domain::Tx, payload, &tampered).is_err());
}

#[test]
fn secp256k1_rfc6979_vectors_match_independent_reference() {
    let f = fixtures();
    for index in 1..=2 {
        let seed = arr32(&f[&format!("secp{index}_secret")]);
        let expected_public = hx(&f[&format!("secp{index}_public")]);
        let prehash = arr32(&f[&format!("secp{index}_prehash")]);
        let expected_signature = arr64(&f[&format!("secp{index}_signature")]);

        assert_eq!(
            secp_public_key(&seed).unwrap(),
            expected_public,
            "secp{index} public"
        );
        let signature = sign_prehash_raw(&seed, &prehash).unwrap();
        assert_eq!(
            signature, expected_signature,
            "secp{index} deterministic signature"
        );
        assert!(
            verify_prehash_raw(&expected_public, &prehash, &expected_signature).is_ok(),
            "secp{index} verify"
        );
    }
}

#[test]
fn secp256k1_rejects_tampering_and_bad_keys() {
    let f = fixtures();
    let seed = arr32(&f["secp1_secret"]);
    let public = secp_public_key(&seed).unwrap();
    let prehash = arr32(&f["secp1_prehash"]);
    let mut signature = sign_prehash_raw(&seed, &prehash).unwrap();

    assert!(verify_prehash_raw(&public, &prehash, &signature).is_ok());
    signature[10] ^= 0x01;
    assert!(verify_prehash_raw(&public, &prehash, &signature).is_err());

    let mut other_prehash = prehash;
    other_prehash[0] ^= 0x01;
    assert!(verify_prehash_raw(&public, &other_prehash, &signature).is_err());

    assert_eq!(
        secp_public_key(&[0u8; 32]).err(),
        Some(aura_crypto::CryptoError::InvalidSecpSecret)
    );
    assert!(verify_prehash_raw(&[0u8; 33], &prehash, &signature).is_err());
}

#[test]
fn constant_time_eq_is_correct() {
    assert!(constant_time_eq(b"aura", b"aura"));
    assert!(!constant_time_eq(b"aura", b"aurb"));
    assert!(!constant_time_eq(b"aura", b"aur"));
    assert!(constant_time_eq(b"", b""));
    assert!(!constant_time_eq(b"", b"x"));
}

#[test]
fn secret_bytes_are_redacted_and_length_checked() {
    let secret = SecretBytes::from_slice(&[0xab; 32]).unwrap();
    assert_eq!(secret.as_bytes(), &[0xab; 32]);

    let debug = format!("{secret:?}");
    assert_eq!(debug, "SecretBytes([REDACTED])");
    assert!(!debug.contains("abab"), "secret must not appear in Debug");

    assert_eq!(
        SecretBytes::from_slice(&[0u8; 31]).err(),
        Some(aura_crypto::CryptoError::InvalidSecretLength(31))
    );
    assert_eq!(
        SecretBytes::from_slice(&[0u8; 33]).err(),
        Some(aura_crypto::CryptoError::InvalidSecretLength(33))
    );
}
