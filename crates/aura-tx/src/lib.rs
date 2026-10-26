//! AuraLite-Crypto canonical transaction format (C4).
//!
//! A transaction is `Tx = UnsignedTx || Signature`. The unsigned part carries
//! the ten protocol fields from `CORE_PLAN.md` C4 plus a `codec_version` from
//! `ARCHITECTURE.md`:
//!
//! ```text
//! codec_version, chain_id, signer, nonce, kind, payload,
//! gas_limit, max_fee, tip, expiry
//! ```
//!
//! The signature is an **envelope**, not a bare 64-byte value: it also carries
//! the algorithm tag and the 32-byte public key. Ed25519 cannot recover a
//! public key from a signature, so the key must travel with the transaction;
//! [`signer_address`] binds that key to the 20-byte `signer` field.
//!
//! Invariants:
//! * one value → one byte string (C2 [`aura_codec::decode_canonical`]);
//! * `signing_bytes()` = `SIGNING_CONTEXT || canonical(UnsignedTx)`, signed
//!   through C3's domain-separated `aura_crypto::sign(Domain::Tx, chain_id)`;
//! * replay protection: `validate()` takes the expected chain-id and rejects
//!   a transaction minted for `auralite-1` on `auralite-test-1`;
//! * no clock, no randomness, no floating point — `expiry` and `height` are
//!   consensus heights supplied by the caller.
//!
//! Full field/wire specification: `docs/spec/tx-format.md`.
//! Golden vectors: `tools/gen_c4_vectors.py` (independent Python).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod kind;
mod signature;
mod tx;

pub use error::TxError;
pub use kind::TxKind;
pub use signature::{SIG_ALGO_ED25519, Signature, SignatureAlgo};
pub use tx::{
    ADDRESS_CONTEXT, MAX_TX_PAYLOAD, SIGNING_CONTEXT, TXID_CONTEXT, Tx, TxParams, UnsignedTx,
    signer_address,
};
