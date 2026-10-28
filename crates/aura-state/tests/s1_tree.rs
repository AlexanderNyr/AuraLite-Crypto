//! S1 gate: 10,000 operations and 1,000 proofs against an independent Python
//! implementation of the same sparse Merkle tree.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use aura_state::{
    MerkleTree, S1_ADDR_CONTEXT, S1_EMPTY_CONTEXT, S1_LEAF_CONTEXT, S1_NODE_CONTEXT,
    S1_ROOT_CONTEXT, S1_VALUE_CONTEXT, StateKey, TreeError, empty_hash, state_path, vector_address,
    vector_value, verify_inclusion, verify_proof,
};
use aura_types::Hash;

const GOLDEN: &str = include_str!("fixtures/s1_golden.txt");

const MASK64: u64 = u64::MAX;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 & MASK64
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

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
        assert!(!key.is_empty());
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

fn hash32(text: &str) -> Hash {
    let bytes = hx(text);
    Hash::from_slice(&bytes).expect("32-byte hash")
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn f_u64(f: &BTreeMap<String, String>, key: &str) -> u64 {
    f[key].parse().expect("u64")
}

static SHARED: OnceLock<(MerkleTree, Vec<(u64, Hash)>)> = OnceLock::new();

/// Build the 10,000-operation state once per test binary; the other tests
/// reuse it so `make test` stays fast.
fn shared(f: &BTreeMap<String, String>) -> &'static (MerkleTree, Vec<(u64, Hash)>) {
    SHARED.get_or_init(|| build_tree(f))
}

fn build_tree(f: &BTreeMap<String, String>) -> (MerkleTree, Vec<(u64, Hash)>) {
    let operation_count = f_u64(f, "operation_count");
    let key_space = f_u64(f, "key_space");
    let checkpoint_count = f_u64(f, "checkpoint_count") as usize;
    let checkpoints: BTreeSet<u64> = (1..=checkpoint_count)
        .map(|index| f_u64(f, &format!("checkpoint_{index}_op")))
        .collect();

    let mut rng = Lcg(f_u64(f, "operation_seed"));
    let mut tree = MerkleTree::new();
    let mut committed: Vec<(u64, Hash)> = Vec::new();
    for index in 0..operation_count {
        let kind = rng.below(10);
        let key_index = rng.below(key_space);
        let seed = rng.next();
        let key = StateKey::account(&vector_address(key_index));
        if kind < 7 {
            tree.insert(&key, vector_value(seed));
        } else if kind == 7 {
            tree.remove(&key);
        }

        if checkpoints.contains(&index) {
            let number = committed.len() + 1;
            let expected_version = f_u64(f, &format!("checkpoint_{number}_version"));
            let root = tree.commit();
            assert_eq!(
                tree.version(),
                expected_version,
                "checkpoint {number} version"
            );
            assert_eq!(
                tree.len().to_string(),
                f[&format!("checkpoint_{number}_leaves")],
                "checkpoint {number} leaves"
            );
            assert_eq!(
                root.to_hex(),
                f[&format!("checkpoint_{number}_root")],
                "checkpoint {number} root"
            );
            assert_eq!(tree.state_root(), root, "state_root must equal commit root");
            committed.push((expected_version, root));
        }
    }
    assert_eq!(tree.version() as usize, checkpoint_count);
    (tree, committed)
}

#[test]
fn constants_are_pinned_by_the_independent_fixture() {
    let f = parse(GOLDEN);
    assert_eq!(to_hex(S1_ADDR_CONTEXT), f["addr_context"]);
    assert_eq!(to_hex(S1_VALUE_CONTEXT), f["value_context"]);
    assert_eq!(to_hex(S1_EMPTY_CONTEXT), f["empty_context"]);
    assert_eq!(to_hex(S1_LEAF_CONTEXT), f["leaf_context"]);
    assert_eq!(to_hex(S1_NODE_CONTEXT), f["node_context"]);
    assert_eq!(to_hex(S1_ROOT_CONTEXT), f["root_context"]);
    assert_eq!(f["operation_count"], "10000");
    assert_eq!(f["checkpoint_count"], "12");
    assert_eq!(f["proof_count"], "1000");
    assert_eq!(empty_hash(0).to_hex(), f["empty_root_depth0"]);
    assert_eq!(empty_hash(256).to_hex(), f["empty_root_depth256"]);
}

#[test]
fn ten_thousand_operations_match_independent_roots() {
    // S1 DoD: 10,000 operations, root matches an independent reference.
    let f = parse(GOLDEN);
    let (tree, committed) = shared(&f);
    assert_eq!(committed.len(), 12);
    assert!(tree.len() >= 1000);
    for (version, root) in committed {
        assert_eq!(tree.root_at(*version), Some(*root));
    }
    let mut distinct = BTreeSet::new();
    for (_, root) in committed {
        distinct.insert(*root);
    }
    assert_eq!(distinct.len(), 12, "versioning must change every root");
}

#[test]
fn one_thousand_proofs_match_and_verify() {
    // S1 DoD: proof check on 1000 keys.
    let f = parse(GOLDEN);
    let (tree, committed) = shared(&f);
    let (version, root) = committed.last().copied().unwrap();

    let mut paths = Vec::with_capacity(1000);
    let mut values = Vec::with_capacity(1000);
    for number in 1..=1000 {
        paths.push(hash32(&f[&format!("proof_{number}_key")]));
        values.push(hx(&f[&format!("proof_{number}_value")]));
    }
    let proofs = tree.prove_many_at(version, &paths).expect("batch proofs");
    assert_eq!(proofs.len(), 1000);
    for (offset, proof) in proofs.iter().enumerate() {
        let number = offset + 1;
        let key = paths[offset];
        let value = &values[offset];
        assert_eq!(
            to_hex(&proof.to_bytes()),
            f[&format!("proof_{number}_bytes")],
            "proof {number} bytes"
        );
        verify_proof(&root, proof).unwrap_or_else(|e| panic!("proof {number} verification: {e}"));
        verify_inclusion(&root, &key, value, proof).expect("inclusion");
        assert_eq!(proof.version, version);
        assert_eq!(&proof.value, value);
        assert_eq!(proof.siblings.len(), proof.leaf_depth as usize);
    }
}

#[test]
fn proof_tampering_is_rejected() {
    let f = parse(GOLDEN);
    let (tree, committed) = shared(&f);
    let (version, root) = committed.last().copied().unwrap();
    let key = hash32(&f["proof_1_key"]);
    let proof = tree.prove_at(version, &key).unwrap();

    let mut flipped_sibling = proof.clone();
    let mut bytes = *flipped_sibling.siblings[0].as_bytes();
    bytes[0] ^= 0x01;
    flipped_sibling.siblings[0] = Hash::from_bytes(bytes);
    assert_eq!(
        verify_proof(&root, &flipped_sibling),
        Err(TreeError::RootMismatch)
    );

    let mut flipped_value = proof.clone();
    flipped_value.value[0] ^= 0x01;
    assert_eq!(
        verify_proof(&root, &flipped_value),
        Err(TreeError::RootMismatch)
    );

    let wrong_key = hash32(&f["proof_2_key"]);
    assert_eq!(
        verify_inclusion(&root, &wrong_key, &proof.value, &proof),
        Err(TreeError::InvalidProof)
    );

    let mut truncated = proof.clone();
    truncated.siblings.pop();
    assert_eq!(
        verify_proof(&root, &truncated),
        Err(TreeError::InvalidProof)
    );

    let absent = Hash::from_bytes([0xEE; 32]);
    assert_eq!(tree.prove_at(version, &absent), Err(TreeError::KeyNotFound));
    assert_eq!(
        tree.prove_at(999, &key),
        Err(TreeError::VersionNotFound(999))
    );
}

#[test]
fn insertion_order_does_not_change_the_root() {
    let f = parse(GOLDEN);
    let (tree, _) = shared(&f);
    let expected = tree.tree_root();

    // Re-insert the same final state in a different order via digest paths.
    let mut forward = MerkleTree::new();
    let mut reverse = MerkleTree::new();
    let mut entries: Vec<(Hash, Vec<u8>)> = Vec::new();
    for number in 1..=200 {
        let key = hash32(&f[&format!("proof_{number}_key")]);
        let value = hx(&f[&format!("proof_{number}_value")]);
        entries.push((key, value));
    }
    for (key, value) in &entries {
        forward.insert_digest(*key, value.clone());
    }
    for (key, value) in entries.iter().rev() {
        reverse.insert_digest(*key, value.clone());
    }
    assert_eq!(forward.tree_root(), reverse.tree_root());
    assert_ne!(
        forward.tree_root(),
        expected,
        "subset must differ from full tree"
    );

    // Deleting one leaf changes the root; re-adding restores it.
    let (first, value) = entries[0].clone();
    let mut changed = forward.clone();
    assert_eq!(changed.remove_digest(&first), Some(value.clone()));
    assert_ne!(changed.tree_root(), forward.tree_root());
    changed.insert_digest(first, value);
    assert_eq!(changed.tree_root(), forward.tree_root());
}

#[test]
fn state_path_matches_c7_digest_and_versions_are_distinct() {
    let f = parse(GOLDEN);
    let (mut tree, committed) = shared(&f).clone();
    let key = StateKey::account(&vector_address(7));
    assert_eq!(state_path(&key), key.digest());
    let before = tree.state_root();
    let after = tree.commit();
    assert_ne!(before, after, "version must be bound into the root");
    assert_ne!(committed[0].1, committed[11].1);
    assert_eq!(tree.version_count(), 13);
}
