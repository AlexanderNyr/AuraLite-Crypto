//! C4 gate: transaction format against an independent Python encoder.
//!
//! `fixtures/c4_golden.txt` is produced by `tools/gen_c4_vectors.py`, which
//! self-checks pure-Python BLAKE3 and pure-Python Ed25519 against official
//! BLAKE3 vectors and RFC 8032 before emitting anything.

use std::collections::BTreeMap;

use aura_codec::{CodecError, encode_to_bytes};
use aura_crypto::{ed25519_public_key, hash_domain};
use aura_tx::{
    ADDRESS_CONTEXT, MAX_TX_PAYLOAD, SIGNING_CONTEXT, SignatureAlgo, TXID_CONTEXT, Tx, TxError,
    TxKind, TxParams, UnsignedTx, signer_address,
};
use aura_types::{
    Address, Amount, CHAIN_ID, ChainId, Domain, Gas, Height, MAINNET_CHAIN_ID, Nonce,
};

const GOLDEN: &str = include_str!("fixtures/c4_golden.txt");

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
        _ => panic!("invalid hex nibble in fixture"),
    }
}

fn arr32(text: &str) -> [u8; 32] {
    hx(text).try_into().expect("expected 32 bytes")
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn get(f: &BTreeMap<String, String>, index: usize, key: &str) -> String {
    f[&format!("tx{index}_{key}")].clone()
}

fn build(f: &BTreeMap<String, String>, index: usize) -> (UnsignedTx, [u8; 32]) {
    let unsigned = UnsignedTx::new(TxParams {
        chain_id: ChainId::new(&get(f, index, "chain")).expect("valid chain-id"),
        signer: Address::from_slice(&hx(&get(f, index, "address"))).expect("20-byte address"),
        nonce: Nonce::new(get(f, index, "nonce").parse().expect("u64")),
        kind: TxKind::from_tag(get(f, index, "kind").parse().expect("u8 tag")).expect("known kind"),
        payload: hx(&get(f, index, "payload")),
        gas_limit: Gas::new(get(f, index, "gas").parse().expect("u64")),
        max_fee: Amount::from_units(get(f, index, "max_fee").parse().expect("u128")),
        tip: Amount::from_units(get(f, index, "tip").parse().expect("u128")),
        expiry: Height::new(get(f, index, "expiry").parse().expect("u64")),
    })
    .expect("structurally valid");
    (unsigned, arr32(&get(f, index, "seed")))
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = fixtures();
    assert_eq!(to_hex(SIGNING_CONTEXT), f["signing_context"]);
    assert_eq!(to_hex(TXID_CONTEXT), f["txid_context"]);
    assert_eq!(to_hex(ADDRESS_CONTEXT), f["address_context"]);
    assert_eq!(MAX_TX_PAYLOAD.to_string(), f["max_tx_payload"]);
    assert_eq!(aura_codec::CODEC_VERSION.to_string(), f["codec_version"]);
}

#[test]
fn golden_transactions_match_the_independent_encoder() {
    let f = fixtures();
    for index in 1..=5 {
        let (unsigned, seed) = build(&f, index);

        assert_eq!(
            to_hex(&encode_to_bytes(&unsigned)),
            get(&f, index, "unsigned")
        );
        let signing = unsigned.signing_bytes();
        assert_eq!(to_hex(&signing), get(&f, index, "signing_bytes"));

        let digest = hash_domain(unsigned.chain_id.as_str(), Domain::Tx, &signing);
        assert_eq!(digest.to_hex(), get(&f, index, "signing_hash"));

        let public_key = ed25519_public_key(&seed);
        assert_eq!(to_hex(&public_key), get(&f, index, "public_key"));
        assert_eq!(
            to_hex(signer_address(&public_key).as_bytes()),
            get(&f, index, "address"),
            "tx{index} signer derivation"
        );

        let tx = unsigned.sign(&seed).expect("signer matches key");
        assert_eq!(to_hex(&tx.sig.bytes), get(&f, index, "signature"));
        assert_eq!(to_hex(&tx.to_bytes()), get(&f, index, "bytes"));
        assert_eq!(tx.txid().to_hex(), get(&f, index, "txid"));
        assert_eq!(tx.signing_bytes(), signing);

        let decoded = Tx::from_canonical_bytes(&hx(&get(&f, index, "bytes")))
            .unwrap_or_else(|e| panic!("tx{index} decode: {e}"));
        assert_eq!(decoded, tx, "tx{index} canonical round-trip");
        assert_eq!(
            decoded.txid().to_hex(),
            get(&f, index, "txid"),
            "tx{index} decoded txid"
        );
        assert!(
            tx.validate(&get(&f, index, "chain"), Height::new(0))
                .is_ok(),
            "tx{index} validates on its own chain"
        );
    }
}

#[test]
fn replay_across_chains_is_rejected() {
    // tx3 is minted for the reserved mainnet chain-id.
    let f = fixtures();
    let (unsigned, seed) = build(&f, 3);
    assert_eq!(unsigned.chain_id.as_str(), MAINNET_CHAIN_ID);
    let tx = unsigned.sign(&seed).unwrap();

    assert!(tx.validate(MAINNET_CHAIN_ID, Height::new(0)).is_ok());
    assert_eq!(
        tx.validate(CHAIN_ID, Height::new(0)),
        Err(TxError::WrongChain(
            CHAIN_ID.to_owned(),
            MAINNET_CHAIN_ID.to_owned()
        ))
    );

    // Rewriting the chain-id after signing cannot rescue it: chain-id is
    // inside both signing_bytes and the C3 domain prefix.
    let mut rewritten = tx.clone();
    rewritten.unsigned.chain_id = ChainId::stage1();
    assert!(rewritten.verify_signature().is_err());

    // Same unsigned payload, different chain => different signature.
    let mut other = unsigned.clone();
    other.chain_id = ChainId::stage1();
    // `signer` stays bound to the same key; only the chain changes.
    let other_tx = other.sign(&seed).unwrap();
    assert_ne!(tx.sig.bytes, other_tx.sig.bytes);
    assert_ne!(tx.signing_bytes(), other_tx.signing_bytes());
}

#[test]
fn signing_requires_the_matching_key() {
    let f = fixtures();
    let (unsigned, _) = build(&f, 1);
    let wrong_seed = [0x66u8; 32];
    assert_eq!(
        unsigned.sign(&wrong_seed).err(),
        Some(TxError::SignerMismatch)
    );
}

#[test]
fn every_mutation_breaks_the_signature() {
    let f = fixtures();
    let (unsigned, seed) = build(&f, 2);
    let tx = unsigned.sign(&seed).unwrap();
    assert!(tx.verify_signature().is_ok());

    let mut cases: Vec<(&str, Tx)> = Vec::new();

    let mut t = tx.clone();
    t.unsigned.codec_version = 2;
    cases.push(("codec_version", t));

    let mut t = tx.clone();
    t.unsigned.chain_id = ChainId::new("auralite-9").unwrap();
    cases.push(("chain_id", t));

    let mut t = tx.clone();
    let mut addr = *t.unsigned.signer.as_bytes();
    addr[0] ^= 0x01;
    t.unsigned.signer = Address::from_bytes(addr);
    cases.push(("signer", t));

    let mut t = tx.clone();
    t.unsigned.nonce = Nonce::new(999);
    cases.push(("nonce", t));

    let mut t = tx.clone();
    t.unsigned.kind = TxKind::Deploy;
    cases.push(("kind", t));

    let mut t = tx.clone();
    t.unsigned.payload.push(0x00);
    cases.push(("payload", t));

    let mut t = tx.clone();
    t.unsigned.gas_limit = Gas::new(1);
    cases.push(("gas_limit", t));

    let mut t = tx.clone();
    t.unsigned.max_fee = Amount::from_units(1);
    cases.push(("max_fee", t));

    let mut t = tx.clone();
    t.unsigned.tip = Amount::from_units(1);
    cases.push(("tip", t));

    let mut t = tx.clone();
    t.unsigned.expiry = Height::new(1);
    cases.push(("expiry", t));

    let mut t = tx.clone();
    t.sig.public_key[0] ^= 0x01;
    cases.push(("public_key", t));

    let mut t = tx.clone();
    t.sig.bytes[0] ^= 0x01;
    cases.push(("signature_bytes", t));

    for (name, mutated) in cases {
        assert!(
            mutated.verify_signature().is_err(),
            "changing {name} must invalidate the signature"
        );
    }
}

#[test]
fn every_bad_vector_is_rejected() {
    let f = fixtures();
    const BAD: [&str; 9] = [
        "bad_trailing",
        "bad_truncated",
        "bad_version",
        "bad_kind_tag",
        "bad_algo_tag",
        "bad_signer",
        "bad_signature",
        "bad_chain_len",
        "bad_payload_len",
    ];
    for key in BAD {
        let bytes = hx(&f[key]);
        let rejected = match Tx::from_canonical_bytes(&bytes) {
            Err(_) => true,
            Ok(tx) => tx.validate(CHAIN_ID, Height::new(0)).is_err(),
        };
        assert!(rejected, "{key} must be rejected");
    }
}

#[test]
fn bad_vectors_return_the_specific_error() {
    let f = fixtures();
    let check = |key: &str, expected: CodecError| {
        assert_eq!(
            Tx::from_canonical_bytes(&hx(&f[key])),
            Err(TxError::Decode(expected)),
            "{key} error kind"
        );
    };
    check("bad_trailing", CodecError::TrailingBytes(1));
    check("bad_version", CodecError::UnsupportedVersion(1, 2));
    check("bad_kind_tag", CodecError::InvalidEnumTag("TxKind", 9));
    check(
        "bad_algo_tag",
        CodecError::InvalidEnumTag("SignatureAlgo", 1),
    );
    check("bad_chain_len", CodecError::NonCanonicalVarint);
    check("bad_payload_len", CodecError::NonCanonicalVarint);
    assert_eq!(
        Tx::from_canonical_bytes(&hx(&f["bad_signer"]))
            .unwrap()
            .verify_signature(),
        Err(TxError::SignerMismatch)
    );
    assert_eq!(
        Tx::from_canonical_bytes(&hx(&f["bad_signature"]))
            .unwrap()
            .verify_signature(),
        Err(TxError::InvalidSignature)
    );
}

#[test]
fn structural_validation_rules_are_enforced() {
    let f = fixtures();
    let (unsigned, seed) = build(&f, 1);
    let tx = unsigned.sign(&seed).unwrap();

    assert_eq!(
        tx.validate(CHAIN_ID, Height::new(1001)),
        Err(TxError::Expired(1000, 1001))
    );
    assert!(tx.validate(CHAIN_ID, Height::new(1000)).is_ok());

    let mut wrong_version = tx.clone();
    wrong_version.unsigned.codec_version = 2;
    assert_eq!(
        wrong_version.validate(CHAIN_ID, Height::new(0)),
        Err(TxError::UnsupportedCodecVersion(1, 2))
    );

    let mut oversized = tx.clone();
    oversized.unsigned.payload = vec![0u8; MAX_TX_PAYLOAD + 1];
    assert_eq!(
        oversized.validate(CHAIN_ID, Height::new(0)),
        Err(TxError::PayloadTooLarge(MAX_TX_PAYLOAD + 1))
    );

    let mut zero_gas = tx.clone();
    zero_gas.unsigned.gas_limit = Gas::new(0);
    assert_eq!(
        zero_gas.validate(CHAIN_ID, Height::new(0)),
        Err(TxError::ZeroGasLimit)
    );

    let mut bad_tip = tx.clone();
    bad_tip.unsigned.tip = Amount::from_units(1001);
    bad_tip.unsigned.max_fee = Amount::from_units(1000);
    assert_eq!(
        bad_tip.validate(CHAIN_ID, Height::new(0)),
        Err(TxError::TipExceedsMaxFee)
    );

    // The constructor refuses the same mistakes before signing exists.
    assert_eq!(
        UnsignedTx::new(TxParams {
            chain_id: ChainId::stage1(),
            signer: tx.unsigned.signer,
            nonce: Nonce::new(0),
            kind: TxKind::Transfer,
            payload: vec![],
            gas_limit: Gas::new(0),
            max_fee: Amount::ZERO,
            tip: Amount::ZERO,
            expiry: Height::new(1),
        })
        .err(),
        Some(TxError::ZeroGasLimit)
    );
    assert!(
        UnsignedTx::new(TxParams {
            chain_id: ChainId::stage1(),
            signer: tx.unsigned.signer,
            nonce: Nonce::new(0),
            kind: TxKind::Transfer,
            payload: vec![0u8; MAX_TX_PAYLOAD],
            gas_limit: Gas::new(1),
            max_fee: Amount::ZERO,
            tip: Amount::ZERO,
            expiry: Height::new(1),
        })
        .is_ok(),
        "payload exactly at the limit is allowed"
    );
}

#[test]
fn enum_tags_are_strict() {
    assert_eq!(TxKind::ALL.map(TxKind::tag), [0, 1, 2, 3, 4]);
    for tag in 0..=4u8 {
        assert_eq!(TxKind::from_tag(tag).map(TxKind::tag), Some(tag));
    }
    assert_eq!(TxKind::from_tag(5), None);
    assert_eq!(TxKind::from_tag(255), None);
    assert_eq!(
        SignatureAlgo::from_tag(aura_tx::SIG_ALGO_ED25519),
        Some(SignatureAlgo::Ed25519)
    );
    assert_eq!(SignatureAlgo::from_tag(1), None);
}

#[test]
fn txid_covers_the_signature_too() {
    let f = fixtures();
    let (unsigned, seed) = build(&f, 1);
    let tx = unsigned.sign(&seed).unwrap();

    let mut other = tx.clone();
    other.sig.bytes[63] ^= 0x01;
    assert_ne!(tx.txid(), other.txid(), "signature must affect the txid");
    assert!(other.verify_signature().is_err());
    assert_eq!(tx.txid(), tx.txid(), "txid is deterministic");
}
