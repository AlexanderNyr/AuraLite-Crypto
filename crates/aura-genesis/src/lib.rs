//! AuraLite-Crypto deterministic genesis document (C8).
//!
//! `aura-genesis` turns a genesis **config file** (JSON) into one canonical
//! binary document and a set of deterministic commitments:
//!
//! * [`Genesis::hash`] — the genesis id, domain-separated with
//!   [`aura_types::Domain::Genesis`] and the chain-id;
//! * [`Genesis::validator_set_hash`] — the value that fills C5's
//!   `validator_set_hash` header field (previously caller-supplied);
//! * [`Genesis::allocation_root`] — a Merkle commitment over C7 account
//!   state keys/values, using the C6 tree shape. This is **not** the JMT
//!   `state_root` (S1), just a genesis allocation commitment.
//!
//! JSON is parsed with `serde_json` and used **only as a config format** —
//! exactly the exception allowed by `ARCHITECTURE.md` ("serde is for
//! RPC/configs only"). Consensus bytes always come from `aura-codec`.
//! Object key order and whitespace never affect the hash; the validator
//! array order is consensus-significant and does.
//!
//! Specification: `docs/spec/genesis-format.md`.
//! Golden vectors: `tools/gen_c8_vectors.py` (independent Python).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod json;
mod types;

pub use error::GenesisError;
pub use json::{from_json_file, from_json_str};
pub use types::{
    ALLOCATION_LEAF_CONTEXT, Allocation, GENESIS_CONTEXT, Genesis, VALIDATOR_SET_CONTEXT, Validator,
};
