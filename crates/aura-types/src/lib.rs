//! AuraLite-Crypto canonical primitives.
//!
//! C0 locked the network identity constants. C1 adds the canonical value
//! types that every later crate consumes: [`Hash`], [`Address`], [`Amount`],
//! [`Denom`], [`ChainId`], [`Nonce`], [`Height`], [`Gas`] and [`Domain`],
//! plus the deterministic [`CanonicalMap`] alias.
//!
//! Deliberately **no** serialization format yet — that is `aura-codec` in C2 —
//! and no hashing/signing yet — that is `aura-crypto` in C3. This crate still
//! has zero external dependencies.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod address;
mod amount;
mod hash;
mod primitives;

pub use address::{ADDRESS_LEN, ADDRESS_VERSION, Address, AddressError};
pub use amount::{Amount, Denom};
pub use hash::{HASH_LEN, Hash, HashError};
pub use primitives::{CanonicalMap, ChainId, Domain, Gas, Height, Nonce, PrimitiveError};

/// Human-readable network name (Q1).
pub const NETWORK_NAME: &str = "AuraLite-Crypto Network";

/// Stage 1 chain identifier. A signature from any other chain is invalid (D5).
pub const CHAIN_ID: &str = "auralite-test-1";

/// chain-id reserved for mainnet. It must never be active in Stage 1 (Q5).
pub const MAINNET_CHAIN_ID: &str = "auralite-1";

/// Native asset ticker (Q1 / `docs/TICKER_AUDIT.md`).
pub const DENOM: &str = "ALCRY";

/// Name of the smallest unit, 10^-18 ALCRY.
pub const SMALLEST_UNIT: &str = "aalcry";

/// Number of decimal places of the native asset.
pub const DECIMALS: u8 = 18;

/// bech32 human-readable part for addresses (`aura1...`).
pub const BECH32_PREFIX: &str = "aura";

/// Consensus family locked by Q2.
pub const CONSENSUS: &str = "bft-pos";

/// Execution engine locked by Q3. EVM (`revm`) is the optional X9 only.
pub const VM: &str = "wasm";

/// Protocol version. Any format change requires a new phase, a migration test
/// and a bump here (ROADMAP rule 4, CORE_PLAN C10).
pub const PROTOCOL_VERSION: u32 = 1;

/// Returns true only for the chain this build is allowed to sign for.
///
/// This is deliberately a function rather than an exposed constant comparison
/// in callers: replay protection must have one owner.
pub fn is_our_chain(chain_id: &str) -> bool {
    chain_id == CHAIN_ID
}

/// Number of smallest units in one whole `DENOM` unit.
pub const fn units_per_coin() -> u128 {
    10u128.pow(DECIMALS as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d0_identity_is_locked() {
        assert_eq!(NETWORK_NAME, "AuraLite-Crypto Network");
        assert_eq!(CHAIN_ID, "auralite-test-1");
        assert_eq!(DENOM, "ALCRY");
        assert_eq!(DECIMALS, 18);
        assert_eq!(BECH32_PREFIX, "aura");
        assert_eq!(CONSENSUS, "bft-pos");
        assert_eq!(VM, "wasm");
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn mainnet_chain_id_is_reserved_not_active() {
        assert_ne!(CHAIN_ID, MAINNET_CHAIN_ID);
        assert!(!is_our_chain(MAINNET_CHAIN_ID));
        assert!(!is_our_chain("auralite-2"));
        assert!(is_our_chain(CHAIN_ID));
    }

    #[test]
    fn smallest_unit_matches_decimals() {
        assert_eq!(SMALLEST_UNIT, "aalcry");
        assert_eq!(units_per_coin(), 1_000_000_000_000_000_000);
    }

    #[test]
    fn empty_chain_id_is_rejected() {
        assert!(!is_our_chain(""));
    }

    #[test]
    fn stage1_chain_id_is_a_valid_canonical_chain_id() {
        let parsed = ChainId::new(CHAIN_ID).expect("locked chain-id must be valid");
        assert_eq!(parsed.as_str(), CHAIN_ID);
    }
}
