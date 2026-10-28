//! S1: versioned sparse binary Merkle tree.
//!
//! This replaces the S0 naive baseline with a deterministic, versioned,
//! provable state commitment. Design (decision D-S1):
//!
//! * path = [`StateKey::digest`] (C7, 256-bit BLAKE3 of the namespaced key);
//! * binary tree over the bits of that path, MSB first;
//! * **single-leaf subtree compression**: a subtree containing exactly one
//!   leaf is hashed directly as `leaf_hash(depth, path, value)` instead of
//!   descending all 256 levels. Empty subtrees use a depth-dependent
//!   `empty_hash(depth)` so a leaf cannot be confused with an empty branch;
//! * `state_root(version, tree_root)` binds the root to a block height, so
//!   two versions with identical contents have different ids;
//! * inclusion proofs carry the sibling hashes from the leaf up to the root.
//!
//! Honest scope: this is an in-memory tree. There is still no persistence
//! (S2), no execution overlay (S3) and no snapshots (S4). Values are opaque
//! byte strings; account encoding stays the caller's job (C7).
//!
//! The golden vectors come from `tools/gen_s1_vectors.py`, an independent
//! Python implementation of exactly this tree.

use std::collections::BTreeMap;
use std::fmt;

use aura_codec::{Writer, write_varint};
use aura_crypto::blake3_hash;
use aura_types::{Address, Hash};

use crate::key::{STATE_KEY_CONTEXT, StateKey};

/// Context for depth-dependent empty subtree hashes.
pub const S1_EMPTY_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-empty-v1\x00";
/// Context for account addresses used by the S1 vector generator.
pub const S1_ADDR_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-addr-v1\x00";
/// Context for deterministic vector values.
pub const S1_VALUE_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-value-v1\x00";
/// Context for compressed single-leaf nodes.
///
/// NC target: `tools/nc_s1.py` changes this constant and requires the S1
/// golden roots to go red.
pub const S1_LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-leaf-v1\x00";
/// Context for internal branch nodes.
///
/// NC target: `tools/nc_s1.py` changes this constant and requires the S1
/// golden roots to go red.
pub const S1_NODE_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-node-v1\x00";
/// Context binding a tree root to its version.
pub const S1_ROOT_CONTEXT: &[u8] = b"AuraLite-Crypto/s1-root-v1\x00";

/// Errors from tree operations and proof verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    /// The requested key is not in this version.
    KeyNotFound,
    /// The requested version was never committed.
    VersionNotFound(u64),
    /// A proof is structurally inconsistent.
    InvalidProof,
    /// A proof did not reconstruct the expected root.
    RootMismatch,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::KeyNotFound => f.write_str("key not found in this state version"),
            TreeError::VersionNotFound(v) => write!(f, "version {v} was never committed"),
            TreeError::InvalidProof => f.write_str("malformed inclusion proof"),
            TreeError::RootMismatch => f.write_str("proof did not reconstruct the state root"),
        }
    }
}

impl std::error::Error for TreeError {}

/// The MSB-first bit of a 256-bit path at `depth` (0 = root level).
pub fn key_bit(key: &Hash, depth: u16) -> u8 {
    let byte = key.as_bytes()[(depth / 8) as usize];
    (byte >> (7 - depth % 8)) & 1
}

/// Hash of an empty subtree at `depth`.
pub fn empty_hash(depth: u16) -> Hash {
    let mut input = Vec::with_capacity(S1_EMPTY_CONTEXT.len() + 2);
    input.extend_from_slice(S1_EMPTY_CONTEXT);
    input.extend_from_slice(&depth.to_le_bytes());
    blake3_hash(&input)
}

/// Hash of a compressed single-leaf subtree.
pub fn leaf_hash(depth: u16, key: &Hash, value: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(S1_LEAF_CONTEXT.len() + 2 + 32 + 8 + value.len());
    input.extend_from_slice(S1_LEAF_CONTEXT);
    input.extend_from_slice(&depth.to_le_bytes());
    input.extend_from_slice(key.as_bytes());
    input.extend_from_slice(&(value.len() as u64).to_le_bytes());
    input.extend_from_slice(value);
    blake3_hash(&input)
}

/// Hash of an internal branch.
pub fn node_hash(left: &Hash, right: &Hash) -> Hash {
    let mut input = Vec::with_capacity(S1_NODE_CONTEXT.len() + 64);
    input.extend_from_slice(S1_NODE_CONTEXT);
    input.extend_from_slice(left.as_bytes());
    input.extend_from_slice(right.as_bytes());
    blake3_hash(&input)
}

/// Bind a tree root to a version.
pub fn versioned_root(version: u64, tree_root: &Hash) -> Hash {
    let mut input = Vec::with_capacity(S1_ROOT_CONTEXT.len() + 8 + 32);
    input.extend_from_slice(S1_ROOT_CONTEXT);
    input.extend_from_slice(&version.to_le_bytes());
    input.extend_from_slice(tree_root.as_bytes());
    blake3_hash(&input)
}

/// Deterministic address for the S1 vector generator.
pub fn vector_address(index: u64) -> Address {
    let mut input = Vec::with_capacity(S1_ADDR_CONTEXT.len() + 8);
    input.extend_from_slice(S1_ADDR_CONTEXT);
    input.extend_from_slice(&index.to_le_bytes());
    let digest = blake3_hash(&input);
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(&digest.as_bytes()[..20]);
    Address::from_bytes(bytes)
}

/// Deterministic 32-byte value for the S1 vector generator.
pub fn vector_value(seed: u64) -> Vec<u8> {
    let mut input = Vec::with_capacity(S1_VALUE_CONTEXT.len() + 8);
    input.extend_from_slice(S1_VALUE_CONTEXT);
    input.extend_from_slice(&seed.to_le_bytes());
    blake3_hash(&input).as_bytes().to_vec()
}

/// Digest of a C7 account state key — the JMT path.
pub fn state_path(key: &StateKey) -> Hash {
    let raw = key.as_bytes();
    let mut input = Vec::with_capacity(STATE_KEY_CONTEXT.len() + 8 + raw.len());
    input.extend_from_slice(STATE_KEY_CONTEXT);
    input.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    input.extend_from_slice(raw);
    blake3_hash(&input)
}

/// A compact inclusion proof, siblings ordered bottom-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// Version whose state root this proof is checked against.
    pub version: u64,
    /// The 256-bit path being proven.
    pub key: Hash,
    /// The stored value.
    pub value: Vec<u8>,
    /// Depth at which the single-leaf compression stopped.
    pub leaf_depth: u16,
    /// Sibling hashes, from the leaf's parent up to the root.
    pub siblings: Vec<Hash>,
}

impl MerkleProof {
    /// Canonical wire bytes (used by the golden vectors).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.push_bytes(&self.version.to_le_bytes());
        writer.push_bytes(self.key.as_bytes());
        writer.push_bytes(&self.leaf_depth.to_le_bytes());
        write_varint(&mut writer, self.siblings.len() as u64);
        for sibling in &self.siblings {
            writer.push_bytes(sibling.as_bytes());
        }
        write_varint(&mut writer, self.value.len() as u64);
        writer.push_bytes(&self.value);
        writer.into_bytes()
    }
}

#[derive(Debug, Clone)]
enum NodeKind {
    Empty,
    Leaf { key: Hash, value: Vec<u8> },
    Branch { left: usize, right: usize },
}

#[derive(Debug, Clone)]
struct Node {
    depth: u16,
    hash: Hash,
    kind: NodeKind,
}

type Entry<'a> = (&'a Hash, &'a Vec<u8>);

fn build_nodes(leaves: &BTreeMap<Hash, Vec<u8>>) -> Vec<Node> {
    let entries: Vec<Entry<'_>> = leaves.iter().collect();
    let mut arena = Vec::with_capacity(leaves.len() * 2 + 1);
    build_subtree(0, &entries, &mut arena);
    arena
}

fn build_subtree(depth: u16, entries: &[Entry<'_>], arena: &mut Vec<Node>) -> usize {
    if entries.is_empty() {
        let hash = empty_hash(depth);
        arena.push(Node {
            depth,
            hash,
            kind: NodeKind::Empty,
        });
        return arena.len() - 1;
    }
    if entries.len() == 1 {
        let (key, value) = entries[0];
        let hash = leaf_hash(depth, key, value);
        arena.push(Node {
            depth,
            hash,
            kind: NodeKind::Leaf {
                key: *key,
                value: value.clone(),
            },
        });
        return arena.len() - 1;
    }
    debug_assert!(depth < 256, "duplicate 256-bit paths cannot exist");
    let mut left: Vec<Entry<'_>> = Vec::new();
    let mut right: Vec<Entry<'_>> = Vec::new();
    for entry in entries {
        if key_bit(entry.0, depth) == 0 {
            left.push(*entry);
        } else {
            right.push(*entry);
        }
    }
    let left_index = build_subtree(depth + 1, &left, arena);
    let right_index = build_subtree(depth + 1, &right, arena);
    let hash = node_hash(&arena[left_index].hash, &arena[right_index].hash);
    arena.push(Node {
        depth,
        hash,
        kind: NodeKind::Branch {
            left: left_index,
            right: right_index,
        },
    });
    arena.len() - 1
}

fn prove_from_arena(arena: &[Node], key: &Hash, version: u64) -> Result<MerkleProof, TreeError> {
    let mut index = arena.len() - 1;
    let mut siblings = Vec::new();
    loop {
        match &arena[index].kind {
            NodeKind::Empty => return Err(TreeError::KeyNotFound),
            NodeKind::Leaf {
                key: leaf_key,
                value,
            } => {
                if leaf_key != key {
                    return Err(TreeError::KeyNotFound);
                }
                siblings.reverse();
                return Ok(MerkleProof {
                    version,
                    key: *key,
                    value: value.clone(),
                    leaf_depth: arena[index].depth,
                    siblings,
                });
            }
            NodeKind::Branch { left, right } => {
                let (child, sibling) = if key_bit(key, arena[index].depth) == 0 {
                    (*left, *right)
                } else {
                    (*right, *left)
                };
                siblings.push(arena[sibling].hash);
                index = child;
            }
        }
    }
}

/// Verify an inclusion proof against a tree root.
///
/// NC target: `tools/nc_s1.py` disables the root comparison here and requires
/// the tampered-proof tests to go red.
pub fn verify_proof(root: &Hash, proof: &MerkleProof) -> Result<(), TreeError> {
    if proof.siblings.len() != proof.leaf_depth as usize {
        return Err(TreeError::InvalidProof);
    }
    let mut hash = leaf_hash(proof.leaf_depth, &proof.key, &proof.value);
    for (step, sibling) in proof.siblings.iter().enumerate() {
        let depth = proof.leaf_depth - 1 - step as u16;
        hash = if key_bit(&proof.key, depth) == 0 {
            node_hash(&hash, sibling)
        } else {
            node_hash(sibling, &hash)
        };
    }
    if versioned_root(proof.version, &hash) != *root {
        return Err(TreeError::RootMismatch);
    }
    Ok(())
}

/// Verify that `proof` proves `(key, value)` under `root`.
pub fn verify_inclusion(
    root: &Hash,
    key: &Hash,
    value: &[u8],
    proof: &MerkleProof,
) -> Result<(), TreeError> {
    if &proof.key != key || proof.value != value {
        return Err(TreeError::InvalidProof);
    }
    verify_proof(root, proof)
}

/// An in-memory versioned sparse Merkle tree.
#[derive(Debug, Clone, Default)]
pub struct MerkleTree {
    leaves: BTreeMap<Hash, Vec<u8>>,
    version: u64,
    roots: BTreeMap<u64, Hash>,
    snapshots: BTreeMap<u64, BTreeMap<Hash, Vec<u8>>>,
}

impl MerkleTree {
    /// Create an empty tree at version 0.
    pub fn new() -> Self {
        MerkleTree::default()
    }

    /// Current version (number of commits).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Number of leaves in the current state.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// True when the current state has no leaves.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Insert or overwrite a value for a C7 state key.
    pub fn insert(&mut self, key: &StateKey, value: Vec<u8>) {
        self.insert_digest(state_path(key), value);
    }

    /// Insert using a precomputed 256-bit path.
    pub fn insert_digest(&mut self, path: Hash, value: Vec<u8>) {
        self.leaves.insert(path, value);
    }

    /// Remove a key, returning its last value.
    pub fn remove(&mut self, key: &StateKey) -> Option<Vec<u8>> {
        self.remove_digest(&state_path(key))
    }

    /// Remove using a precomputed path.
    pub fn remove_digest(&mut self, path: &Hash) -> Option<Vec<u8>> {
        self.leaves.remove(path)
    }

    /// Look up a value by C7 state key.
    pub fn get(&self, key: &StateKey) -> Option<&[u8]> {
        self.get_digest(&state_path(key))
    }

    /// Look up by precomputed path.
    pub fn get_digest(&self, path: &Hash) -> Option<&[u8]> {
        self.leaves.get(path).map(Vec::as_slice)
    }

    /// Hash of the current tree root (not version-bound).
    pub fn tree_root(&self) -> Hash {
        let arena = build_nodes(&self.leaves);
        arena[arena.len() - 1].hash
    }

    /// Version-bound state root of the current state.
    pub fn state_root(&self) -> Hash {
        versioned_root(self.version, &self.tree_root())
    }

    /// Commit the current state as a new version, snapshotting it for proofs.
    pub fn commit(&mut self) -> Hash {
        self.version += 1;
        let root = versioned_root(self.version, &self.tree_root());
        self.roots.insert(self.version, root);
        self.snapshots.insert(self.version, self.leaves.clone());
        root
    }

    /// State root of a previously committed version.
    pub fn root_at(&self, version: u64) -> Option<Hash> {
        self.roots.get(&version).copied()
    }

    /// Number of committed versions available for proofs.
    pub fn version_count(&self) -> usize {
        self.snapshots.len()
    }

    /// All `(path, value)` leaves, sorted by path. Added in S3 for overlay
    /// materialisation; does not change the S1 wire format. Read-only.
    pub fn entries(&self) -> Vec<(Hash, Vec<u8>)> {
        self.leaves
            .iter()
            .map(|(path, value)| (*path, value.clone()))
            .collect()
    }

    /// Rebuild a tree at `version` from an ordered `[(path, value)]` set. Added
    /// in S3 for overlay commit.
    pub fn from_entries(version: u64, entries: &[(Hash, Vec<u8>)]) -> Self {
        let leaves: BTreeMap<Hash, Vec<u8>> = entries
            .iter()
            .map(|(path, value)| (*path, value.clone()))
            .collect();
        let mut tree = MerkleTree::new();
        tree.leaves = leaves;
        tree.version = version;
        tree.roots
            .insert(version, versioned_root(version, &tree.tree_root()));
        tree.snapshots.insert(version, tree.leaves.clone());
        tree
    }

    /// Build an inclusion proof for the current state.
    pub fn prove(&self, key: &StateKey) -> Result<MerkleProof, TreeError> {
        self.prove_digest(&state_path(key))
    }

    /// Build an inclusion proof for a precomputed path in the current state.
    pub fn prove_digest(&self, path: &Hash) -> Result<MerkleProof, TreeError> {
        let arena = build_nodes(&self.leaves);
        prove_from_arena(&arena, path, self.version)
    }

    /// Build many inclusion proofs against one committed version.
    ///
    /// The node arena is built ONCE and reused, so proving 1,000 keys costs
    /// O(tree_size + paths * depth) instead of rebuilding the tree per key.
    pub fn prove_many_at(
        &self,
        version: u64,
        paths: &[Hash],
    ) -> Result<Vec<MerkleProof>, TreeError> {
        let snapshot = self
            .snapshots
            .get(&version)
            .ok_or(TreeError::VersionNotFound(version))?;
        let arena = build_nodes(snapshot);
        paths
            .iter()
            .map(|path| prove_from_arena(&arena, path, version))
            .collect()
    }

    /// Build an inclusion proof against a committed version's snapshot.
    pub fn prove_at(&self, version: u64, path: &Hash) -> Result<MerkleProof, TreeError> {
        let mut proofs = self.prove_many_at(version, std::slice::from_ref(path))?;
        Ok(proofs.pop().expect("one path requested"))
    }
}
