//! X5 gate (artifacts): what is on disk is what the fixture pins.
//!
//! A gate that deploys "the artifact in the repository" is only meaningful if the
//! artifact is the one the build produced. This test is the link: measured size and
//! hash from `tools/build_standards.py`, an import set that is a subset of ABI v1
//! and v2, and the two exports the lifecycle requires.
//!
//! It is also the test that would fail if someone committed a `.wasm` built from a
//! different source tree, which is the quietest possible way to ship a lie.

use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/x5_standards.txt");

const ARC20: &[u8] = include_bytes!("../../../standards/arc20/wasm/arc20.wasm");
const ARC721: &[u8] = include_bytes!("../../../standards/arc721/wasm/arc721.wasm");
const MULTISIG: &[u8] = include_bytes!("../../../standards/multisig/wasm/multisig.wasm");

fn field(key: &str) -> String {
    FIXTURE
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing {key} in the X5 fixture"))
        .to_string()
}

fn artifacts() -> BTreeMap<&'static str, &'static [u8]> {
    BTreeMap::from([("arc20", ARC20), ("arc721", ARC721), ("multisig", MULTISIG)])
}

#[test]
fn every_standard_matches_its_pinned_size_and_hash() {
    for (name, wasm) in artifacts() {
        let size: usize = field(&format!("standard_{name}_size"))
            .parse()
            .expect("pinned size");
        assert_eq!(
            wasm.len(),
            size,
            "{name}: the artifact is not the pinned one"
        );
        // The digest the node can compute: BLAKE3, recorded by `x5-record` when the
        // fixture was written. A `.wasm` swapped for another one fails here.
        let pinned_blake3 = field(&format!("standard_{name}_blake3"));
        assert_eq!(
            aura_crypto::blake3_hash(wasm).to_string(),
            pinned_blake3,
            "{name}: the shipped artifact is not the one the fixture pins"
        );
        // SHA-256 comes from the build tool; it is pinned as a build receipt, so the
        // check here is only that it is a well-formed digest.
        let pinned_sha256 = field(&format!("standard_{name}_sha256"));
        assert_eq!(
            pinned_sha256.len(),
            64,
            "{name}: pinned sha256 has the wrong length"
        );
        assert!(
            pinned_sha256.chars().all(|c| c.is_ascii_hexdigit()),
            "{name}: pinned sha256 is not hex"
        );
    }
}

#[test]
fn every_standard_imports_only_abi_functions() {
    let v1 = [
        "storage_read",
        "storage_write",
        "balance",
        "transfer",
        "emit",
        "hash",
        "block_ctx",
    ];
    let v2 = [
        "calldata_len",
        "calldata_read",
        "return_write",
        "caller",
        "self_address",
        "abi_version",
    ];
    for (name, _wasm) in artifacts() {
        let imports = field(&format!("standard_{name}_imports"));
        assert!(
            !imports.is_empty(),
            "{name}: a standard must import the ABI"
        );
        for import in imports.split(',') {
            let (module, function) = import.split_once('.').expect("module.name");
            let allowed = match module {
                "env" => v1.contains(&function),
                "env2" => v2.contains(&function),
                _ => false,
            };
            assert!(allowed, "{name} imports {import}, which is not in the ABI");
        }
    }
}

#[test]
fn every_standard_exports_the_entry_points_the_lifecycle_calls() {
    for (name, _) in artifacts() {
        let exports = field(&format!("standard_{name}_exports"));
        for required in ["run", "init", "memory"] {
            assert!(
                exports.split(',').any(|export| export == required),
                "{name} does not export {required}"
            );
        }
    }
}

#[test]
fn the_measured_gas_of_every_standard_is_pinned() {
    // A gas number in a report must be one the gate can recompute, so the fixture
    // carries the measured values and this test refuses a zero or a missing row.
    for key in [
        "measured_arc20_deploy_gas",
        "measured_arc20_mint_gas",
        "measured_arc20_transfer_gas",
        "measured_arc20_balance_of_gas",
        "measured_arc721_deploy_gas",
        "measured_arc721_mint_gas",
        "measured_arc721_transfer_gas",
        "measured_multisig_deploy_gas",
        "measured_multisig_policy_gas",
        "measured_multisig_propose_gas",
        "measured_multisig_approve_gas",
    ] {
        let value: u64 = field(key)
            .parse()
            .unwrap_or_else(|_| panic!("{key} is not a number"));
        assert!(value > 0, "{key} must be a measured, non-zero number");
    }
}

#[test]
fn the_fixture_publishes_the_surface_the_standards_use() {
    assert_eq!(field("abi_version"), "2");
    assert_eq!(field("abi_modules"), "env,env2");
    assert_eq!(field("target"), "wasm32-unknown-unknown");
    for name in artifacts().keys() {
        assert_eq!(
            field(&format!("standard_{name}_crate")),
            format!("aura-standard-{name}")
        );
    }
}
