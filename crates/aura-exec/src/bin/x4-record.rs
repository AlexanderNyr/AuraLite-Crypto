//! Record the **measured** gas of the X4 reference contracts.
//!
//! Writes a `measured_*` block into `crates/aura-exec/tests/fixtures/x4_contracts.txt`
//! after the marker line, so the numbers a test asserts are the numbers this
//! recorder measured through the real `Lifecycle` — not numbers typed into the
//! test. Re-running it must reproduce the block byte for byte (the evidence
//! generator checks exactly that).
//!
//! ```text
//! cargo run -p aura-exec --bin x4-record
//! ```

use std::path::PathBuf;

use aura_exec::lifecycle::Lifecycle;
use aura_exec::{BlockContext, ContractCall};
use aura_state::{Account, MerkleTree, StateKey, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

const STORE: &[u8] = include_bytes!("../../../../examples/store/wasm/store.wasm");
const KV: &[u8] = include_bytes!("../../../../examples/kv/wasm/kv.wasm");
const EVENTS: &[u8] = include_bytes!("../../../../examples/events/wasm/events.wasm");

const MARKER: &str = "# --- measured by `cargo run -p aura-exec --bin x4-record` ---";

const CONTRACT: [u8; 20] = [0x11; 20];
const CALLER: [u8; 20] = [0x22; 20];

fn address(bytes: [u8; 20]) -> Address {
    Address::from_slice(&bytes).expect("20-byte address")
}

fn call() -> ContractCall {
    ContractCall {
        contract: address(CONTRACT),
        caller: address(CALLER),
        block: BlockContext {
            height: 7,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: 1_000_000,
    }
}

fn base() -> MerkleTree {
    let caller = Account::new(
        Nonce::new(0),
        Amount::from_units(10_000),
        Hash::ZERO,
        Hash::ZERO,
    );
    MerkleTree::from_entries(
        1,
        &[(
            StateKey::account(&address(CALLER)).digest(),
            caller.to_bytes(),
        )],
    )
}

fn main() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("x4_contracts.txt");
    let text = std::fs::read_to_string(&path).expect("fixture");
    let head: Vec<&str> = text.lines().take_while(|line| *line != MARKER).collect();

    let lifecycle = Lifecycle::new();
    let base = base();

    let deployed = lifecycle
        .deploy(&base, call(), STORE.to_vec(), Amount::ZERO, None)
        .expect("deploy store");
    let tree = apply_diff(&base, deployed.diff.as_ref().expect("diff")).expect("diff");
    let called = lifecycle
        .call_stored(&tree, call(), None)
        .expect("call store");

    let kv = lifecycle
        .deploy(&base, call(), KV.to_vec(), Amount::ZERO, Some(5))
        .expect("deploy kv");
    let kv_tree = apply_diff(&base, kv.diff.as_ref().expect("diff")).expect("diff");
    let kv_call = lifecycle
        .call_stored(&kv_tree, call(), Some(9))
        .expect("call kv");
    let upgraded = lifecycle
        .upgrade(&kv_tree, call(), 1, KV.to_vec())
        .expect("upgrade kv");

    let events = lifecycle
        .deploy(&base, call(), EVENTS.to_vec(), Amount::ZERO, None)
        .expect("deploy events");
    let events_tree = apply_diff(&base, events.diff.as_ref().expect("diff")).expect("diff");
    let events_call = lifecycle
        .call_stored(&events_tree, call(), None)
        .expect("call events");

    let measured = [
        ("deploy_store_gas", deployed.gas_used),
        ("deploy_store_code_gas", deployed.code_gas),
        ("call_store_gas", called.gas_used),
        ("deploy_kv_gas", kv.gas_used),
        ("call_kv_gas", kv_call.gas_used),
        ("upgrade_kv_gas", upgraded.gas_used),
        ("deploy_events_gas", events.gas_used),
        ("call_events_gas", events_call.gas_used),
    ];

    let mut out = head.join("\n");
    out.push('\n');
    out.push_str(MARKER);
    out.push('\n');
    for (name, gas) in measured {
        out.push_str(&format!("measured_{name}={gas}\n"));
    }
    std::fs::write(&path, out).expect("write fixture");
    for (name, gas) in measured {
        println!("measured_{name}={gas}");
    }
}
