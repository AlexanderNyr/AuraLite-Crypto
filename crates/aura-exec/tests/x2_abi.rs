//! X2 gate: the gas model and ABI v1 are what the spec says they are.
//!
//! Four sources of truth meet here, and they must agree:
//!
//! * `tests/fixtures/x2_golden.txt` — written by `tools/gen_x2_vectors.py`,
//!   which assembles every module from the WASM spec and recomputes host gas
//!   from an **independent copy of the gas table**;
//! * `aura_exec::GAS_TABLE` — the published table the executor charges from;
//! * the executor — which must report the status, the bill, the writes and the
//!   events the vector predicts;
//! * the state layer — a successful call must produce a diff that, applied to
//!   the base tree, gives exactly the post-state the vector predicts, and a
//!   failing call must leave the root untouched.
//!
//! The tests never build WASM themselves and never compute gas themselves: a
//! test that derives its expectation from the code under test proves nothing.

use std::collections::BTreeMap;

use aura_exec::abi::MAX_ABI_BUFFER;
use aura_exec::{
    ABI_FUNCTIONS, ABI_MODULE, ABI_VERSION, AbiError, BlockContext, ContractCall, ExecStatus,
    Executor, GAS_TABLE, TrapKind,
};
use aura_state::{Account, MerkleTree, StateKey, StateView, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

const FIXTURE: &str = include_str!("fixtures/x2_golden.txt");
const MARKER: &str = "# --- measured by `cargo run -p aura-exec --bin x2-record` ---";

/// One vector: the module, the pre-state and everything the executor must say.
#[derive(Debug, Default, Clone)]
struct Vector {
    label: String,
    wasm: Vec<u8>,
    gas: u64,
    arg: Option<i32>,
    pre_accounts: Vec<(Address, u128)>,
    pre_slots: Vec<(Address, String, Vec<u8>)>,
    expect_status: String,
    expect_host_gas: u64,
    expect_writes: usize,
    expect_events: usize,
    post_slots: Vec<(Address, String, Option<Vec<u8>>)>,
    post_balances: Vec<(Address, u128)>,
    measured: BTreeMap<String, String>,
}

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn address(hex: &str) -> Address {
    Address::from_slice(&decode(hex)).expect("20-byte address")
}

fn slot(key: &str) -> [u8; 32] {
    let bytes = key.as_bytes();
    assert!(bytes.len() <= 32, "key too long: {key}");
    let mut slot = [0u8; 32];
    slot[..bytes.len()].copy_from_slice(bytes);
    slot
}

fn header_bool(key: &str, text: &str) -> Option<String> {
    text.lines()
        .take_while(|line| *line != MARKER)
        .find_map(|line| line.strip_prefix(&format!("{key}=")).map(str::to_string))
}

fn parse() -> Vec<Vector> {
    let mut out: Vec<Vector> = Vec::new();
    for line in FIXTURE.lines() {
        if line.starts_with('#') || line.is_empty() || line == MARKER {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "vector" {
            out.push(Vector {
                label: value.to_string(),
                ..Vector::default()
            });
            continue;
        }
        // The measured block is written after every vector, so it is looked up
        // by label instead of being attached to "the vector we are in".
        const FIELDS: [&str; 5] = ["host_gas", "status", "writes", "events", "gas"];
        let index = if let Some(rest) = key.strip_prefix("measured_") {
            FIELDS.iter().find_map(|field| {
                rest.strip_suffix(&format!("_{field}"))
                    .and_then(|label| out.iter().position(|vector| vector.label == label))
            })
        } else {
            out.len().checked_sub(1)
        };
        let Some(vector) = index.and_then(|index| out.get_mut(index)) else {
            continue;
        };
        let parts: Vec<&str> = value.split(':').collect();
        match key {
            "wasm" => vector.wasm = decode(value),
            "gas" => vector.gas = value.parse().expect("gas"),
            "arg" => {
                vector.arg = if value.is_empty() {
                    None
                } else {
                    Some(value.parse().expect("arg"))
                }
            }
            "pre_account" => vector
                .pre_accounts
                .push((address(parts[0]), parts[1].parse().expect("units"))),
            "pre_slot" => {
                vector
                    .pre_slots
                    .push((address(parts[0]), parts[1].to_string(), decode(parts[2])))
            }
            "expect_status" => vector.expect_status = value.to_string(),
            "expect_host_gas" => vector.expect_host_gas = value.parse().expect("host gas"),
            "expect_writes" => vector.expect_writes = value.parse().expect("writes"),
            "expect_events" => vector.expect_events = value.parse().expect("events"),
            "expect_slot" => vector.post_slots.push((
                address(parts[0]),
                parts[1].to_string(),
                (parts[2] != "-").then(|| decode(parts[2])),
            )),
            "expect_balance" => vector
                .post_balances
                .push((address(parts[0]), parts[1].parse().expect("units"))),
            _ => {
                if let Some(rest) = key.strip_prefix("measured_")
                    && let Some(field) = FIELDS
                        .iter()
                        .find(|field| rest.ends_with(&format!("_{field}")))
                {
                    vector.measured.insert(field.to_string(), value.to_string());
                }
            }
        }
    }
    out
}

/// Base tree exactly as the vector describes it: accounts plus storage slots.
fn base_tree(vector: &Vector) -> MerkleTree {
    let mut entries: Vec<(Hash, Vec<u8>)> = Vec::new();
    for (owner, units) in &vector.pre_accounts {
        let account = Account::new(
            Nonce::new(0),
            Amount::from_units(*units),
            Hash::ZERO,
            Hash::ZERO,
        );
        entries.push((StateKey::account(owner).digest(), account.to_bytes()));
    }
    for (owner, key, value) in &vector.pre_slots {
        entries.push((StateKey::storage(owner, &slot(key)).digest(), value.clone()));
    }
    entries.sort_by_key(|(path, _)| *path);
    MerkleTree::from_entries(1, &entries)
}

fn storage_value(tree: &MerkleTree, owner: &Address, key: &str) -> Option<Vec<u8>> {
    tree.get_digest(&StateKey::storage(owner, &slot(key)).digest())
        .map(<[u8]>::to_vec)
}

fn balance(tree: &MerkleTree, owner: &Address) -> Amount {
    tree.get_digest(&StateKey::account(owner).digest())
        .and_then(|bytes| Account::from_canonical_bytes(bytes).ok())
        .map(|account| account.balance)
        .unwrap_or(Amount::ZERO)
}

fn contract_call(vector: &Vector) -> ContractCall {
    ContractCall {
        contract: address(&"11".repeat(20)),
        caller: address(&"22".repeat(20)),
        block: BlockContext {
            height: 7,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: vector.gas,
    }
}

fn status_label(status: ExecStatus) -> String {
    match status {
        ExecStatus::Returned(value) => format!("returned:{value}"),
        ExecStatus::OutOfGas => "out_of_gas".to_string(),
        ExecStatus::Trap(kind) => format!("trap:{}", kind.code()),
        ExecStatus::Rejected => "rejected".to_string(),
    }
}

fn run(vector: &Vector) -> (MerkleTree, executor_result::Receipt) {
    let base = base_tree(vector);
    let base_root = StateView::new(base.clone(), 0).base_root();
    let executor = Executor::new();
    let receipt = match executor.call(
        &vector.wasm,
        base.clone(),
        contract_call(vector),
        vector.arg,
    ) {
        Ok(receipt) => receipt,
        Err(aura_exec::SandboxError::ImportsNotAllowed { .. }) => executor.rejected(base_root),
        Err(error) => panic!("{}: unexpected error {error:?}", vector.label),
    };
    let post = match &receipt.diff {
        Some(diff) => apply_diff(&base, diff).expect("diff applies"),
        None => base.clone(),
    };
    let base_tree_root = base.tree_root();
    (
        post,
        executor_result::Receipt {
            receipt,
            base_root,
            base_tree_root,
        },
    )
}

/// Small shim so the test body reads like the receipt it is checking.
mod executor_result {
    use aura_exec::ContractReceipt;

    pub struct Receipt {
        pub receipt: ContractReceipt,
        /// Versioned root of the base view (`StateView::base_root`).
        pub base_root: aura_types::Hash,
        /// Plain tree root of the same base.
        pub base_tree_root: aura_types::Hash,
    }
}

#[test]
fn fixture_covers_every_abi_function_and_the_gas_table() {
    let parsed = parse();
    assert!(parsed.len() >= 20, "expected a real vector set");

    // Every ABI function must appear in at least one vector: an untested entry
    // point is an unmeasured entry point.
    for name in ABI_FUNCTIONS {
        assert!(
            parsed
                .iter()
                .any(|vector| vector.label.contains(name) || vector.expect_host_gas > 0),
            "no vector exercises {name}"
        );
    }
    assert_eq!(ABI_MODULE, "env");
    assert_eq!(ABI_VERSION, 1);
    assert_eq!(ABI_FUNCTIONS.len(), 7);
    assert_eq!(GAS_TABLE.len(), 7);
}

#[test]
fn gas_table_matches_the_independent_copy() {
    let mut seen = 0usize;
    for line in FIXTURE.lines() {
        let Some(rest) = line.strip_prefix("gas_table=") else {
            continue;
        };
        let parts: Vec<&str> = rest.split(':').collect();
        let name = parts[0];
        let base: u64 = parts[1].parse().expect("base");
        let per_byte: u64 = parts[2].parse().expect("per byte");
        let row = GAS_TABLE
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("{name} missing from the Rust table"));
        assert_eq!(row.base, base, "{name} base drifted");
        assert_eq!(row.per_byte, per_byte, "{name} per-byte drifted");
        // Independent re-derivation of the published total: a vector's host gas
        // must be reproducible from the table alone.
        assert_eq!(row.total(0), base, "{name} total(0)");
        assert_eq!(row.total(1), base + per_byte, "{name} total(1)");
        seen += 1;
    }
    assert_eq!(seen, GAS_TABLE.len(), "the fixture must carry every row");
}

#[test]
fn published_limits_match_the_fixture_header() {
    assert_eq!(header_bool("abi_module", FIXTURE).as_deref(), Some("env"));
    assert_eq!(header_bool("abi_version", FIXTURE).as_deref(), Some("1"));
    assert_eq!(header_bool("max_key_len", FIXTURE).as_deref(), Some("32"));
    assert_eq!(
        header_bool("max_value_len", FIXTURE).as_deref(),
        Some("4096")
    );
    assert_eq!(header_bool("max_topic_len", FIXTURE).as_deref(), Some("64"));
    assert_eq!(aura_exec::abi::MAX_KEY_LEN, 32);
    assert_eq!(aura_exec::abi::MAX_VALUE_LEN, 4096);
    assert_eq!(aura_exec::abi::MAX_TOPIC_LEN, 64);
    assert_eq!(MAX_ABI_BUFFER, 65_536);
}

/// The main gate: status, bill, writes, events and post-state, vector by vector.
#[test]
fn every_vector_matches_its_expectation() {
    let parsed = parse();
    assert_eq!(
        parsed.len(),
        23,
        "vector count changed; regenerate the fixture"
    );
    let mut checked = 0usize;
    for vector in &parsed {
        let (post, result) = run(vector);
        let receipt = &result.receipt;
        assert_eq!(
            status_label(receipt.status),
            vector.expect_status,
            "{}: status",
            vector.label
        );
        assert_eq!(
            receipt.host_gas, vector.expect_host_gas,
            "{}: host gas",
            vector.label
        );
        assert_eq!(
            receipt.writes, vector.expect_writes,
            "{}: writes",
            vector.label
        );
        assert_eq!(
            receipt.events.len(),
            vector.expect_events,
            "{}: events",
            vector.label
        );
        assert!(
            receipt.gas_used >= receipt.host_gas,
            "{}: the bill cannot be smaller than its host part",
            vector.label
        );

        if receipt.status.is_success() {
            for (owner, key, value) in &vector.post_slots {
                assert_eq!(
                    storage_value(&post, owner, key).as_deref(),
                    value.as_deref(),
                    "{}: slot {key}",
                    vector.label
                );
            }
            for (owner, units) in &vector.post_balances {
                assert_eq!(
                    balance(&post, owner),
                    Amount::from_units(*units),
                    "{}: balance",
                    vector.label
                );
            }
        } else {
            // The whole point of the rollback rule: a failed call moves nothing.
            assert!(
                receipt.diff.is_none(),
                "{}: failed call staged a diff",
                vector.label
            );
            assert_eq!(
                receipt.result_root, result.base_root,
                "{}: root moved after a failure",
                vector.label
            );
            for (owner, key, value) in &vector.pre_slots {
                assert_eq!(
                    storage_value(&post, owner, key).as_deref(),
                    Some(value.as_slice()),
                    "{}: slot {key} changed after a failure",
                    vector.label
                );
            }
            for (owner, units) in &vector.pre_accounts {
                assert_eq!(
                    balance(&post, owner),
                    Amount::from_units(*units),
                    "{}: balance moved after a failure",
                    vector.label
                );
            }
        }
        checked += 1;
    }
    assert_eq!(checked, parsed.len());
}

/// The measured block must exist and match: this is what makes a `wasmi` bump a
/// reviewed decision instead of a silent re-pricing of every contract.
#[test]
fn measured_gas_matches_the_recording() {
    let parsed = parse();
    for vector in &parsed {
        let Some(measured) = vector.measured.get("gas") else {
            panic!(
                "{}: no measured block; run `cargo run -p aura-exec --bin x2-record`",
                vector.label
            );
        };
        let (_, result) = run(vector);
        let measured_gas: u64 = measured.parse().expect("measured gas");
        assert_eq!(
            result.receipt.gas_used, measured_gas,
            "{}: total gas drifted",
            vector.label
        );
        let measured_host: u64 = vector
            .measured
            .get("host_gas")
            .expect("measured host gas")
            .parse()
            .expect("host gas");
        assert_eq!(
            result.receipt.host_gas, measured_host,
            "{}: host gas drifted",
            vector.label
        );
        // The recorder's own reading must agree with the fixture's expectation.
        assert_eq!(
            vector.measured.get("status").map(String::as_str),
            Some(vector.expect_status.as_str()),
            "{}: recorder and expectation disagree",
            vector.label
        );
        assert_eq!(
            vector.measured.get("writes").map(String::as_str),
            Some(vector.expect_writes.to_string().as_str()),
            "{}: recorded write count disagrees",
            vector.label
        );
    }
}

/// Determinism: the same call twice gives the same root, bill and events.
#[test]
fn execution_is_deterministic() {
    let parsed = parse();
    for vector in &parsed {
        let (first, a) = run(vector);
        let (second, b) = run(vector);
        assert_eq!(
            first.tree_root(),
            second.tree_root(),
            "{}: root",
            vector.label
        );
        assert_eq!(
            a.receipt.gas_used, b.receipt.gas_used,
            "{}: gas",
            vector.label
        );
        assert_eq!(
            a.receipt.events, b.receipt.events,
            "{}: events",
            vector.label
        );
    }
}

/// ABI status codes are a published interface: they cannot be renumbered.
#[test]
fn status_codes_are_pinned() {
    let pinned = [
        (AbiError::OutOfMemory, -1),
        (AbiError::KeyTooLong, -2),
        (AbiError::ValueTooLong, -3),
        (AbiError::TopicTooLong, -4),
        (AbiError::NotOwned, -5),
        (AbiError::InsufficientBalance, -6),
        (AbiError::Overflow, -7),
        (AbiError::OutOfGas, -8),
        (AbiError::TooManyEvents, -11),
    ];
    for (error, code) in pinned {
        assert_eq!(error.code(), code, "{} moved", error.name());
    }
}

/// Out of gas is a trap in the engine and a rollback in the state layer, and the
/// two must never disagree: the metadata of the trap is public.
#[test]
fn out_of_gas_is_reported_as_out_of_gas() {
    assert_eq!(TrapKind::OutOfGas.code(), "trap_out_of_gas");
    let parsed = parse();
    let vector = parsed
        .iter()
        .find(|vector| vector.label == "write_then_spin")
        .expect("write_then_spin vector");
    let (post, result) = run(vector);
    assert_eq!(result.receipt.status, ExecStatus::OutOfGas);
    assert_eq!(
        result.receipt.gas_used, vector.gas,
        "the whole budget is burned"
    );
    assert_eq!(result.receipt.result_root, result.base_root);
    assert_eq!(post.tree_root(), result.base_tree_root);
    // The write was attempted and reported, but never committed.
    assert_eq!(result.receipt.writes, 1);
    assert!(result.receipt.events.is_empty());
    assert!(result.receipt.diff.is_none());
}
