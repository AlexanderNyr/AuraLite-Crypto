//! H4 tree differential (docs/h4-property-plan.md §2, D-H4-1).
//!
//! An independent re-implementation of the S1 sparse merkle rules (hash
//! layouts assembled here from the constants, not by calling the production
//! helpers) must agree with `MerkleTree` over generated key/value sets.
//! `tools/nc_h4.py` NC-5 swaps left/right in `node_hash` and this reference
//! must notice (`H4: tree differential mismatch`). The cross-language half of
//! the differential lives in `tools/h4_reference.py`.

use aura_crypto::blake3_hash;
use aura_state::{MerkleTree, StateKey};
use aura_types::Hash;

const S1_EMPTY_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-empty-v1\0";
const S1_LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-leaf-v1\0";
const S1_NODE_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-node-v1\0";

fn ref_empty(depth: u16) -> Hash {
    let mut input = Vec::new();
    input.extend_from_slice(S1_EMPTY_CONTEXT);
    input.extend_from_slice(&depth.to_le_bytes());
    blake3_hash(&input)
}

fn ref_leaf(depth: u16, key: &Hash, value: &[u8]) -> Hash {
    let mut input = Vec::new();
    input.extend_from_slice(S1_LEAF_CONTEXT);
    input.extend_from_slice(&depth.to_le_bytes());
    input.extend_from_slice(key.as_bytes());
    input.extend_from_slice(&(value.len() as u64).to_le_bytes());
    input.extend_from_slice(value);
    blake3_hash(&input)
}

fn ref_node(left: &Hash, right: &Hash) -> Hash {
    let mut input = Vec::new();
    input.extend_from_slice(S1_NODE_CONTEXT);
    input.extend_from_slice(left.as_bytes());
    input.extend_from_slice(right.as_bytes());
    blake3_hash(&input)
}

fn key_bit(key: &Hash, depth: u16) -> u8 {
    let byte = key.as_bytes()[(depth / 8) as usize];
    (byte >> (7 - depth % 8)) & 1
}

/// Naive recursive sparse-tree root: empty subtree → empty_hash(depth); a
/// single leaf is compressed into leaf_hash(depth, key, value); otherwise the
/// set splits on the key bit at `depth`.
fn ref_root(entries: &mut [(Hash, Vec<u8>)], depth: u16) -> Hash {
    if entries.is_empty() {
        return ref_empty(depth);
    }
    if entries.len() == 1 {
        let (key, value) = &entries[0];
        return ref_leaf(depth, key, value);
    }
    let split = entries
        .iter()
        .position(|(key, _)| key_bit(key, depth) == 1)
        .unwrap_or(entries.len());
    let (low, high) = entries.split_at_mut(split);
    ref_node(&ref_root(low, depth + 1), &ref_root(high, depth + 1))
}

fn case(entries: &[(u64, Vec<u8>)]) {
    let mut prepared: Vec<(Hash, Vec<u8>)> = entries
        .iter()
        .map(|(index, value)| {
            (
                StateKey::account(&aura_types::Address::from_bytes([*index as u8; 20])).digest(),
                value.clone(),
            )
        })
        .collect();
    prepared.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let expected = ref_root(&mut prepared, 0);

    let mut tree = MerkleTree::new();
    for (index, value) in entries {
        let key = StateKey::account(&aura_types::Address::from_bytes([*index as u8; 20]));
        tree.insert(&key, value.clone());
    }
    assert_eq!(tree.tree_root(), expected, "H4: tree differential mismatch");
}

#[test]
fn h4_tree_differential() {
    // Deterministic sweep: k entries for k = 1..=16 with fixed values.
    for count in 1u64..=16 {
        let entries: Vec<(u64, Vec<u8>)> = (0..count)
            .map(|index| (index, vec![index as u8; 3 + (index as usize % 5)]))
            .collect();
        case(&entries);
    }
    // Sparse sets: far-apart keys and a single leaf.
    case(&[(0, vec![1]), (255, vec![2])]);
    case(&[(7, vec![7; 32])]);
    case(&[(1, vec![]), (2, vec![0]), (3, vec![0, 0])]);
    // Deterministic pseudo-random sets (SplitMix64 inline).
    let mut state = 20261180u64;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    for round in 0..32u64 {
        let count = 1 + next() % 12;
        let entries: Vec<(u64, Vec<u8>)> = (0..count)
            .map(|_| {
                let key = next() % 256;
                let len = (next() % 24) as usize;
                let value = (0..len).map(|_| next() as u8).collect();
                (key, value)
            })
            .collect();
        let mut unique = entries.clone();
        unique.sort_by_key(|(key, _)| *key);
        unique.dedup_by_key(|(key, _)| *key);
        case(&unique);
        let _ = round;
    }
}
