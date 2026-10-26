//! Integration-side check that the workspace actually exports the C0 identity.
//! C1 and later crates will consume these constants from outside the crate.

use aura_types::{CHAIN_ID, DENOM, NETWORK_NAME, is_our_chain, units_per_coin};

#[test]
fn workspace_exports_identity() {
    assert_eq!(CHAIN_ID, "auralite-test-1");
    assert_eq!(DENOM, "ALCRY");
    assert_eq!(NETWORK_NAME, "AuraLite-Crypto Network");
    assert!(is_our_chain(CHAIN_ID));
    assert!(units_per_coin() > 0);
}
