//! S6 — the incremental `HistoryTree` must be indistinguishable from S1.
//!
//! Every assertion here compares two DIFFERENT algorithms: `HistoryTree`
//! (node map, incremental path update) against `MerkleTree` (full arena
//! rebuild per commit). Roots, proofs and entries must agree at every
//! checkpoint, including after deletes that force subtree compression.

use aura_crypto::blake3_hash;
use aura_state::{
    HistoryTree, MerkleTree, S1_ROOT_CONTEXT, StateKey, TreeError, empty_hash, leaf_hash,
    vector_address, vector_value, verify_proof, versioned_root,
};
use aura_types::Hash;

const OPS: u64 = 2_000;
const LIVE_KEYS: u64 = 96;

fn key_for(index: u64) -> StateKey {
    StateKey::account(&vector_address(index))
}

fn digest_for(index: u64) -> Hash {
    aura_state::state_path(&key_for(index))
}

/// Insert / overwrite / delete schedule: deterministic, covers all three paths.
fn op_at(step: u64) -> (u64, Option<Vec<u8>>) {
    let index = (step * 37 + 11) % LIVE_KEYS;
    if step % 5 == 4 {
        (index, None)
    } else {
        (index, Some(vector_value(step)))
    }
}

fn apply(tree: &mut HistoryTree, mirror: &mut MerkleTree, step: u64) {
    let (index, value) = op_at(step);
    let key = key_for(index);
    match value {
        Some(value) => {
            tree.insert(&key, value.clone());
            mirror.insert(&key, value);
        }
        None => {
            tree.remove_digest(&digest_for(index));
            mirror.remove(&key);
        }
    }
}

#[test]
fn history_tree_matches_s1_rebuild_at_every_checkpoint() {
    let mut tree = HistoryTree::new();
    let mut mirror = MerkleTree::new();
    let mut checkpoints = 0u64;

    assert_eq!(tree.tree_root(), empty_hash(0), "empty tree root");
    assert_eq!(tree.tree_root(), mirror.tree_root(), "empty mirror root");

    for step in 0..OPS {
        apply(&mut tree, &mut mirror, step);
        if step % 100 == 99 {
            mirror.commit();
            let version = mirror.version();
            assert_eq!(
                tree.tree_root(),
                mirror.tree_root(),
                "tree root diverged at step {step}"
            );
            assert_eq!(
                tree.state_root(version),
                mirror.state_root(),
                "versioned root diverged at step {step} (version {version})"
            );
            assert_eq!(
                tree.len(),
                mirror.len(),
                "leaf count diverged at step {step}"
            );
            assert_eq!(
                tree.entries(),
                mirror.entries(),
                "entries diverged at {step}"
            );
            checkpoints += 1;
        }
    }
    assert_eq!(checkpoints, OPS / 100);
    assert!(
        !tree.is_empty(),
        "the schedule must leave a non-empty state"
    );
}

#[test]
fn history_tree_proofs_verify_against_s1_roots() {
    let mut tree = HistoryTree::new();
    let mut mirror = MerkleTree::new();
    for step in 0..600 {
        apply(&mut tree, &mut mirror, step);
    }
    mirror.commit();
    let version = mirror.version();
    let root = tree.state_root(version);
    assert_eq!(root, mirror.state_root());

    let mut proven = 0u64;
    for index in 0..LIVE_KEYS {
        let digest = digest_for(index);
        let Some(value) = tree.get_digest(&digest).map(<[u8]>::to_vec) else {
            continue;
        };
        let proof = mirror
            .prove_at(version, &digest)
            .expect("committed version has a snapshot");
        verify_proof(&root, &proof).expect("proof must verify against the S1 root");
        assert_eq!(proof.value, value, "proof value must match the tree value");
        proven += 1;
    }
    assert!(
        proven > 50,
        "expected most live keys to prove, got {proven}"
    );
}

#[test]
fn delete_compresses_the_subtree_back_to_a_leaf() {
    // Two leaves under one branch, then delete one: S1 must compress the
    // survivor up to the depth of the branch, and the incremental tree must
    // follow exactly.
    let mut tree = HistoryTree::new();
    let mut mirror = MerkleTree::new();
    let a = key_for(0);
    let b = key_for(1);
    tree.insert(&a, vector_value(1));
    tree.insert(&b, vector_value(2));
    mirror.insert(&a, vector_value(1));
    mirror.insert(&b, vector_value(2));
    let two_leaf_nodes = tree.node_count();
    assert!(
        two_leaf_nodes >= 3,
        "two leaves need a branch and two leaves"
    );

    tree.remove_digest(&digest_for(0));
    mirror.remove(&a);
    assert_eq!(tree.tree_root(), mirror.tree_root(), "root after delete");

    // A single leaf compresses to depth 0: the root is the leaf hash itself.
    let expected = leaf_hash(0, &digest_for(1), &vector_value(2));
    assert_eq!(
        tree.tree_root(),
        expected,
        "single leaf must sit at depth 0"
    );
    assert!(
        tree.node_count() < 10,
        "arena must not grow past a handful of nodes, got {}",
        tree.node_count()
    );

    tree.remove_digest(&digest_for(1));
    mirror.remove(&b);
    assert!(tree.is_empty());
    assert_eq!(
        tree.tree_root(),
        empty_hash(0),
        "empty tree root after deletes"
    );
    assert_eq!(tree.tree_root(), mirror.tree_root());
    assert!(
        tree.node_count() < 20,
        "a drained tree must not keep an unbounded arena, got {}",
        tree.node_count()
    );
}

#[test]
fn versioned_root_is_s1_domain_separated() {
    let mut tree = HistoryTree::new();
    tree.insert(&key_for(7), vector_value(7));
    let tree_root = tree.tree_root();
    let expected = {
        let mut input = Vec::new();
        input.extend_from_slice(S1_ROOT_CONTEXT);
        input.extend_from_slice(&9u64.to_le_bytes());
        input.extend_from_slice(tree_root.as_bytes());
        blake3_hash(&input)
    };
    assert_eq!(tree.state_root(9), expected);
    assert_eq!(tree.state_root(9), versioned_root(9, &tree_root));
    assert_ne!(
        tree.state_root(9),
        tree.state_root(10),
        "version is bound in"
    );
}

#[test]
fn incremental_updates_are_cheap_and_measured() {
    // 5 000 writes into a 96-leaf working set. A rebuild-per-commit tree pays
    // ~2 * leaves hashes per commit; the incremental tree pays ~2 * depth.
    let mut tree = HistoryTree::new();
    for step in 0..5_000u64 {
        let index = (step * 37 + 11) % LIVE_KEYS;
        tree.insert(&key_for(index), vector_value(step));
    }
    let writes = tree.writes();
    let hashes = tree.hashes();
    assert_eq!(writes, 5_000);
    // Bound per write: split costs a branch chain, but the working set is 96
    // leaves (depth ~ 7), so 32 hashes per write is a generous ceiling.
    assert!(
        hashes <= writes * 32,
        "incremental tree used {hashes} hashes for {writes} writes"
    );
    println!(
        "history tree: {writes} writes, {hashes} hashes, {} nodes, {} leaves",
        tree.node_count(),
        tree.len()
    );
}

#[test]
fn missing_key_errors_instead_of_inventing_a_proof() {
    let mut tree = HistoryTree::new();
    tree.insert(&key_for(3), vector_value(3));
    let mirror = MerkleTree::from_entries(1, &tree.entries());
    let absent = digest_for(4);
    assert_eq!(tree.get_digest(&absent), None);
    assert_eq!(
        mirror.prove_at(1, &absent),
        Err(TreeError::KeyNotFound),
        "an absent key must not produce a proof"
    );
}
