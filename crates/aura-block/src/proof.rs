//! Compact Merkle inclusion proofs (C6).
//!
//! The tree shape is the C5 tree: leaves are `leaf_hash(txid)` (or
//! `event_leaf_hash(event)` for the event tree), internal nodes are
//! `node_hash(left, right)`, and an odd final node is duplicated at every
//! level. C6 adds proof generation and verification over that same tree —
//! the `tx_root` scheme is unchanged.
//!
//! A proof is `varint(index) || varint(total) || leaf[32] ||
//! varint(sibling_count) || sibling[32] * count`. Sibling direction is
//! derived from the index at each level, so no direction bits are stored.
//! For 1000 leaves the depth is 10, i.e. ~330 bytes per proof.

use aura_codec::{
    CodecError, Decode, Encode, Reader, Writer, decode_canonical, encode_to_bytes, read_varint,
    write_varint,
};
use aura_crypto::blake3_hash;
use aura_types::Hash;

use crate::error::MerkleError;
use crate::merkle::{leaf_hash, node_hash};

/// Domain tag for event leaves. Separate from [`crate::LEAF_CONTEXT`] so a
/// transaction leaf can never be replayed as an event leaf.
///
/// NC target: `tools/nc_c6.py` changes this constant and requires the C6
/// event vectors to go red.
pub const EVENT_LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/event-leaf-v1\x00";

/// Maximum proof depth. A `u64` leaf count needs at most 64 levels.
pub const MAX_PROOF_DEPTH: usize = 64;

/// Hash an event into a leaf: length-prefixed so boundaries are unambiguous.
pub fn event_leaf_hash(event: &[u8]) -> Hash {
    let mut input = Vec::with_capacity(EVENT_LEAF_CONTEXT.len() + 8 + event.len());
    input.extend_from_slice(EVENT_LEAF_CONTEXT);
    input.extend_from_slice(&(event.len() as u64).to_le_bytes());
    input.extend_from_slice(event);
    blake3_hash(&input)
}

/// Number of sibling hashes a proof for `total` leaves must carry.
pub fn tree_depth(total: u64) -> usize {
    let mut len = total;
    let mut depth = 0usize;
    while len > 1 {
        len = len.div_ceil(2);
        depth += 1;
    }
    depth
}

/// In-memory Merkle tree. Levels are padded (odd final node duplicated), so
/// `prove()` can always find `position ^ 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleTree {
    total: usize,
    levels: Vec<Vec<Hash>>,
}

impl MerkleTree {
    /// Original (unpadded) number of leaves.
    pub fn len(&self) -> usize {
        self.total
    }

    /// True if the tree has no leaves.
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// The Merkle root, or [`Hash::ZERO`] for an empty tree.
    pub fn root(&self) -> Hash {
        match self.levels.last() {
            Some(level) => level[0],
            None => Hash::ZERO,
        }
    }

    /// Build an inclusion proof for one leaf index.
    pub fn prove(&self, index: usize) -> Result<MerkleProof, MerkleError> {
        if self.total == 0 {
            return Err(MerkleError::EmptyTree);
        }
        if index >= self.total {
            return Err(MerkleError::IndexOutOfBounds(
                index as u64,
                self.total as u64,
            ));
        }
        let mut siblings = Vec::with_capacity(self.levels.len().saturating_sub(1));
        let mut position = index;
        for level in 0..self.levels.len().saturating_sub(1) {
            let nodes = &self.levels[level];
            siblings.push(nodes[position ^ 1]);
            position /= 2;
        }
        Ok(MerkleProof {
            index: index as u64,
            total: self.total as u64,
            leaf: self.levels[0][index],
            siblings,
        })
    }
}

/// Build a tree from pre-hashed leaves (single source of tree shape).
pub fn build_tree_from_leaf_hashes(leaf_hashes: &[Hash]) -> MerkleTree {
    if leaf_hashes.is_empty() {
        return MerkleTree {
            total: 0,
            levels: Vec::new(),
        };
    }
    let total = leaf_hashes.len();
    let mut levels: Vec<Vec<Hash>> = Vec::new();
    let mut current = leaf_hashes.to_vec();
    loop {
        if current.len() > 1 && current.len() % 2 == 1 {
            current.push(*current.last().expect("non-empty level"));
        }
        levels.push(current.clone());
        if current.len() == 1 {
            break;
        }
        current = current
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| node_hash(&pair[0], &pair[1]))
            .collect();
    }
    MerkleTree { total, levels }
}

/// Build a transaction tree from txids.
pub fn build_tx_tree(txids: &[Hash]) -> MerkleTree {
    build_tree_from_leaf_hashes(&txids.iter().map(leaf_hash).collect::<Vec<_>>())
}

/// Build an event tree from raw event bytes.
pub fn build_event_tree(events: &[Vec<u8>]) -> MerkleTree {
    build_tree_from_leaf_hashes(
        &events
            .iter()
            .map(|event| event_leaf_hash(event))
            .collect::<Vec<_>>(),
    )
}

/// A compact inclusion proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// Leaf index in the original (unpadded) tree.
    pub index: u64,
    /// Total number of leaves in the original tree.
    pub total: u64,
    /// Hash of the leaf being proven.
    pub leaf: Hash,
    /// Sibling hashes, bottom-up.
    pub siblings: Vec<Hash>,
}

impl MerkleProof {
    /// Canonical wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode_to_bytes(self)
    }

    /// Decode with full C2 canonicality.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, MerkleError> {
        Ok(decode_canonical::<MerkleProof>(bytes)?)
    }
}

impl Encode for MerkleProof {
    fn encode(&self, writer: &mut Writer) {
        write_varint(writer, self.index);
        write_varint(writer, self.total);
        writer.push_bytes(self.leaf.as_bytes());
        write_varint(writer, self.siblings.len() as u64);
        for sibling in &self.siblings {
            writer.push_bytes(sibling.as_bytes());
        }
    }
}

impl Decode for MerkleProof {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, CodecError> {
        let index = read_varint(reader)?;
        let total = read_varint(reader)?;
        let leaf = Hash::decode(reader)?;
        let count = read_varint(reader)?;
        if count > MAX_PROOF_DEPTH as u64 {
            return Err(CodecError::TooManyItems(count as usize));
        }
        let mut siblings = Vec::with_capacity(count as usize);
        for _ in 0..count {
            siblings.push(Hash::decode(reader)?);
        }
        Ok(MerkleProof {
            index,
            total,
            leaf,
            siblings,
        })
    }
}

/// Verify a proof path against a root.
///
/// NC targets: `tools/nc_c6.py` disables the sibling-count check, the
/// direction rule and the root comparison here, requiring the C6 vectors to
/// go red.
pub fn verify_proof(root: &Hash, proof: &MerkleProof) -> Result<(), MerkleError> {
    if proof.total == 0 {
        return Err(MerkleError::EmptyTree);
    }
    if proof.index >= proof.total {
        return Err(MerkleError::IndexOutOfBounds(proof.index, proof.total));
    }
    let expected_depth = tree_depth(proof.total);
    if proof.siblings.len() != expected_depth {
        return Err(MerkleError::WrongSiblingCount(
            proof.siblings.len(),
            expected_depth,
        ));
    }
    let mut computed = proof.leaf;
    let mut position = proof.index;
    for sibling in &proof.siblings {
        if position.is_multiple_of(2) {
            computed = node_hash(&computed, sibling);
        } else {
            computed = node_hash(sibling, &computed);
        }
        position /= 2;
    }
    if computed != *root {
        return Err(MerkleError::RootMismatch(computed, *root));
    }
    Ok(())
}

/// Verify that `txid` is included at `proof.index` under `root`.
///
/// NC target: `tools/nc_c6.py` disables the leaf binding here, requiring the
/// substituted-leaf vector to go red.
pub fn verify_tx_inclusion(
    root: &Hash,
    txid: &Hash,
    proof: &MerkleProof,
) -> Result<(), MerkleError> {
    let expected_leaf = leaf_hash(txid);
    if expected_leaf != proof.leaf {
        return Err(MerkleError::LeafMismatch);
    }
    verify_proof(root, proof)
}

/// Verify that raw `event` bytes are included at `proof.index` under `root`.
pub fn verify_event_inclusion(
    root: &Hash,
    event: &[u8],
    proof: &MerkleProof,
) -> Result<(), MerkleError> {
    let expected_leaf = event_leaf_hash(event);
    if expected_leaf != proof.leaf {
        return Err(MerkleError::LeafMismatch);
    }
    verify_proof(root, proof)
}
