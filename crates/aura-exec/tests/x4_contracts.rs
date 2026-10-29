//! X4 gate (engine side): the reference contracts, deployed for real.
//!
//! The bytecode under test is the **shipped** artifact from
//! `examples/<name>/wasm/<name>.wasm`, built by `tools/build_contracts.py` from
//! Rust source that only uses `aura-sdk`. Nothing here is asserted about Rust
//! source: each contract is deployed through `Lifecycle::deploy`, called through
//! `Lifecycle::call_stored`, and upgraded through `Lifecycle::upgrade`, exactly
//! as a transaction would.
//!
//! What is asserted, one sentence each:
//!
//! * the artifacts on disk are the ones the fixture pins (`size`), so a test can
//!   never pass against a stale build;
//! * `store` is a counter: a Rust `Slot<u64>` really maps to a 32-byte state key
//!   and really survives between calls;
//! * `kv` takes an argument, migrates storage on upgrade (`migrate(from, to)`),
//!   and keeps its balance;
//! * `events` reads consensus data (`block_ctx`) and writes a node-computed
//!   digest (`hash`) — so the SDK's wrappers are the node's ABI;
//! * a Rust contract pays exactly `DEPLOY_BASE_GAS + CODE_BYTE_GAS * size`,
//!   the same formula as hand-written bytecode;
//! * two runs produce byte-identical state roots (determinism is not an
//!   aspiration here, it is a diff).

use std::collections::BTreeMap;

use aura_crypto::blake3_hash;
use aura_exec::census::{self, ModuleLimits};
use aura_exec::lifecycle::{
    CODE_BYTE_GAS, DEPLOY_BASE_GAS, Lifecycle, LifecycleStatus, account_of, code_of,
};
use aura_exec::{BlockContext, ContractCall, ExecStatus};
use aura_state::{Account, MerkleTree, StateKey, StateView, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

const FIXTURE: &str = include_str!("fixtures/x4_contracts.txt");

const STORE: &[u8] = include_bytes!("../../../examples/store/wasm/store.wasm");
const KV: &[u8] = include_bytes!("../../../examples/kv/wasm/kv.wasm");
const EVENTS: &[u8] = include_bytes!("../../../examples/events/wasm/events.wasm");

const CONTRACT: [u8; 20] = [0x11; 20];
const CALLER: [u8; 20] = [0x22; 20];

fn raw_field(key: &str) -> String {
    FIXTURE
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing {key} in the X4 fixture"))
        .to_string()
}

/// `field("store_size")` reads `contract_store_size`.
fn field(name: &str) -> String {
    raw_field(&format!("contract_{name}"))
}

fn address(bytes: [u8; 20]) -> Address {
    Address::from_slice(&bytes).expect("20-byte address")
}

fn call(gas: u64) -> ContractCall {
    ContractCall {
        contract: address(CONTRACT),
        caller: address(CALLER),
        block: BlockContext {
            height: 7,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: gas,
    }
}

fn base(value: u128) -> MerkleTree {
    let caller = Account::new(
        Nonce::new(0),
        Amount::from_units(value),
        Hash::ZERO,
        Hash::ZERO,
    );
    let entries = vec![(
        StateKey::account(&address(CALLER)).digest(),
        caller.to_bytes(),
    )];
    MerkleTree::from_entries(1, &entries)
}

fn commit(base: &MerkleTree, diff: &aura_state::StateDiff) -> MerkleTree {
    apply_diff(base, diff).expect("diff applies")
}

fn slot(key: &str) -> [u8; 32] {
    let mut slot = [0u8; 32];
    slot[..key.len()].copy_from_slice(key.as_bytes());
    slot
}

fn storage(tree: &MerkleTree, key: &str) -> Option<Vec<u8>> {
    tree.get_digest(&StateKey::storage(&address(CONTRACT), &slot(key)).digest())
        .map(<[u8]>::to_vec)
}

/// Events are encoded by the host as `len(topic) u32 LE || topic || data`, so a
/// test can read them without a decoder struct.
fn event(event: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let len = u32::from_le_bytes(event[..4].try_into().expect("4-byte length")) as usize;
    let topic = event[4..4 + len].to_vec();
    let data = event[4 + len..].to_vec();
    (topic, data)
}

fn measured(key: &str) -> u64 {
    raw_field(&format!("measured_{key}"))
        .parse()
        .expect("measured gas is a number")
}

fn balance_of(tree: &MerkleTree) -> Amount {
    account_of(&StateView::new(tree.clone(), 0), &address(CONTRACT)).balance
}

fn deploy(
    lifecycle: &Lifecycle,
    code: &[u8],
    arg: Option<i32>,
) -> aura_exec::lifecycle::LifecycleReceipt {
    lifecycle
        .deploy(
            &base(10_000),
            call(1_000_000),
            code.to_vec(),
            Amount::from_units(700),
            arg,
        )
        .expect("deploy")
}

// --------------------------------------------------------------------- fixture
#[test]
fn the_shipped_artifacts_are_the_ones_the_fixture_pins() {
    for (name, code) in [("store", STORE), ("kv", KV), ("events", EVENTS)] {
        let pinned: usize = field(&format!("{name}_size")).parse().expect("size");
        assert_eq!(
            code.len(),
            pinned,
            "{name}.wasm is {pinned} bytes in the fixture but {} on disk — rebuild with \
             `make build-contracts`",
            code.len()
        );
        assert_eq!(&code[..4], b"\0asm", "{name} is not a wasm module");
    }
    assert_eq!(raw_field("contracts_count"), "3");
    assert_eq!(raw_field("sdk_abi_version"), "1");
}

#[test]
fn every_contract_imports_only_the_published_abi() {
    for (name, _code) in [("store", STORE), ("kv", KV), ("events", EVENTS)] {
        let imports = field(&format!("{name}_imports"));
        assert!(
            !imports.is_empty(),
            "{name} must import the ABI to do anything"
        );
        for entry in imports.split(',') {
            let (path, kind) = entry.split_once(':').expect("module.name:kind");
            assert_eq!(kind, "func", "{name} imports a non-function: {entry}");
            let (module, function) = path.split_once('.').expect("module.name");
            assert_eq!(module, "env", "{name} imports from an unknown module");
            assert!(
                aura_exec::abi::is_abi_import(module, function),
                "{name} imports {function}, which is not ABI v1"
            );
        }
    }
    // The census agrees: ABI-only imports survive, everything else does not.
    for code in [STORE, KV, EVENTS] {
        let report = census::census(code, &ModuleLimits::hosted());
        assert_eq!(report.ban(), None, "the census refuses a shipped contract");
    }
}

// ----------------------------------------------------------------------- store
#[test]
fn store_is_a_real_counter() {
    let lifecycle = Lifecycle::new();
    let deployed = deploy(&lifecycle, STORE, None);
    assert!(
        matches!(deployed.lifecycle, LifecycleStatus::Deployed { .. }),
        "deploy: {:?}",
        deployed.lifecycle
    );
    assert_eq!(deployed.status, ExecStatus::Returned(0), "init returns 0");
    let tree = commit(&base(10_000), deployed.diff.as_ref().expect("diff"));
    assert_eq!(
        storage(&tree, "count").as_deref(),
        Some(&0u64.to_le_bytes()[..]),
        "`init` wrote the slot as 8 little-endian bytes"
    );

    // First call: 0 -> 1, reported as the return value and as an event.
    let first = lifecycle
        .call_stored(&tree, call(1_000_000), None)
        .expect("call");
    assert_eq!(first.status, ExecStatus::Returned(1));
    assert_eq!(first.events.len(), 1, "one event per call");
    let (topic, data) = event(&first.events[0]);
    assert_eq!(topic, b"count");
    assert_eq!(data, 1u64.to_le_bytes());
    let tree = commit(&tree, first.diff.as_ref().expect("diff"));
    assert_eq!(
        storage(&tree, "count").as_deref(),
        Some(&1u64.to_le_bytes()[..])
    );

    // Second call: 1 -> 2, so the slot really persisted between calls.
    let second = lifecycle
        .call_stored(&tree, call(1_000_000), None)
        .expect("call");
    assert_eq!(second.status, ExecStatus::Returned(2));
    let tree = commit(&tree, second.diff.as_ref().expect("diff"));
    assert_eq!(
        storage(&tree, "count").as_deref(),
        Some(&2u64.to_le_bytes()[..])
    );
}

#[test]
fn a_rust_contract_pays_the_handwritten_deploy_price() {
    let lifecycle = Lifecycle::new();
    for (name, code) in [("store", STORE), ("kv", KV), ("events", EVENTS)] {
        let receipt = deploy(&lifecycle, code, None);
        let expected = DEPLOY_BASE_GAS + CODE_BYTE_GAS * code.len() as u64;
        assert_eq!(
            receipt.code_gas, expected,
            "{name}: code gas is base + bytes, no SDK discount and no SDK surcharge"
        );
        assert!(
            receipt.gas_used >= receipt.code_gas,
            "{name}: a deploy that runs `init` costs at least its code"
        );
        assert_eq!(
            receipt.gas_used,
            measured(&format!("deploy_{name}_gas")),
            "{name}: deploy gas is pinned by x4-record"
        );
    }
}

// -------------------------------------------------------------------------- kv
#[test]
fn kv_takes_an_argument_and_migrates_on_upgrade() {
    let lifecycle = Lifecycle::new();
    let deployed = deploy(&lifecycle, KV, Some(42));
    assert_eq!(deployed.status, ExecStatus::Returned(0));
    let tree = commit(&base(10_000), deployed.diff.as_ref().expect("diff"));
    assert_eq!(
        storage(&tree, "value").as_deref(),
        Some(&42u64.to_le_bytes()[..]),
        "`#[constructor(arg)]` received the deploy argument"
    );
    assert_eq!(
        storage(&tree, "version").as_deref(),
        Some(&1u32.to_le_bytes()[..])
    );
    assert_eq!(balance_of(&tree), Amount::from_units(700));

    // A call with an argument overwrites the slot.
    let called = lifecycle
        .call_stored(&tree, call(1_000_000), Some(7))
        .expect("call");
    assert_eq!(called.status, ExecStatus::Returned(7));
    let tree = commit(&tree, called.diff.as_ref().expect("diff"));
    assert_eq!(
        storage(&tree, "value").as_deref(),
        Some(&7u64.to_le_bytes()[..])
    );

    // Upgrading to the same code still runs `migrate(from, to)`.
    let upgraded = lifecycle
        .upgrade(&tree, call(1_000_000), 1, KV.to_vec())
        .expect("upgrade");
    assert_eq!(
        upgraded.lifecycle,
        LifecycleStatus::Upgraded {
            from: 1,
            to: 2,
            code_hash: blake3_hash(KV),
        }
    );
    assert_eq!(
        upgraded.status,
        ExecStatus::Returned(3),
        "`migrate(1, 2)` returned from + to"
    );
    let tree = commit(&tree, upgraded.diff.as_ref().expect("diff"));
    assert_eq!(
        storage(&tree, "migrated").as_deref(),
        Some(&1u8.to_le_bytes()[..]),
        "the migration really ran"
    );
    assert_eq!(
        storage(&tree, "version").as_deref(),
        Some(&2u32.to_le_bytes()[..])
    );
    assert_eq!(
        storage(&tree, "value").as_deref(),
        Some(&7u64.to_le_bytes()[..]),
        "the value survived the migration"
    );
    assert_eq!(
        balance_of(&tree),
        Amount::from_units(700),
        "the balance survived the migration"
    );
}

// ---------------------------------------------------------------------- events
#[test]
fn events_reads_consensus_data_and_writes_a_node_digest() {
    let lifecycle = Lifecycle::new();
    // `events` has no `init`, which X3 accepts: a `run`-only contract deploys.
    let deployed = deploy(&lifecycle, EVENTS, None);
    assert!(matches!(
        deployed.lifecycle,
        LifecycleStatus::Deployed { .. }
    ));
    let tree = commit(&base(10_000), deployed.diff.as_ref().expect("diff"));

    let called = lifecycle
        .call_stored(&tree, call(1_000_000), None)
        .expect("call");
    assert_eq!(
        called.status,
        ExecStatus::Returned(7),
        "the contract sees the block height the node published"
    );
    assert_eq!(called.events.len(), 1);
    let (topic, data) = event(&called.events[0]);
    assert_eq!(topic, b"height");
    assert_eq!(data, 7u64.to_le_bytes());

    let tree = commit(&tree, called.diff.as_ref().expect("diff"));
    // The digest in state is the *node's* BLAKE3 of the height: the contract
    // asked for it through `env.hash` and got the real thing.
    assert_eq!(
        storage(&tree, "digest").as_deref(),
        Some(&blake3_hash(&7u64.to_le_bytes()).as_bytes()[..]),
        "`hash` in the SDK is the node's BLAKE3"
    );
}

#[test]
fn a_call_takes_its_code_from_state_not_from_the_caller() {
    let lifecycle = Lifecycle::new();
    let deployed = deploy(&lifecycle, STORE, None);
    let tree = commit(&base(10_000), deployed.diff.as_ref().expect("diff"));

    // The stored record is the artifact, byte for byte, and the account points
    // at BLAKE3(code).
    let record = code_of(&StateView::new(tree.clone(), 0), &address(CONTRACT))
        .expect("readable")
        .expect("present");
    assert_eq!(record.code, STORE);
    assert_eq!(record.code_version, 1);
    assert_eq!(record.abi_version, 1);
    assert_eq!(
        account_of(&StateView::new(tree.clone(), 0), &address(CONTRACT)).code_hash,
        blake3_hash(STORE)
    );
}

// --------------------------------------------------------------------- determinism
#[test]
fn two_runs_of_the_same_contract_agree_byte_for_byte() {
    let code = KV;
    let mut roots = BTreeMap::new();
    let mut gases = Vec::new();
    for attempt in 0..2 {
        let lifecycle = Lifecycle::new();
        let deployed = deploy(&lifecycle, code, Some(5));
        let tree = commit(&base(10_000), deployed.diff.as_ref().expect("diff"));
        let called = lifecycle
            .call_stored(&tree, call(1_000_000), Some(9))
            .expect("call");
        let tree = commit(&tree, called.diff.as_ref().expect("diff"));
        let upgraded = lifecycle
            .upgrade(&tree, call(1_000_000), 1, code.to_vec())
            .expect("upgrade");
        let tree = commit(&tree, upgraded.diff.as_ref().expect("diff"));
        roots.insert(attempt, tree.tree_root());
        gases.push((deployed.gas_used, called.gas_used, upgraded.gas_used));
    }
    assert_eq!(
        roots[&0], roots[&1],
        "the same contract on the same inputs must reach the same root"
    );
    assert_eq!(gases[0], gases[1], "and must cost the same");
    // Pinned by `cargo run -p aura-exec --bin x4-record`: a call's cost is a
    // measured number, not a claim. (It is ~250x the 30 gas of the X2
    // hand-written counter because a Rust contract carries the codec, the
    // stack-guard prologue and `core`'s panic path.)
    assert_eq!(gases[0].1, measured("call_kv_gas"));
    assert_eq!(gases[0].2, measured("upgrade_kv_gas"));
}

#[test]
fn every_contract_ships_an_abi_json_that_matches_its_artifact() {
    // `examples/<name>/abi.json` is generated by `tools/build_contracts.py` from
    // the artifact. It is the machine-readable half of the DoD ("ABI-JSON
    // generation"), so the gate checks it against the fixture rather than
    // trusting the file to be regenerated.
    for (name, code) in [("store", STORE), ("kv", KV), ("events", EVENTS)] {
        let path = format!(
            "{}/../../examples/{name}/abi.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {path}"));
        assert!(
            text.contains(&format!("\"code_size\": {}", code.len())),
            "{name}/abi.json does not describe the shipped artifact"
        );
        assert!(text.contains("\"abi_version\": 1"));
        assert!(text.contains("\"abi_module\": \"env\""));
        let pinned = field(&format!("{name}_sha256"));
        assert!(
            text.contains(&format!("\"code_sha256\": \"{pinned}\"")),
            "{name}/abi.json hash differs from the pinned hash"
        );
    }
}

#[test]
fn the_sdk_contracts_are_within_the_module_size_ceiling() {
    for (name, code) in [("store", STORE), ("kv", KV), ("events", EVENTS)] {
        assert!(
            code.len() < census::MAX_MODULE_BYTES,
            "{name} is {}/{} bytes",
            code.len(),
            census::MAX_MODULE_BYTES
        );
    }
    // The reference set is small enough to deploy in one transaction at the
    // Stage 1 budget, which is the practical claim: ~5 KiB each.
    let total: usize = STORE.len() + KV.len() + EVENTS.len();
    assert!(total < 20 * 1024, "the three contracts total {total} bytes");
}
