//! `aura-consensus` — BFT-PoS consensus: B0 baseline + B1 proposer & block
//! signature + B2 BLS12-381 & QC + B3 BFT finality + B4 validator set + B5 VRF
//! leader election + B6 slashing & evidence + B7 rewards + B8 fork-choice &
//! deep-reorg defence + B9 on-chain governance + B10 safety evidence & TLA+ model.
//!
//! All `CONSENSUS_PLAN` phases (`B0`–`B10`) are complete.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(rust_2018_idioms)]

pub mod baseline;
pub mod bls;
pub mod error;
pub mod finality;
pub mod fork_choice;
pub mod governance;
pub mod proposer;
pub mod qc;
pub mod rewards;
pub mod safety_model;
pub mod signature;
pub mod slashing;
pub mod validator_set;
pub mod vrf_leader;
