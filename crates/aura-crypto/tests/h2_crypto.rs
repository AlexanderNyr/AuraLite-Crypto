//! H2 gate: 50 golden crypto vectors, byte-for-byte, plus behavioral negatives.
//!
//! Fixture: `tools/gen_h2_vectors.py` (independent oracle: pure-Python RFC
//! 8032 from `tools/gen_c4_vectors.py`, the python `blake3` binding, and a
//! from-spec `domain_message` assembler). Nothing here was produced by the
//! Rust code under review.
//!
//! Families (docs/h2-crypto-review-plan.md §2): sign_verify, keygen,
//! wrong_domain, cross_chain, tampered_payload, wrong_key, high_s, digest,
//! edge. NC targets (`tools/nc_h2.py`):
//!   NC-1 (named) collapse `Domain::tag()` -> `H2: wrong-domain signature must be rejected`,
//!   NC-2 remove the canonical-S guard -> `H2: high-S signature must be rejected`,
//!   NC-3 drop the chain binding -> `H2: cross-chain digest must be chain-bound`.

use std::collections::BTreeMap;

use aura_crypto::{CryptoError, ed25519_public_key, hash_domain, sign_raw, verify, verify_raw};
use aura_types::Domain;

const H2: &str = include_str!("fixtures/h2_vectors.txt");

/// Parse the block-format fixture: `vector=<label>` starts a block; every
/// `key=value` line belongs to the current block. Header lines before the
/// first block are ignored here (see `h2_vector_count_is_50`).
fn vectors() -> Vec<BTreeMap<String, String>> {
    let mut out: Vec<BTreeMap<String, String>> = Vec::new();
    let mut current: Option<BTreeMap<String, String>> = None;
    for line in H2.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "vector" {
            if let Some(map) = current.take() {
                out.push(map);
            }
            let mut map = BTreeMap::new();
            map.insert(key.to_owned(), value.to_owned());
            current = Some(map);
        } else if let Some(map) = current.as_mut() {
            assert!(
                map.insert(key.to_owned(), value.to_owned()).is_none(),
                "duplicate fixture key {key}"
            );
        }
    }
    if let Some(map) = current.take() {
        out.push(map);
    }
    out
}

fn get<'a>(vec: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    vec.get(key)
        .unwrap_or_else(|| panic!("missing fixture field {key} in {:?}", vec.get("vector")))
}

fn kind(vec: &BTreeMap<String, String>) -> &str {
    get(vec, "kind")
}

fn domain_of(tag: &str) -> Domain {
    match tag {
        "tx" => Domain::Tx,
        "vote" => Domain::Vote,
        "block" => Domain::Block,
        "contract" => Domain::Contract,
        "genesis" => Domain::Genesis,
        _ => panic!("unknown domain tag in fixture: {tag}"),
    }
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

fn arr32(text: &str) -> [u8; 32] {
    hx(text).try_into().expect("expected 32 bytes")
}

fn arr64(text: &str) -> [u8; 64] {
    hx(text).try_into().expect("expected 64 bytes")
}

#[test]
fn h2_vector_count_is_50() {
    assert!(H2.contains("vector_count=50"), "fixture header pin");
    let vecs = vectors();
    assert_eq!(vecs.len(), 50, "H2: exactly 50 vectors expected");
    for kind_name in [
        "sign_verify",
        "keygen",
        "wrong_domain",
        "cross_chain",
        "tampered_payload",
        "wrong_key",
        "high_s",
        "digest",
        "edge",
    ] {
        assert!(
            vecs.iter().any(|v| kind(v) == kind_name),
            "fixture must contain {kind_name} vectors"
        );
    }
}

#[test]
fn h2_keygen_matches_oracle() {
    for vec in vectors().iter().filter(|v| kind(v) == "keygen") {
        let seed = arr32(get(vec, "seed"));
        assert_eq!(
            ed25519_public_key(&seed),
            arr32(get(vec, "pubkey")),
            "keygen mismatch for {:?}",
            vec.get("vector")
        );
    }
}

#[test]
fn h2_sign_verify_byte_for_byte() {
    let positives = ["sign_verify", "edge"];
    for vec in vectors().iter().filter(|v| positives.contains(&kind(v))) {
        let label = vec.get("vector").cloned().unwrap_or_default();
        let seed = arr32(get(vec, "seed"));
        let payload = hx(get(vec, "payload_hex"));
        let digest = arr32(get(vec, "digest"));
        let signature = arr64(get(vec, "sig"));
        let public = arr32(get(vec, "pubkey"));
        assert_eq!(ed25519_public_key(&seed), public, "pubkey {label}");
        assert_eq!(
            hash_domain(get(vec, "chain"), domain_of(get(vec, "domain")), &payload).to_hex(),
            get(vec, "digest"),
            "digest {label}"
        );
        assert_eq!(sign_raw(&seed, &digest), signature, "signature {label}");
        assert!(
            verify(
                &public,
                get(vec, "chain"),
                domain_of(get(vec, "domain")),
                &payload,
                &signature
            )
            .is_ok(),
            "verify must accept {label}"
        );
    }
}

#[test]
fn h2_wrong_domain_rejected() {
    for vec in vectors().iter().filter(|v| kind(v) == "wrong_domain") {
        let seed = arr32(get(vec, "seed"));
        let payload = hx(get(vec, "payload_hex"));
        let signature = arr64(get(vec, "sig"));
        let public = arr32(get(vec, "pubkey"));
        assert_eq!(ed25519_public_key(&seed), public);
        let chain = get(vec, "chain");
        assert!(
            verify(
                &public,
                chain,
                domain_of(get(vec, "domain")),
                &payload,
                &signature
            )
            .is_ok()
        );
        assert!(
            verify(
                &public,
                chain,
                domain_of(get(vec, "other_domain")),
                &payload,
                &signature
            )
            .is_err(),
            "H2: wrong-domain signature must be rejected"
        );
    }
}

#[test]
fn h2_cross_chain_rejected() {
    for vec in vectors().iter().filter(|v| kind(v) == "cross_chain") {
        let seed = arr32(get(vec, "seed"));
        let payload = hx(get(vec, "payload_hex"));
        let signature = arr64(get(vec, "sig"));
        let public = arr32(get(vec, "pubkey"));
        assert_eq!(ed25519_public_key(&seed), public);
        let domain = domain_of(get(vec, "domain"));
        assert_ne!(
            hash_domain(get(vec, "other_chain"), domain, &payload),
            hash_domain(get(vec, "chain"), domain, &payload),
            "H2: cross-chain digest must be chain-bound"
        );
        assert!(verify(&public, get(vec, "chain"), domain, &payload, &signature).is_ok());
        assert!(
            verify(
                &public,
                get(vec, "other_chain"),
                domain,
                &payload,
                &signature
            )
            .is_err(),
            "H2: cross-chain signature must be rejected"
        );
    }
}

#[test]
fn h2_tampered_payload_rejected() {
    for vec in vectors().iter().filter(|v| kind(v) == "tampered_payload") {
        let seed = arr32(get(vec, "seed"));
        let payload = hx(get(vec, "payload_hex"));
        let other = hx(get(vec, "other_payload_hex"));
        let signature = arr64(get(vec, "sig"));
        let public = arr32(get(vec, "pubkey"));
        assert_eq!(ed25519_public_key(&seed), public);
        let chain = get(vec, "chain");
        let domain = domain_of(get(vec, "domain"));
        assert!(verify(&public, chain, domain, &payload, &signature).is_ok());
        assert!(
            verify(&public, chain, domain, &other, &signature).is_err(),
            "H2: tampered payload signature must be rejected"
        );
    }
}

#[test]
fn h2_wrong_key_rejected() {
    for vec in vectors().iter().filter(|v| kind(v) == "wrong_key") {
        let payload = hx(get(vec, "payload_hex"));
        let signature = arr64(get(vec, "sig"));
        let public = arr32(get(vec, "pubkey"));
        let other = arr32(get(vec, "other_pubkey"));
        let chain = get(vec, "chain");
        let domain = domain_of(get(vec, "domain"));
        assert!(verify(&public, chain, domain, &payload, &signature).is_ok());
        assert!(
            verify(&other, chain, domain, &payload, &signature).is_err(),
            "H2: wrong-key signature must be rejected"
        );
    }
}

#[test]
fn h2_high_s_malleability_rejected() {
    for vec in vectors().iter().filter(|v| kind(v) == "high_s") {
        let seed = arr32(get(vec, "seed"));
        let payload = hx(get(vec, "payload_hex"));
        let digest = arr32(get(vec, "digest"));
        let signature = arr64(get(vec, "sig"));
        let twin = arr64(get(vec, "sig_high_s"));
        let public = arr32(get(vec, "pubkey"));
        assert_eq!(ed25519_public_key(&seed), public);
        assert_eq!(
            hash_domain(get(vec, "chain"), domain_of(get(vec, "domain")), &payload).to_hex(),
            get(vec, "digest")
        );
        assert!(verify_raw(&public, &digest, &signature).is_ok());
        assert_eq!(
            verify_raw(&public, &digest, &twin),
            Err(CryptoError::NonCanonicalSignature),
            "H2: high-S signature must be rejected"
        );
    }
}

#[test]
fn h2_digest_matches_oracle() {
    for vec in vectors().iter().filter(|v| kind(v) == "digest") {
        let payload = hx(get(vec, "payload_hex"));
        assert_eq!(
            hash_domain(get(vec, "chain"), domain_of(get(vec, "domain")), &payload).to_hex(),
            get(vec, "digest"),
            "digest mismatch for {:?}",
            vec.get("vector")
        );
    }
}
