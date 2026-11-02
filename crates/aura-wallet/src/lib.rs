//! `aura-wallet` — keys, keystore, offline signing, and multisig (`WALLET_PLAN` W0–W4).
//!
//! At **W0 (Baseline)** this crate provides:
//! - [`baseline`]: the W0 capability survey of `crates/aura-wallet/src`, the
//!   key-exposure surface audit ("where keys could end up"), threat model notes
//!   (`D-W1`..`D-W4`), and the tree/log secret-material scanner (`NC: key in a log -> gate red`).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod baseline;
