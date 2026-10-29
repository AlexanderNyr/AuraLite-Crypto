//! AuraLite-Crypto BFT consensus building blocks.
//!
//! B0 establishes an honest baseline, B1 adds PoA proposer rotation and block
//! signatures, B2 adds BLS/QC, and B3 adds the finality pipeline. Validator-set
//! updates remain an isolated B4 draft and are not part of the live B3 pipeline.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod baseline;
pub mod bls;
pub mod error;
pub mod finality;
pub mod proposer;
pub mod qc;
pub mod signature;
pub mod validator_set;
