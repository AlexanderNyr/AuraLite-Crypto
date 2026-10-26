//! AuraLite-Crypto block and header format (C5).
//!
//! `Block = Header || Body`. The header carries the protocol version, the
//! chain-id, the height, the parent hash, three roots (`tx_root`,
//! `state_root`, `validator_set_hash`), a consensus timestamp, the proposer,
//! an opaque QC, gas used and the base fee. The body is a canonically sorted
//! list of [`aura_tx::Tx`].
//!
//! [`Block::block_hash`] hashes the **header only** (the `tx_root` commits to
//! the body), with chain-id and `Domain::Block` separation from C3.
//! [`compute_tx_root`] is a binary BLAKE3 Merkle root over txids; C6 adds
//! proofs over exactly this tree.
//!
//! `state_root`, `validator_set_hash` and `qc` are **supplied by the caller**
//! at C5: state (S1), validator sets (B phases) and quorum certificates
//! (B phase) are not implemented yet. C5 fixes their wire representation and
//! their validation, not their computation. Honest placeholders, not fake
//! values.
//!
//! Full byte-level specification: `docs/spec/block-format.md`.
//! Golden vectors: `tools/gen_c5_vectors.py` (independent Python).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod block;
mod body;
mod error;
mod header;
mod merkle;
mod proof;

pub use block::Block;
pub use body::{Body, MAX_BLOCK_TXS};
pub use error::{BlockError, MerkleError};
pub use header::{BLOCK_HASH_PAYLOAD_CONTEXT, Header, HeaderParams, MAX_QC_BYTES};
pub use merkle::{LEAF_CONTEXT, NODE_CONTEXT, compute_tx_root, leaf_hash, node_hash};
pub use proof::{
    EVENT_LEAF_CONTEXT, MAX_PROOF_DEPTH, MerkleProof, MerkleTree, build_event_tree,
    build_tree_from_leaf_hashes, build_tx_tree, event_leaf_hash, tree_depth,
    verify_event_inclusion, verify_proof, verify_tx_inclusion,
};
