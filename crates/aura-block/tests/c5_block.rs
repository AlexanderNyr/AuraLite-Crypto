//! C5 gate: block/header format against an independent Python encoder.
//!
//! `fixtures/c5_golden.txt` comes from `tools/gen_c5_vectors.py`, which
//! self-checks pure-Python BLAKE3 against official vectors and reuses the
//! committed C4 transaction vectors as block bodies.

use std::collections::BTreeMap;

use aura_block::{
    BLOCK_HASH_PAYLOAD_CONTEXT, Block, BlockError, Body, HeaderParams, LEAF_CONTEXT, MAX_BLOCK_TXS,
    MAX_QC_BYTES, NODE_CONTEXT, compute_tx_root, leaf_hash, node_hash,
};
use aura_codec::{CodecError, encode_to_bytes};
use aura_tx::Tx;
use aura_types::{Address, Amount, CHAIN_ID, ChainId, Gas, Hash, Height, PROTOCOL_VERSION};

const GOLDEN: &str = include_str!("fixtures/c5_golden.txt");
const C4_GOLDEN: &str = include_str!("../../aura-tx/tests/fixtures/c4_golden.txt");

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

fn hash32(text: &str) -> Hash {
    Hash::from_slice(&hx(text)).expect("32-byte hash")
}

fn c4_tx(index: usize) -> Tx {
    let f = parse(C4_GOLDEN);
    Tx::from_canonical_bytes(&hx(&f[&format!("tx{index}_bytes")]))
        .unwrap_or_else(|e| panic!("tx{index} decode: {e}"))
}

fn header_params(g: &BTreeMap<String, String>, index: usize) -> HeaderParams {
    let k = |key: &str| g[&format!("b{index}_{key}")].clone();
    let qc = if k("qc_present") == "1" {
        Some(hx(&k("qc")))
    } else {
        None
    };
    HeaderParams {
        version: k("version").parse().expect("u32"),
        chain_id: ChainId::new(&k("chain")).expect("valid chain-id"),
        height: Height::new(k("height").parse().expect("u64")),
        parent_hash: hash32(&k("parent")),
        state_root: hash32(&k("state_root")),
        validator_set_hash: hash32(&k("validator_set_hash")),
        timestamp_ms: k("timestamp_ms").parse().expect("u64"),
        proposer: Address::from_slice(&hx(&k("proposer"))).expect("20-byte address"),
        qc,
        gas_used: Gas::new(k("gas_used").parse().expect("u64")),
        base_fee: Amount::from_units(k("base_fee").parse().expect("u128")),
    }
}

fn build(g: &BTreeMap<String, String>, index: usize) -> Block {
    let ids: Vec<usize> = g[&format!("b{index}_tx_ids")]
        .split(',')
        .filter(|item| !item.is_empty())
        .map(|item| item.parse().expect("tx index"))
        .collect();
    let txs: Vec<Tx> = ids.into_iter().map(c4_tx).collect();
    Block::new(header_params(g, index), txs).expect("valid block")
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(LEAF_CONTEXT), f["leaf_context"]);
    assert_eq!(to_hex(NODE_CONTEXT), f["node_context"]);
    assert_eq!(
        to_hex(BLOCK_HASH_PAYLOAD_CONTEXT),
        f["block_hash_payload_context"]
    );
    assert_eq!(MAX_BLOCK_TXS.to_string(), f["max_block_txs"]);
    assert_eq!(MAX_QC_BYTES.to_string(), f["max_qc_bytes"]);
    assert_eq!(PROTOCOL_VERSION.to_string(), f["protocol_version"]);
    assert_eq!(CHAIN_ID, f["chain_id"]);
}

#[test]
fn golden_blocks_match_the_independent_encoder() {
    let f = parse(GOLDEN);
    for index in 1..=4 {
        let block = build(&f, index);
        assert_eq!(
            to_hex(&encode_to_bytes(&block.header)),
            f[&format!("b{index}_header")]
        );
        assert_eq!(
            to_hex(&encode_to_bytes(&block.body)),
            f[&format!("b{index}_body")]
        );
        assert_eq!(to_hex(&block.to_bytes()), f[&format!("b{index}_bytes")]);
        assert_eq!(
            block.header.tx_root.to_hex(),
            f[&format!("b{index}_tx_root")]
        );
        assert_eq!(
            block.body.tx_root().to_hex(),
            f[&format!("b{index}_tx_root")]
        );
        assert_eq!(
            block.block_hash().to_hex(),
            f[&format!("b{index}_block_hash")]
        );

        let decoded = Block::from_canonical_bytes(&hx(&f[&format!("b{index}_bytes")]))
            .unwrap_or_else(|e| panic!("b{index} decode: {e}"));
        assert_eq!(decoded, block, "b{index} canonical round-trip");
        assert_eq!(
            decoded.block_hash().to_hex(),
            f[&format!("b{index}_block_hash")]
        );
        assert!(block.validate(CHAIN_ID).is_ok(), "b{index} validates");
    }
    // b2 really is the child of b1.
    assert_eq!(f["b2_parent"], f["b1_block_hash"]);
}

#[test]
fn changing_tx_order_changes_tx_root() {
    let f = parse(GOLDEN);
    assert_ne!(f["tx_root_forward"], f["tx_root_backward"]);
    let first = c4_tx(1);
    let second = c4_tx(2);
    let forward = compute_tx_root(&[first.clone(), second.clone()]);
    let backward = compute_tx_root(&[second, first]);
    assert_eq!(forward.to_hex(), f["tx_root_forward"]);
    assert_eq!(backward.to_hex(), f["tx_root_backward"]);
    assert_ne!(forward, backward);
}

#[test]
fn merkle_tree_shape_is_pinned() {
    assert_eq!(compute_tx_root(&[]), Hash::ZERO, "empty body -> zero root");

    let single = c4_tx(4);
    assert_eq!(
        compute_tx_root(std::slice::from_ref(&single)),
        leaf_hash(&single.txid()),
        "one leaf is the root"
    );

    // Three leaves: odd final node is duplicated at the second level.
    let a = c4_tx(1);
    let b = c4_tx(2);
    let c = c4_tx(5);
    let la = leaf_hash(&a.txid());
    let lb = leaf_hash(&b.txid());
    let lc = leaf_hash(&c.txid());
    let expected = node_hash(&node_hash(&la, &lb), &node_hash(&lc, &lc));
    assert_eq!(compute_tx_root(&[a, b, c]), expected);
}

#[test]
fn block_hash_covers_the_whole_header_and_the_chain() {
    let f = parse(GOLDEN);
    let block = build(&f, 1);
    let original = block.block_hash();

    let mut cases: Vec<(&str, Block)> = Vec::new();
    let mut t = block.clone();
    t.header.timestamp_ms += 1;
    cases.push(("timestamp_ms", t));
    let mut t = block.clone();
    t.header.parent_hash = Hash::ZERO;
    cases.push(("parent_hash", t));
    let mut t = block.clone();
    t.header.state_root = Hash::ZERO;
    cases.push(("state_root", t));
    let mut t = block.clone();
    t.header.chain_id = ChainId::new("auralite-9").unwrap();
    cases.push(("chain_id", t));
    let mut t = block.clone();
    t.header.version += 1;
    cases.push(("version", t));

    for (name, mutated) in cases {
        assert_ne!(
            mutated.block_hash(),
            original,
            "changing {name} must change block_hash"
        );
    }
    assert_eq!(block.block_hash(), original, "block_hash is deterministic");
}

#[test]
fn every_bad_vector_is_rejected() {
    let f = parse(GOLDEN);
    const BAD: [&str; 9] = [
        "bad_trailing",
        "bad_truncated",
        "bad_version",
        "bad_root",
        "bad_reordered",
        "bad_unsorted_with_root",
        "bad_duplicate",
        "bad_qc_tag",
        "bad_nonminimal_count",
    ];
    for key in BAD {
        let bytes = hx(&f[key]);
        let rejected = match Block::from_canonical_bytes(&bytes) {
            Err(_) => true,
            Ok(block) => block.validate(CHAIN_ID).is_err(),
        };
        assert!(rejected, "{key} must be rejected");
    }
}

#[test]
fn bad_vectors_return_the_specific_error() {
    let f = parse(GOLDEN);
    let decode = |key: &str| Block::from_canonical_bytes(&hx(&f[key]));

    assert_eq!(
        decode("bad_trailing"),
        Err(BlockError::Decode(CodecError::TrailingBytes(1)))
    );
    assert!(decode("bad_truncated").is_err());
    assert_eq!(
        decode("bad_qc_tag"),
        Err(BlockError::Decode(CodecError::InvalidOptionTag(2)))
    );
    assert_eq!(
        decode("bad_nonminimal_count"),
        Err(BlockError::Decode(CodecError::NonCanonicalVarint))
    );
    assert_eq!(
        decode("bad_version").unwrap().validate(CHAIN_ID),
        Err(BlockError::UnsupportedProtocolVersion(1, 2))
    );
    let bad_root = decode("bad_root").unwrap();
    assert!(matches!(
        bad_root.validate(CHAIN_ID),
        Err(BlockError::TxRootMismatch(_, _))
    ));
    assert_eq!(
        decode("bad_reordered").unwrap().validate(CHAIN_ID),
        Err(BlockError::UnsortedTransactions)
    );
    assert_eq!(
        decode("bad_unsorted_with_root").unwrap().validate(CHAIN_ID),
        Err(BlockError::UnsortedTransactions)
    );
    assert_eq!(
        decode("bad_duplicate").unwrap().validate(CHAIN_ID),
        Err(BlockError::DuplicateTxKey)
    );
}

#[test]
fn reorder_without_recomputing_the_root_is_rejected() {
    let f = parse(GOLDEN);
    let mut block = build(&f, 1);
    assert!(block.validate(CHAIN_ID).is_ok());
    block.body.txs.reverse();
    assert!(
        block.validate(CHAIN_ID).is_err(),
        "NC target: reorder -> red"
    );
    assert_ne!(block.body.tx_root(), block.header.tx_root);
}

#[test]
fn body_rejects_duplicate_keys_at_construction() {
    let first = c4_tx(1);
    assert_eq!(
        Body::new(vec![first.clone(), first]).err(),
        Some(BlockError::DuplicateTxKey)
    );
}

#[test]
fn wrong_chain_and_size_limits_are_enforced() {
    let f = parse(GOLDEN);
    let block = build(&f, 1);
    assert_eq!(
        block.validate("auralite-1"),
        Err(BlockError::WrongChain(
            "auralite-1".to_owned(),
            CHAIN_ID.to_owned()
        ))
    );

    let mut big_qc = build(&f, 4);
    big_qc.header.qc = Some(vec![0u8; MAX_QC_BYTES + 1]);
    assert_eq!(
        big_qc.validate(CHAIN_ID),
        Err(BlockError::QcTooLarge(MAX_QC_BYTES + 1))
    );
}

#[test]
fn transaction_validation_is_separate_from_block_validation() {
    let f = parse(GOLDEN);
    let mut block = build(&f, 1);
    assert!(block.validate(CHAIN_ID).is_ok());
    assert!(block.validate_transactions(CHAIN_ID).is_ok());

    // Break a signature, recompute the root so block-level validation still
    // passes: only full transaction validation can catch this.
    block.body.txs[0].sig.bytes[0] ^= 0x01;
    block.header.tx_root = block.body.tx_root();
    assert!(block.validate(CHAIN_ID).is_ok());
    assert!(matches!(
        block.validate_transactions(CHAIN_ID),
        Err(BlockError::InvalidTransaction(_))
    ));

    // A height past a tx's expiry is also a transaction-level failure.
    let mut expired = build(&f, 1);
    expired.header.height = Height::new(2_000);
    assert!(expired.validate(CHAIN_ID).is_ok());
    assert!(expired.validate_transactions(CHAIN_ID).is_err());
}
