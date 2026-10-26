//! AuraLite-Crypto account model and state key layout (C7).
//!
//! This is the **first real API** of `aura-state`. It fixes the account value
//! ([`Account`]) and the byte layout of state keys ([`StateKey`],
//! [`KeySpace`]); the versioned Merkle tree, proofs and persistence remain the
//! job of `STATE_PLAN` S1–S7. C7 is deliberately a *layout* phase: no storage
//! backend, no root computation, no fake `Ok(())`.
//!
//! Invariants:
//! * five known key namespaces (`0x01`–`0x05`); everything else — including
//!   `0x00` and `0x06`–`0xFF` — is **reserved** and cannot be decoded into a
//!   [`StateKey`];
//! * every namespace has one fixed key length, so two accounts can never
//!   collide and a key cannot alias another namespace;
//! * a contract may write only under its own `0x03 || contract || slot`
//!   prefix, checked by [`assert_contract_may_write`];
//! * [`StateKey::digest`] is domain-separated with an explicit length, ready
//!   for the JMT path hashing in S1;
//! * all account arithmetic is checked (`u128` balance, `u64` nonce).
//!
//! Full specification: `docs/spec/account-model.md`.
//! Golden vectors: `tools/gen_c7_vectors.py` (independent Python).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod account;
mod error;
mod key;

pub use account::Account;
pub use error::StateError;
pub use key::{
    KeySpace, MAX_STATE_KEY_LEN, STATE_KEY_CONTEXT, StateKey, assert_contract_may_write,
};
