//! `aura-wallet` — keys, keystore, offline signing, and multisig (`WALLET_PLAN` W0–W4).
//!
//! The **experimental W1 prototype** currently provides:
//! - [`keystore`], [`derivation`], [`secret`]: BIP-39/32/44-style derivation,
//!   Argon2id/scrypt + AES-256-GCM encrypted keystore, zeroizing secrets.
//! - [`baseline`]: the W0 capability survey of `crates/aura-wallet/src`, the
//!   key-exposure surface audit ("where keys could end up"), threat model notes
//!   (`D-W1`..`D-W4`), and the tree/log secret-material scanner (`NC: key in a log -> gate red`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod baseline;
pub mod derivation;
pub mod keystore;
pub mod secret;
