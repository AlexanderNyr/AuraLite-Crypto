//! S6: incremental versioned tree with S1-identical root semantics.
//!
//! Why this module exists (measured, not assumed). `MerkleTree` (S1) rebuilds
//! the whole node arena on every `commit()`: at 1 024 live leaves that is
//! **16.4 ms per commit in a debug build** (measured, 5 000 commits), i.e.
//! about 27 minutes for the S6 DoD of 100 000 blocks. A pruning layer cannot
//! pay that bill: it needs one root per block, 100 000 times.
//!
//! [`HistoryTree`] keeps the SAME tree shape — `empty_hash(depth)` for an
//! empty subtree, `leaf_hash(depth, path, value)` for a compressed
//! single-entry subtree, `node_hash(left, right)` for a branch — but stores
//! nodes in a mutable arena and updates only the path that changed. Cost per
//! write is `O(compressed leaf depth)` node hashes (measured: ~12 for a
//! 1 024-key working set) instead of `O(tree size)`. [`HistoryTree::hashes`]
//! reports the exact count, so the claim stays measurable.
//!
//! Root commitments stay S1: `versioned_root(version, tree_root)`. The roots
//! produced here are therefore interchangeable with `MerkleTree::state_root`,
//! which the S6 gates verify against a full *rebuild* at every checkpoint
//! (`crates/aura-state/tests/s6_history.rs`, `crates/aura-storage/tests/
//! s6_prune.rs`) and against an independent Python rebuild
//! (`tools/gen_s6_vectors.py`).
//!
//! S6 deliberately defines no wire format here: the version records, the base
//! snapshot and the retention policy live in `aura-storage::prune`.

use std::collections::BTreeMap;

use aura_types::Hash;

use crate::tree::{empty_hash, key_bit, leaf_hash, node_hash, state_path, versioned_root};

/// Index of the shared empty node. Never overwritten, never freed.
const EMPTY: u32 = 0;

#[derive(Debug, Clone)]
enum Node {
    Empty,
    Leaf {
        hash: Hash,
        key: Hash,
        value: Vec<u8>,
    },
    Branch {
        hash: Hash,
        left: u32,
        right: u32,
    },
}

impl Node {
    fn hash(&self) -> Option<&Hash> {
        match self {
            Node::Empty => None,
            Node::Leaf { hash, .. } | Node::Branch { hash, .. } => Some(hash),
        }
    }
}

/// The 257 possible empty-subtree hashes, computed once.
///
/// `empty_hash(depth)` is a BLAKE3 call and an update asks for the same
/// handful of depths on every write; caching them removes those calls without
/// changing a single root.
fn empty_hashes() -> &'static [Hash; 257] {
    static TABLE: std::sync::OnceLock<[Hash; 257]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [Hash::ZERO; 257];
        for (depth, slot) in table.iter_mut().enumerate() {
            *slot = empty_hash(depth as u16);
        }
        table
    })
}

fn cached_empty(depth: u16) -> Hash {
    empty_hashes()[depth as usize]
}

/// An incrementally maintained compressed sparse Merkle tree.
///
/// The root commitment is bit-for-bit the S1 commitment, so a `HistoryTree`
/// and a `MerkleTree` holding the same leaves at the same version report the
/// same `state_root`.
#[derive(Debug, Clone)]
pub struct HistoryTree {
    nodes: Vec<Node>,
    root: u32,
    root_hash: Hash,
    leaves: BTreeMap<Hash, Vec<u8>>,
    writes: u64,
    hashes: u64,
}

impl Default for HistoryTree {
    fn default() -> Self {
        HistoryTree::new()
    }
}

impl HistoryTree {
    /// Empty tree at root `empty_hash(0)`.
    pub fn new() -> Self {
        HistoryTree {
            nodes: vec![Node::Empty],
            root: EMPTY,
            root_hash: cached_empty(0),
            leaves: BTreeMap::new(),
            writes: 0,
            hashes: 0,
        }
    }

    /// Rebuild a tree from `(path, value)` entries by inserting them.
    pub fn from_entries(entries: &[(Hash, Vec<u8>)]) -> Self {
        let mut tree = HistoryTree::new();
        for (path, value) in entries {
            tree.insert_digest(*path, value.clone());
        }
        tree
    }

    /// Hash of the current tree root (not version-bound).
    pub fn tree_root(&self) -> Hash {
        self.root_hash
    }

    /// Version-bound state root, identical to `MerkleTree::state_root`.
    pub fn state_root(&self, version: u64) -> Hash {
        versioned_root(version, &self.root_hash)
    }

    /// Number of leaves in the current state.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// True when the current state has no leaves.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Number of arena nodes (garbage from collapsed subtrees included).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of writes applied so far.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// Exact number of leaf/branch hash computations performed so far.
    pub fn hashes(&self) -> u64 {
        self.hashes
    }

    /// Read a value by precomputed 256-bit path.
    pub fn get_digest(&self, path: &Hash) -> Option<&[u8]> {
        self.leaves.get(path).map(Vec::as_slice)
    }

    /// All `(path, value)` leaves, sorted by path.
    pub fn entries(&self) -> Vec<(Hash, Vec<u8>)> {
        self.leaves
            .iter()
            .map(|(path, value)| (*path, value.clone()))
            .collect()
    }

    /// Insert using a C7 state key.
    pub fn insert(&mut self, key: &crate::StateKey, value: Vec<u8>) {
        self.insert_digest(state_path(key), value);
    }

    /// Insert using a precomputed 256-bit path.
    pub fn insert_digest(&mut self, path: Hash, value: Vec<u8>) {
        self.leaves.insert(path, value.clone());
        self.writes += 1;
        self.root = self.insert_at(self.root, 0, &path, Some(&value));
        self.root_hash = match self.nodes[self.root as usize].hash() {
            Some(hash) => *hash,
            None => cached_empty(0),
        };
    }

    /// Remove using a precomputed 256-bit path; returns the previous value.
    pub fn remove_digest(&mut self, path: &Hash) -> Option<Vec<u8>> {
        let previous = self.leaves.remove(path);
        if previous.is_some() {
            self.writes += 1;
            self.root = self.insert_at(self.root, 0, path, None);
            self.root_hash = match self.nodes[self.root as usize].hash() {
                Some(hash) => *hash,
                None => cached_empty(0),
            };
        }
        previous
    }

    fn push(&mut self, node: Node) -> u32 {
        self.nodes.push(node);
        (self.nodes.len() - 1) as u32
    }

    fn hash_of(&self, index: u32, depth: u16) -> Hash {
        match self.nodes[index as usize].hash() {
            Some(hash) => *hash,
            None => cached_empty(depth),
        }
    }

    /// Insert (`Some`) or delete (`None`) below `index`; returns the index of
    /// the (possibly new) node that replaces it.
    fn insert_at(&mut self, index: u32, depth: u16, key: &Hash, value: Option<&Vec<u8>>) -> u32 {
        let node = std::mem::replace(&mut self.nodes[index as usize], Node::Empty);
        match node {
            Node::Empty => match value {
                None => EMPTY,
                Some(value) => {
                    let hash = leaf_hash(depth, key, value);
                    self.hashes += 1;
                    self.push(Node::Leaf {
                        hash,
                        key: *key,
                        value: value.to_vec(),
                    })
                }
            },
            Node::Leaf {
                hash,
                key: leaf_key,
                value: leaf_value,
            } => {
                if &leaf_key == key {
                    match value {
                        None => EMPTY,
                        Some(value) => {
                            let new_hash = leaf_hash(depth, key, value);
                            self.hashes += 1;
                            let index = self.push(Node::Leaf {
                                hash: new_hash,
                                key: *key,
                                value: value.to_vec(),
                            });
                            let _ = (hash, leaf_value);
                            index
                        }
                    }
                } else {
                    match value {
                        // Different key: restore the leaf untouched.
                        None => self.push(Node::Leaf {
                            hash,
                            key: leaf_key,
                            value: leaf_value,
                        }),
                        Some(value) => self.split(depth, &leaf_key, &leaf_value, key, value),
                    }
                }
            }
            Node::Branch { hash, left, right } => {
                let _ = hash;
                let goes_left = key_bit(key, depth) == 0;
                let (child, other) = if goes_left {
                    (left, right)
                } else {
                    (right, left)
                };
                let new_child = self.insert_at(child, depth + 1, key, value);
                self.rebuild(index, depth, goes_left, new_child, other)
            }
        }
    }

    /// Recompute a branch in place after one child changed, compressing a
    /// single-entry subtree back up to this depth (S1 shape).
    fn rebuild(
        &mut self,
        index: u32,
        depth: u16,
        child_is_left: bool,
        new_child: u32,
        other: u32,
    ) -> u32 {
        let child_depth = depth + 1;
        let child_hash = self.hash_of(new_child, child_depth);
        let other_hash = self.hash_of(other, child_depth);
        let child_empty = new_child == EMPTY;
        let other_empty = other == EMPTY;
        if child_empty && other_empty {
            return EMPTY;
        }
        if child_empty || other_empty {
            // The surviving subtree holds one entry iff it is a leaf; S1 then
            // compresses it up to THIS depth, so its hash changes.
            let survivor = if child_empty { other } else { new_child };
            if let Node::Leaf { key, value, .. } = self.nodes[survivor as usize].clone() {
                let hash = leaf_hash(depth, &key, &value);
                self.hashes += 1;
                self.nodes[index as usize] = Node::Leaf { hash, key, value };
                return index;
            }
        }
        let hash = if child_is_left {
            node_hash(&child_hash, &other_hash)
        } else {
            node_hash(&other_hash, &child_hash)
        };
        self.hashes += 1;
        let (left, right) = if child_is_left {
            (new_child, other)
        } else {
            (other, new_child)
        };
        self.nodes[index as usize] = Node::Branch { hash, left, right };
        index
    }

    /// Split one compressed leaf into a branch chain holding two leaves.
    fn split(
        &mut self,
        depth: u16,
        old_key: &Hash,
        old_value: &[u8],
        new_key: &Hash,
        new_value: &[u8],
    ) -> u32 {
        let mut divergence = depth;
        while key_bit(old_key, divergence) == key_bit(new_key, divergence) {
            divergence += 1;
            debug_assert!(divergence < 256, "distinct 256-bit paths must diverge");
        }
        let leaf_depth = divergence + 1;
        let old_hash = leaf_hash(leaf_depth, old_key, old_value);
        let new_hash = leaf_hash(leaf_depth, new_key, new_value);
        self.hashes += 2;
        let old_index = self.push(Node::Leaf {
            hash: old_hash,
            key: *old_key,
            value: old_value.to_vec(),
        });
        let new_index = self.push(Node::Leaf {
            hash: new_hash,
            key: *new_key,
            value: new_value.to_vec(),
        });
        let (left, right, left_hash, right_hash) = if key_bit(old_key, divergence) == 0 {
            (old_index, new_index, old_hash, new_hash)
        } else {
            (new_index, old_index, new_hash, old_hash)
        };
        let mut hash = node_hash(&left_hash, &right_hash);
        self.hashes += 1;
        let mut branch = self.push(Node::Branch { hash, left, right });

        // Walk up to `depth`: every level above the divergence is a branch
        // with an empty sibling, exactly as the S1 rebuild would produce.
        while divergence > depth {
            let child = branch;
            let child_hash = hash;
            divergence -= 1;
            let empty = cached_empty(divergence + 1);
            let goes_left = key_bit(old_key, divergence) == 0;
            hash = if goes_left {
                node_hash(&child_hash, &empty)
            } else {
                node_hash(&empty, &child_hash)
            };
            self.hashes += 1;
            let (left, right) = if goes_left {
                (child, EMPTY)
            } else {
                (EMPTY, child)
            };
            branch = self.push(Node::Branch { hash, left, right });
        }
        branch
    }
}
