//! C6 gate: Merkle proofs against an independent Python implementation.
//!
//! `fixtures/c6_golden.txt` comes from `tools/gen_c6_vectors.py`, which
//! self-checks pure-Python BLAKE3 against official vectors before emitting
//! roots, proofs and rejection vectors.

use std::collections::BTreeMap;

use aura_block::{
    EVENT_LEAF_CONTEXT, LEAF_CONTEXT, MAX_PROOF_DEPTH, MerkleError, MerkleProof, NODE_CONTEXT,
    build_event_tree, build_tree_from_leaf_hashes, build_tx_tree, compute_tx_root, event_leaf_hash,
    tree_depth, verify_event_inclusion, verify_proof, verify_tx_inclusion,
};
use aura_codec::CodecError;
use aura_tx::Tx;
use aura_types::Hash;

const GOLDEN: &str = include_str!("fixtures/c6_golden.txt");
const C5_GOLDEN: &str = include_str!("fixtures/c5_golden.txt");
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

/// Same trivial deterministic pattern as `tools/gen_c6_vectors.py`.
fn txid(index: usize) -> Hash {
    let mut bytes = [0u8; 32];
    for (j, byte) in bytes.iter_mut().enumerate() {
        *byte = ((index as u64) * 7 + (j as u64) * 13 + 0xAB) as u8;
    }
    Hash::from_bytes(bytes)
}

fn event(index: usize) -> Vec<u8> {
    let mut out = b"event:".to_vec();
    out.extend_from_slice(&(index as u32).to_le_bytes());
    out.extend(std::iter::repeat_n((index % 256) as u8, index % 17));
    out
}

fn txids(count: usize) -> Vec<Hash> {
    (0..count).map(txid).collect()
}

fn events(count: usize) -> Vec<Vec<u8>> {
    (0..count).map(event).collect()
}

fn decode_proof(key: &str, fixture: &BTreeMap<String, String>) -> Result<MerkleProof, MerkleError> {
    MerkleProof::from_canonical_bytes(&hx(&fixture[key]))
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(LEAF_CONTEXT), f["leaf_context"]);
    assert_eq!(to_hex(NODE_CONTEXT), f["node_context"]);
    assert_eq!(to_hex(EVENT_LEAF_CONTEXT), f["event_leaf_context"]);
    assert_eq!(MAX_PROOF_DEPTH.to_string(), f["max_proof_depth"]);

    // The C5 and C6 generators must agree on the shared tree contexts.
    let c5 = parse(C5_GOLDEN);
    assert_eq!(f["leaf_context"], c5["leaf_context"]);
    assert_eq!(f["node_context"], c5["node_context"]);
}

#[test]
fn golden_roots_match_independent_python() {
    let f = parse(GOLDEN);
    for size in [1usize, 2, 3, 5, 8, 1000] {
        let tree = build_tx_tree(&txids(size));
        assert_eq!(
            tree.root().to_hex(),
            f[&format!("root_tx_{size}")],
            "root_tx_{size}"
        );
        assert_eq!(tree.len(), size);
        assert_eq!(
            tree_depth(size as u64).to_string(),
            f[&format!("depth_tx_{size}")]
        );
    }
    for size in [1usize, 3, 5] {
        let tree = build_event_tree(&events(size));
        assert_eq!(
            tree.root().to_hex(),
            f[&format!("root_event_{size}")],
            "root_event_{size}"
        );
    }
}

#[test]
fn golden_proofs_match_independent_python() {
    let f = parse(GOLDEN);
    let mut count = 0;
    for (key, encoded) in f.iter().filter(|(key, _)| key.starts_with("proof_tx_")) {
        let parts: Vec<&str> = key.strip_prefix("proof_tx_").unwrap().split('_').collect();
        let size: usize = parts[0].parse().unwrap();
        let index: usize = parts[1].parse().unwrap();
        let tree = build_tx_tree(&txids(size));
        let proof = tree.prove(index).unwrap();
        assert_eq!(to_hex(&proof.to_bytes()), *encoded, "{key}");
        assert!(verify_proof(&tree.root(), &proof).is_ok(), "{key} path");
        assert!(
            verify_tx_inclusion(&tree.root(), &txid(index), &proof).is_ok(),
            "{key} inclusion"
        );
        count += 1;
    }
    for (key, encoded) in f.iter().filter(|(key, _)| key.starts_with("proof_event_")) {
        let parts: Vec<&str> = key
            .strip_prefix("proof_event_")
            .unwrap()
            .split('_')
            .collect();
        let size: usize = parts[0].parse().unwrap();
        let index: usize = parts[1].parse().unwrap();
        let tree = build_event_tree(&events(size));
        let proof = tree.prove(index).unwrap();
        assert_eq!(to_hex(&proof.to_bytes()), *encoded, "{key}");
        assert!(
            verify_event_inclusion(&tree.root(), &event(index), &proof).is_ok(),
            "{key} event inclusion"
        );
        count += 1;
    }
    assert_eq!(count, 34, "all independent proofs must be checked");
}

#[test]
fn leaf_domain_separation_is_pinned() {
    let f = parse(GOLDEN);
    assert_eq!(aura_block::leaf_hash(&txid(2)).to_hex(), f["leaf_tx_2"]);
    assert_eq!(event_leaf_hash(&event(2)).to_hex(), f["leaf_event_2"]);
    assert_ne!(f["leaf_tx_2"], f["leaf_event_2"]);

    // Length prefixing prevents an ambiguous concatenation.
    assert_ne!(event_leaf_hash(b"ab"), event_leaf_hash(b"a"));
    assert_ne!(event_leaf_hash(b"ab"), event_leaf_hash(b"a\x00b"));
}

#[test]
fn all_thousand_proofs_are_valid_and_one_substituted_is_invalid() {
    // C6 DoD: 1000 leaves, all proofs valid, one substituted — invalid.
    let f = parse(GOLDEN);
    let ids = txids(1000);
    let tree = build_tx_tree(&ids);
    let root = tree.root();
    assert_eq!(root.to_hex(), f["root_tx_1000"]);

    for (index, id) in ids.iter().enumerate() {
        let proof = tree.prove(index).expect("proof exists");
        assert_eq!(proof.total, 1000);
        assert_eq!(proof.index, index as u64);
        assert_eq!(proof.siblings.len(), 10, "ceil(log2(1000)) = 10");
        verify_tx_inclusion(&root, id, &proof)
            .unwrap_or_else(|e| panic!("proof {index} failed: {e}"));
    }

    // Substitute a different txid: leaf binding must catch it.
    let proof = tree.prove(499).unwrap();
    assert!(matches!(
        verify_tx_inclusion(&root, &txid(500), &proof),
        Err(MerkleError::LeafMismatch)
    ));

    // Substitute the leaf hash inside the proof: the path must catch it.
    let mut tampered = proof.clone();
    tampered.leaf = Hash::from_bytes([0x9c; 32]);
    assert!(matches!(
        verify_tx_inclusion(&root, &txid(499), &tampered),
        Err(MerkleError::LeafMismatch)
    ));
    assert!(matches!(
        verify_proof(&root, &tampered),
        Err(MerkleError::RootMismatch(_, _))
    ));
}

#[test]
fn c5_root_and_c6_tree_share_one_shape() {
    // The C6 refactor of compute_tx_root must not change any C5 root.
    let c4 = parse(C4_GOLDEN);
    let txs: Vec<Tx> = [1usize, 2, 4, 5]
        .iter()
        .map(|index| {
            Tx::from_canonical_bytes(&hx(&c4[&format!("tx{index}_bytes")])).expect("C4 tx decodes")
        })
        .collect();
    let ids: Vec<Hash> = txs.iter().map(|tx| tx.txid()).collect();
    assert_eq!(compute_tx_root(&txs), build_tx_tree(&ids).root());
    assert_eq!(
        compute_tx_root(&txs),
        build_tree_from_leaf_hashes(&ids.iter().map(aura_block::leaf_hash).collect::<Vec<_>>())
            .root()
    );
}

#[test]
fn empty_and_single_leaf_trees_are_well_defined() {
    let empty = build_tree_from_leaf_hashes(&[]);
    assert!(empty.is_empty());
    assert_eq!(empty.root(), Hash::ZERO);
    assert_eq!(empty.prove(0), Err(MerkleError::EmptyTree));
    assert_eq!(tree_depth(0), 0);

    let single = build_tx_tree(&txids(1));
    assert_eq!(single.root(), aura_block::leaf_hash(&txid(0)));
    let proof = single.prove(0).unwrap();
    assert!(proof.siblings.is_empty());
    assert!(verify_tx_inclusion(&single.root(), &txid(0), &proof).is_ok());
}

#[test]
fn event_proofs_are_bound_to_the_raw_event_bytes() {
    let tree = build_event_tree(&events(5));
    let proof = tree.prove(2).unwrap();
    assert!(verify_event_inclusion(&tree.root(), &event(2), &proof).is_ok());
    assert_eq!(
        verify_event_inclusion(&tree.root(), &event(3), &proof),
        Err(MerkleError::LeafMismatch)
    );
}

#[test]
fn every_bad_vector_is_rejected() {
    let f = parse(GOLDEN);
    let root = hash32(&f["root_tx_5"]);
    let bad: Vec<&str> = f
        .keys()
        .filter(|k| k.starts_with("bad_"))
        .map(String::as_str)
        .collect();
    assert_eq!(bad.len(), 11);
    for key in bad {
        let rejected = match decode_proof(key, &f) {
            Err(_) => true,
            Ok(proof) => verify_tx_inclusion(&root, &txid(2), &proof).is_err(),
        };
        assert!(rejected, "{key} must be rejected");
    }
}

#[test]
fn bad_vectors_return_the_specific_error() {
    let f = parse(GOLDEN);
    let root = hash32(&f["root_tx_5"]);
    let inclusion = |key: &str| {
        let proof = decode_proof(key, &f).expect("decodes");
        verify_tx_inclusion(&root, &txid(2), &proof)
    };

    assert_eq!(
        inclusion("bad_substituted_leaf"),
        Err(MerkleError::LeafMismatch)
    );
    assert!(matches!(
        inclusion("bad_wrong_sibling"),
        Err(MerkleError::RootMismatch(_, _))
    ));
    assert!(matches!(
        inclusion("bad_wrong_index"),
        Err(MerkleError::RootMismatch(_, _))
    ));
    assert_eq!(
        inclusion("bad_wrong_total"),
        Err(MerkleError::WrongSiblingCount(3, 2))
    );
    assert_eq!(
        inclusion("bad_sibling_count"),
        Err(MerkleError::WrongSiblingCount(2, 3))
    );
    assert_eq!(inclusion("bad_empty_total"), Err(MerkleError::EmptyTree));
    assert_eq!(
        inclusion("bad_index_oob"),
        Err(MerkleError::IndexOutOfBounds(5, 5))
    );

    assert_eq!(
        decode_proof("bad_trailing", &f),
        Err(MerkleError::Decode(CodecError::TrailingBytes(1)))
    );
    assert!(decode_proof("bad_truncated", &f).is_err());
    assert_eq!(
        decode_proof("bad_nonminimal_index", &f),
        Err(MerkleError::Decode(CodecError::NonCanonicalVarint))
    );
    assert_eq!(
        decode_proof("bad_depth_overflow", &f),
        Err(MerkleError::Decode(CodecError::TooManyItems(65)))
    );
}

#[test]
fn proof_wire_format_is_canonical_and_self_consistency_holds() {
    let f = parse(GOLDEN);
    let bytes = hx(&f["proof_tx_8_3"]);
    let proof = MerkleProof::from_canonical_bytes(&bytes).unwrap();
    assert_eq!(proof.to_bytes(), bytes);
    assert_eq!(
        MerkleProof::from_canonical_bytes(&proof.to_bytes()).unwrap(),
        proof
    );
    let tree = build_tx_tree(&txids(8));
    assert_eq!(proof, tree.prove(3).unwrap());
    assert!(verify_tx_inclusion(&tree.root(), &txid(3), &proof).is_ok());
}
