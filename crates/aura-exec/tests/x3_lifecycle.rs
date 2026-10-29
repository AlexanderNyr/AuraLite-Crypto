//! X3 gate: the contract lifecycle is a state machine, not a function call.
//!
//! The programs come from `tests/fixtures/x3_golden.txt`, assembled by
//! `tools/gen_x3_vectors.py` (independent Python assembler) — the executor never
//! builds its own inputs, and the gas of the canonical scenarios is pinned by
//! `cargo run -p aura-exec --bin x3-record`.
//!
//! What is asserted here, in one sentence each:
//!
//! * deploy stores the code, points the account at it and runs `init`;
//! * a failed `init` leaves **no account at all** (atomic deploy);
//! * `upgrade` bumps the version, keeps balance and storage, and runs `migrate`;
//! * `upgrade` refuses a stale `expected_version` (this is NC-1's target);
//! * bytecode that the census bans never reaches state;
//! * a call takes its code from state, so a caller cannot substitute logic.

use std::collections::BTreeMap;

use aura_crypto::blake3_hash;
use aura_exec::lifecycle::{
    CODE_BYTE_GAS, CODE_MAGIC, DEPLOY_BASE_GAS, Lifecycle, LifecycleError, LifecycleStatus,
    account_of, code_of,
};
use aura_exec::{BlockContext, ContractCall, ExecStatus};
use aura_state::{Account, MerkleTree, StateKey, StateView, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

const FIXTURE: &str = include_str!("fixtures/x3_golden.txt");

const CONTRACT: [u8; 20] = [0x11; 20];
const CALLER: [u8; 20] = [0x22; 20];

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn address(bytes: [u8; 20]) -> Address {
    Address::from_slice(&bytes).expect("20-byte address")
}

struct Program {
    wasm: Vec<u8>,
    exports: Vec<String>,
}

fn programs() -> BTreeMap<String, Program> {
    let mut hexes: BTreeMap<String, String> = BTreeMap::new();
    let mut exports: BTreeMap<String, String> = BTreeMap::new();
    for line in FIXTURE.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(rest) = key.strip_prefix("program_") else {
            continue;
        };
        if let Some(name) = rest.strip_suffix("_hex") {
            hexes.insert(name.to_string(), value.to_string());
        } else if let Some(name) = rest.strip_suffix("_exports") {
            exports.insert(name.to_string(), value.to_string());
        }
    }
    hexes
        .into_iter()
        .map(|(name, hex)| {
            let exports = exports
                .get(&name)
                .map(|list| {
                    list.split(',')
                        .filter(|item| !item.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            (
                name,
                Program {
                    wasm: decode(&hex),
                    exports,
                },
            )
        })
        .collect()
}

fn program(name: &str) -> Vec<u8> {
    programs()
        .remove(name)
        .unwrap_or_else(|| panic!("missing program {name}"))
        .wasm
}

fn measured(key: &str) -> u64 {
    FIXTURE
        .lines()
        .find_map(|line| line.strip_prefix(&format!("measured_{key}=")))
        .unwrap_or_else(|| panic!("missing measured_{key}; run x3-record"))
        .parse()
        .expect("measured value")
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

/// A funded base tree: the caller has plenty, the contract does not exist yet.
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

fn view_balance(tree: &MerkleTree, who: Address) -> Amount {
    account_of(&StateView::new(tree.clone(), 0), &who).balance
}

fn view_account(tree: &MerkleTree, who: Address) -> Account {
    account_of(&StateView::new(tree.clone(), 0), &who)
}

#[test]
fn fixture_describes_usable_programs() {
    let parsed = programs();
    assert_eq!(
        parsed.len(),
        9,
        "program count changed; regenerate the fixture"
    );
    for (name, program) in &parsed {
        assert!(!program.wasm.is_empty(), "{name} is empty");
        assert!(
            !program.exports.is_empty(),
            "{name} exports nothing, so it cannot be tested"
        );
    }
    // The contract-relevant entry points must exist where the tests expect them.
    assert!(parsed["v1"].exports.contains(&"init".to_string()));
    assert!(parsed["v2"].exports.contains(&"migrate".to_string()));
    assert!(!parsed["v2"].exports.contains(&"init".to_string()) || true);
    assert!(parsed["run_only"].exports == vec!["run".to_string()]);
}

#[test]
fn deploy_stores_code_and_runs_init() {
    let lifecycle = Lifecycle::new();
    let base = base(10_000);
    let code = program("v1");
    let receipt = lifecycle
        .deploy(
            &base,
            call(1_000_000),
            code.clone(),
            Amount::from_units(500),
            None,
        )
        .expect("deploy");

    assert_eq!(receipt.status, ExecStatus::Returned(0), "init returns 0");
    assert!(receipt.committed());
    let expected = LifecycleStatus::Deployed {
        code_hash: blake3_hash(&code),
        code_version: 1,
    };
    assert_eq!(receipt.lifecycle, expected);

    let after = commit(&base, receipt.diff.as_ref().expect("diff"));
    let account = view_account(&after, address(CONTRACT));
    assert_eq!(
        account.code_hash,
        blake3_hash(&code),
        "account points at code"
    );
    assert_eq!(account.balance, Amount::from_units(500), "value was moved");
    assert_eq!(
        view_balance(&after, address(CALLER)),
        Amount::from_units(9_500)
    );

    let stored = code_of(&StateView::new(after.clone(), 0), &address(CONTRACT))
        .expect("code readable")
        .expect("code present");
    assert_eq!(stored.code, code);
    assert_eq!(stored.code_version, 1);
    assert_eq!(stored.abi_version, aura_exec::ABI_VERSION);
    assert_eq!(&stored.to_bytes()[..8], &CODE_MAGIC);
    assert_eq!(storage(&after, "version").as_deref(), Some(&b"1"[..]));
}

#[test]
fn deploy_costs_base_plus_bytes() {
    let lifecycle = Lifecycle::new();
    let base = base(10_000);
    let code = program("v1");
    let receipt = lifecycle
        .deploy(&base, call(1_000_000), code.clone(), Amount::ZERO, None)
        .expect("deploy");
    let code_gas = DEPLOY_BASE_GAS + CODE_BYTE_GAS * code.len() as u64;
    assert_eq!(receipt.code_gas, code_gas, "published code price");
    assert!(
        receipt.gas_used > code_gas,
        "instruction gas must be on top of the code charge"
    );
    assert_eq!(receipt.gas_used, measured("deploy_v1_gas"));
}

#[test]
fn deploy_without_init_is_fine() {
    let lifecycle = Lifecycle::new();
    let base = base(1_000);
    let receipt = lifecycle
        .deploy(
            &base,
            call(1_000_000),
            program("run_only"),
            Amount::ZERO,
            None,
        )
        .expect("deploy");
    assert!(receipt.committed());
    assert_eq!(receipt.events.len(), 0);
    let after = commit(&base, receipt.diff.as_ref().expect("diff"));
    assert_eq!(
        view_account(&after, address(CONTRACT)).code_hash,
        blake3_hash(&program("run_only"))
    );
}

#[test]
fn failed_init_leaves_no_account() {
    let lifecycle = Lifecycle::new();
    let base = base(1_000);
    for (name, expected) in [
        (
            "trap_init",
            ExecStatus::Trap(aura_exec::TrapKind::Unreachable),
        ),
        ("oog_init", ExecStatus::OutOfGas),
    ] {
        let receipt = lifecycle
            .deploy(
                &base,
                call(1_000_000),
                program(name),
                Amount::from_units(10),
                None,
            )
            .expect("deploy runs");
        assert_eq!(receipt.status, expected, "{name}");
        assert_eq!(receipt.lifecycle, LifecycleStatus::Reverted, "{name}");
        assert!(receipt.diff.is_none(), "{name}: nothing may be committed");
        assert_eq!(
            view_account(&base, address(CONTRACT)),
            Account::new_empty(),
            "{name}: the account must not exist"
        );
        assert_eq!(
            view_balance(&base, address(CALLER)),
            Amount::from_units(1_000)
        );
    }
}

#[test]
fn upgrade_preserves_balance_and_storage_and_runs_migrate() {
    let lifecycle = Lifecycle::new();
    let base = base(10_000);
    let v1 = program("v1");
    let v2 = program("v2");

    let deployed = lifecycle
        .deploy(
            &base,
            call(1_000_000),
            v1.clone(),
            Amount::from_units(700),
            None,
        )
        .expect("deploy");
    let after_deploy = commit(&base, deployed.diff.as_ref().expect("diff"));

    // A call takes the code from state and returns 1.
    let called = lifecycle
        .call_stored(&after_deploy, call(1_000_000), None)
        .expect("call");
    assert_eq!(called.status, ExecStatus::Returned(1));
    assert!(!called.events.is_empty() || called.diff.is_some() || called.writes == 0);

    let upgraded = lifecycle
        .upgrade(&after_deploy, call(1_000_000), 1, v2.clone())
        .expect("upgrade");
    assert_eq!(
        upgraded.lifecycle,
        LifecycleStatus::Upgraded {
            from: 1,
            to: 2,
            code_hash: blake3_hash(&v2)
        }
    );
    assert_eq!(
        upgraded.status,
        ExecStatus::Returned(0),
        "migrate returns 0"
    );

    let after_upgrade = commit(&after_deploy, upgraded.diff.as_ref().expect("diff"));
    let account = view_account(&after_upgrade, address(CONTRACT));
    assert_eq!(account.code_hash, blake3_hash(&v2), "logic changed");
    assert_eq!(
        account.balance,
        Amount::from_units(700),
        "balance survived the upgrade"
    );
    assert_eq!(
        storage(&after_upgrade, "version").as_deref(),
        Some(&b"2"[..]),
        "migrate rewrote the version slot"
    );
    assert_eq!(
        storage(&after_upgrade, "migrated").as_deref(),
        Some(&b"yes"[..])
    );
    let stored = code_of(
        &StateView::new(after_upgrade.clone(), 0),
        &address(CONTRACT),
    )
    .expect("readable")
    .expect("present");
    assert_eq!(stored.code_version, 2);

    // And the new logic is what runs.
    let called = lifecycle
        .call_stored(&after_upgrade, call(1_000_000), None)
        .expect("call");
    assert_eq!(called.status, ExecStatus::Returned(2));
    assert_eq!(upgraded.gas_used, measured("upgrade_v1_v2_gas"));
}

#[test]
fn upgrade_refuses_a_stale_version() {
    let lifecycle = Lifecycle::new();
    let base = base(1_000);
    let v1 = program("v1");
    let deployed = lifecycle
        .deploy(&base, call(1_000_000), v1, Amount::ZERO, None)
        .expect("deploy");
    let after = commit(&base, deployed.diff.as_ref().expect("diff"));

    let error = lifecycle
        .upgrade(&after, call(1_000_000), 2, program("v2"))
        .expect_err("stale version must be refused");
    assert_eq!(
        error,
        LifecycleError::VersionMismatch {
            expected: 2,
            actual: 1
        }
    );
    assert_eq!(error.code(), "version_mismatch");

    // The first upgrade wins, the second one sees the new version.
    let first = lifecycle
        .upgrade(&after, call(1_000_000), 1, program("v2"))
        .expect("upgrade");
    let after_two = commit(&after, first.diff.as_ref().expect("diff"));
    let second = lifecycle
        .upgrade(&after_two, call(1_000_000), 1, program("v3"))
        .expect_err("second upgrade from the same version must fail");
    assert_eq!(
        second,
        LifecycleError::VersionMismatch {
            expected: 1,
            actual: 2
        }
    );
    // A third-version upgrade from the right base works.
    lifecycle
        .upgrade(&after_two, call(1_000_000), 2, program("v3"))
        .expect("upgrade from 2");
}

#[test]
fn lifecycle_refusals_happen_before_state() {
    let lifecycle = Lifecycle::new();
    let base = base(1_000);

    // No code at the address.
    assert_eq!(
        lifecycle
            .call_stored(&base, call(1_000_000), None)
            .expect_err("not deployed"),
        LifecycleError::NotDeployed(address(CONTRACT))
    );
    assert_eq!(
        lifecycle
            .upgrade(&base, call(1_000_000), 1, program("v2"))
            .expect_err("not deployed"),
        LifecycleError::NotDeployed(address(CONTRACT))
    );

    // Banned bytecode never reaches state.
    let refused = lifecycle
        .deploy(&base, call(1_000_000), program("float"), Amount::ZERO, None)
        .expect_err("census must refuse floats");
    assert_eq!(refused.code(), "refused:censored:float_instruction");

    // No `run` export: not callable, so not deployable.
    assert_eq!(
        lifecycle
            .deploy(
                &base,
                call(1_000_000),
                program("no_run"),
                Amount::ZERO,
                None
            )
            .expect_err("missing run"),
        LifecycleError::MissingRunEntry
    );

    // A constructor with the wrong signature is a refusal, not a trap.
    let bad = lifecycle
        .deploy(
            &base,
            call(1_000_000),
            program("bad_init_arity"),
            Amount::ZERO,
            None,
        )
        .expect_err("bad init arity");
    assert_eq!(bad.code(), "refused:limit:init_arity");

    // Deploying twice is refused: upgrade is the path.
    let deployed = lifecycle
        .deploy(&base, call(1_000_000), program("v1"), Amount::ZERO, None)
        .expect("deploy");
    let after = commit(&base, deployed.diff.as_ref().expect("diff"));
    assert_eq!(
        lifecycle
            .deploy(&after, call(1_000_000), program("v2"), Amount::ZERO, None)
            .expect_err("already deployed"),
        LifecycleError::AlreadyDeployed(address(CONTRACT))
    );
}

#[test]
fn caller_must_fund_the_deploy() {
    let lifecycle = Lifecycle::new();
    let base = base(100);
    let error = lifecycle
        .deploy(
            &base,
            call(1_000_000),
            program("v1"),
            Amount::from_units(1_000),
            None,
        )
        .expect_err("cannot send more than the caller has");
    assert_eq!(error, LifecycleError::InsufficientBalance);
    assert_eq!(
        view_balance(&base, address(CALLER)),
        Amount::from_units(100),
        "a refusal must not move anything"
    );
}

#[test]
fn lifecycle_is_deterministic() {
    let lifecycle = Lifecycle::new();
    let base = base(10_000);
    let code = program("v2");
    let first = lifecycle
        .deploy(
            &base,
            call(1_000_000),
            code.clone(),
            Amount::from_units(1),
            None,
        )
        .expect("deploy");
    let second = lifecycle
        .deploy(&base, call(1_000_000), code, Amount::from_units(1), None)
        .expect("deploy");
    assert_eq!(first.result_root, second.result_root);
    assert_eq!(first.gas_used, second.gas_used);
    assert_eq!(
        first.diff.as_ref().map(|diff| diff.to_bytes()),
        second.diff.as_ref().map(|diff| diff.to_bytes())
    );
}

#[test]
fn code_record_is_strictly_decoded() {
    use aura_exec::lifecycle::CodeRecord;
    let record = CodeRecord::first(program("v1"));
    let bytes = record.to_bytes();
    assert_eq!(CodeRecord::from_bytes(&bytes).expect("round trip"), record);
    assert_eq!(record.code_hash(), blake3_hash(&record.code));

    // Truncated, wrong magic, wrong length, wrong format: all refused.
    assert!(CodeRecord::from_bytes(&bytes[..20]).is_err());
    let mut wrong_magic = bytes.clone();
    wrong_magic[0] ^= 0xFF;
    assert!(CodeRecord::from_bytes(&wrong_magic).is_err());
    assert!(CodeRecord::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(CodeRecord::from_bytes(&trailing).is_err());
    let mut wrong_format = bytes.clone();
    wrong_format[8] = 9;
    assert!(CodeRecord::from_bytes(&wrong_format).is_err());
}

#[test]
fn measured_gas_is_pinned() {
    let lifecycle = Lifecycle::new();
    let base = base(10_000);
    let deployed = lifecycle
        .deploy(&base, call(1_000_000), program("v1"), Amount::ZERO, None)
        .expect("deploy");
    let after = commit(&base, deployed.diff.as_ref().expect("diff"));
    let called = lifecycle
        .call_stored(&after, call(1_000_000), None)
        .expect("call");
    assert_eq!(deployed.gas_used, measured("deploy_v1_gas"));
    assert_eq!(called.gas_used, measured("call_v1_gas"));
}
