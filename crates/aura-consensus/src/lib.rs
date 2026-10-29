//! `aura-consensus` — BFT-PoS consensus with VRF (HotStuff-like, 2f+1 QC).
//!
//! C0 creates the workspace skeleton only. **B0** lands the baseline
//! measurement. **B1** lands [`proposer`] and [`signature`]. **B2** lands
//! [`bls`] and [`qc`]. **B3** lands [`finality`]. **B4** lands
//! [`validator_set`]. **B5** will add VRF leader election.
//!
//! # Modules
//!
//! - [`baseline`] — the B0–B4 measurement: survey, refusal, report.
//! - [`bls`] — BLS12-381 signing, verification, aggregation (B2).
//! - [`error`] — consensus error types.
//! - [`finality`] — BFT finality: 3-chain commit, view change, timeouts,
//!   nil blocks (B3).
//! - [`proposer`] — deterministic round-robin proposer (B1).
//! - [`qc`] — quorum certificate aggregation (B2).
//! - [`signature`] — simple Ed25519 block signature (B1).
//! - [`validator_set`] — staking contract, epoch, enter/exit queue, set
//!   update (B4).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod validator_set;
