//! Binary BLAKE3 Merkle root over transaction ids.
//!
//! C5 fixes the tree shape; C6 adds proof generation and verification over
//! exactly these leaves and nodes.

use aura_crypto::blake3_hash;
use aura_tx::Tx;
use aura_types::Hash;

/// Domain tag for a leaf hash.
///
/// NC target: `tools/nc_c5.py` changes this constant and requires the C5
/// golden vectors to go red.
pub const LEAF_CONTEXT: &[u8] = b"AuraLite-Crypto/tx-leaf-v1\x00";

/// Domain tag for an internal node hash.
///
/// NC target: `tools/nc_c5.py` changes this constant and requires the C5
/// golden vectors to go red.
pub const NODE_CONTEXT: &[u8] = b"AuraLite-Crypto/tx-node-v1\x00";

/// Hash one leaf: `BLAKE3(LEAF_CONTEXT || txid)`.
pub fn leaf_hash(txid: &Hash) -> Hash {
    let mut input = Vec::with_capacity(LEAF_CONTEXT.len() + 32);
    input.extend_from_slice(LEAF_CONTEXT);
    input.extend_from_slice(txid.as_bytes());
    blake3_hash(&input)
}

/// Hash one internal node: `BLAKE3(NODE_CONTEXT || left || right)`.
pub fn node_hash(left: &Hash, right: &Hash) -> Hash {
    let mut input = Vec::with_capacity(NODE_CONTEXT.len() + 64);
    input.extend_from_slice(NODE_CONTEXT);
    input.extend_from_slice(left.as_bytes());
    input.extend_from_slice(right.as_bytes());
    blake3_hash(&input)
}

/// Compute the `tx_root` for an ordered slice of transactions.
///
/// Rules: an empty body yields [`Hash::ZERO`]; a single leaf is the root; at
/// each level an odd final node is duplicated before pairing. The result is
/// order-sensitive — that is the C5 DoD property.
///
/// Since C6 both the root and the proofs come from the same
/// [`crate::build_tx_tree`] implementation, so a root and a proof can never
/// disagree about the tree shape.
pub fn compute_tx_root(txs: &[Tx]) -> Hash {
    if txs.is_empty() {
        return Hash::ZERO;
    }
    let txids: Vec<Hash> = txs.iter().map(|tx| tx.txid()).collect();
    crate::proof::build_tx_tree(&txids).root()
}
