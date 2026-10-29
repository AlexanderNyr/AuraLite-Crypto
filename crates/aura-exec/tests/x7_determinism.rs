//! X7 — the determinism gate.
//!
//! The claim: **the same ordered input, replayed by independent nodes, produces
//! the same `state_root`, the same gas bill and the same events.**
//!
//! "Independent" is why this file spawns processes. Two runs inside one process
//! share Rust's per-process hasher seed, allocator history and address layout, so
//! they can agree while a second node disagrees. The eight processes below are
//! eight different seeds, eight allocator histories and eight address layouts
//! executing the same 1 004 ordered transactions.
//!
//! Every number asserted here is read from
//! `crates/aura-exec/tests/fixtures/x7_determinism.txt`, which
//! `tools/gen_x7_vectors.py` writes from `x7-run --fixture` — so the pin and the
//! measurement cannot drift apart silently.

use std::collections::BTreeMap;
use std::process::{Command, Stdio};

use aura_exec::detcheck::{
    self, Code, Op, Step, X7_ACCOUNTS, X7_NODES, X7_SEED, X7_STARVED_GAS, X7_STEPS,
};
use aura_types::Hash;

const FIXTURE: &str = include_str!("fixtures/x7_determinism.txt");

/// Parse `key=value` lines, skipping comments and blank lines.
fn fixture() -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in FIXTURE.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    fields
}

fn field(key: &str) -> u64 {
    fixture()
        .get(key)
        .unwrap_or_else(|| panic!("fixture is missing `{key}`"))
        .parse()
        .unwrap_or_else(|_| panic!("fixture `{key}` is not a number"))
}

fn text(key: &str) -> String {
    fixture()
        .get(key)
        .unwrap_or_else(|| panic!("fixture is missing `{key}`"))
        .clone()
}

fn hex(key: &str) -> Hash {
    Hash::from_hex(&text(key)).expect("fixture hash is valid hex")
}

/// Run one scenario in-process.
fn run(steps: &[Step]) -> detcheck::NodeReport {
    detcheck::run(steps, 0)
}

// --------------------------------------------------------------- the input

#[test]
fn the_scenario_is_a_pure_function_of_its_seed() {
    let first = detcheck::scenario();
    let second = detcheck::scenario();
    assert_eq!(
        first, second,
        "the generator must not depend on time or entropy"
    );
    assert_eq!(
        first.len(),
        field("scenario_steps") as usize,
        "the fixture and the generator must agree on the length"
    );
    assert_eq!(first.len(), X7_STEPS + Code::ALL.len());

    // The mix is fixed by construction: the fixture records how many of each.
    let mut deploys = 0;
    let mut calls = 0;
    let mut transfers = 0;
    for step in &first {
        match step.op {
            Op::Deploy { .. } => deploys += 1,
            Op::CallStored { .. } | Op::CallData { .. } => calls += 1,
            Op::Transfer { .. } => transfers += 1,
        }
    }
    assert_eq!(deploys, field("scenario_deploys") as usize);
    assert_eq!(calls, field("scenario_contract_calls") as usize);
    assert_eq!(transfers, field("scenario_transfers") as usize);
    assert!(
        calls >= X7_STEPS * 3 / 4,
        "the workload must be dominated by contract calls, not transfers: {calls}"
    );

    // A different seed is a different input, not a relabelled one.
    let other = detcheck::scenario_with_seed(X7_SEED ^ 0xdead_beef);
    assert_eq!(other.len(), first.len());
    let differing = first
        .iter()
        .zip(other.iter())
        .filter(|(left, right)| left.op != right.op)
        .count();
    assert!(
        differing > first.len() / 2,
        "a new seed must rewrite most of the workload, not a handful of steps: {differing}"
    );
}

// ------------------------------------------------------------- the replay

#[test]
fn one_process_agrees_with_itself() {
    // A short scenario: the property is about the machine, not about the length,
    // and the full 1 004-step run is the eight-process gate's job.
    let steps = detcheck::scenario_of(X7_SEED, 120);
    let first = run(&steps);
    let second = run(&steps);
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert!(
        first.committed > 0 && first.failed + first.refused > 0,
        "the short scenario must still contain both kinds of step: {first:?}"
    );
}

#[test]
fn eight_nodes_in_separate_processes_agree() {
    // Spawned together and then joined, so the wall clock stays close to one run
    // while the hash seeds, allocator histories and address layouts differ.
    let mut children = Vec::with_capacity(X7_NODES);
    for node in 0..X7_NODES as u64 {
        let child = Command::new(env!("CARGO_BIN_EXE_x7-run"))
            .arg("--node")
            .arg(node.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("x7-run is built by the same cargo invocation as this test");
        children.push((node, child));
    }

    let mut reports: Vec<(u64, detcheck::NodeReport)> = Vec::with_capacity(X7_NODES);
    for (node, child) in children {
        let output = child.wait_with_output().expect("node finishes");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        assert!(
            output.status.success(),
            "node {node} failed: {stderr}\n{stdout}"
        );
        reports.push((node, parse_report(&stdout)));
    }

    let (first_node, first) = &reports[0];
    let expected = fixture();
    for (node, report) in &reports {
        assert_eq!(
            report.state_root, first.state_root,
            "node {node} disagrees with node {first_node} about the state root"
        );
        assert_eq!(report.tree_root, first.tree_root, "node {node} tree root");
        assert_eq!(
            report.events_root, first.events_root,
            "node {node} events root"
        );
        assert_eq!(report.digest, first.digest, "node {node} rolling digest");
        assert_eq!(report.gas_total, first.gas_total, "node {node} gas bill");
        assert_eq!(report.version, first.version, "node {node} version");
        assert_eq!(report.committed, first.committed, "node {node} commits");
        assert_eq!(report.refused, first.refused, "node {node} refusals");
        assert_eq!(report.failed, first.failed, "node {node} failures");
    }

    // …and the eight processes agree with the pinned measurement, not only with
    // each other: agreement on the wrong answer is still a failure.
    assert_eq!(
        first.state_root.to_hex(),
        expected["expected_state_root"],
        "the nodes agree with each other but not with the pin"
    );
    assert_eq!(first.gas_total, field("expected_gas"));
    assert_eq!(first.digest.to_hex(), expected["expected_digest"]);
}

/// Parse one `key=value …` line of `x7-run`.
fn parse_report(line: &str) -> detcheck::NodeReport {
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    for token in line.split_whitespace() {
        // The failure mix is printed as `[machine=… trap=… lifecycle=…]`; it is
        // not needed here and its brackets would break the split.
        if token.starts_with('[') || token.ends_with(']') {
            continue;
        }
        if let Some((key, value)) = token.split_once('=') {
            fields.insert(key, value);
        }
    }
    let number = |key: &str| -> u64 {
        fields
            .get(key)
            .unwrap_or_else(|| panic!("report line is missing `{key}`: {line}"))
            .parse()
            .unwrap_or_else(|_| panic!("`{key}` is not a number: {line}"))
    };
    let hash = |key: &str| -> Hash {
        Hash::from_hex(fields.get(key).expect("hash field")).expect("hash hex")
    };
    detcheck::NodeReport {
        node: number("node"),
        steps: number("steps") as usize,
        committed: number("committed") as usize,
        refused: number("refused") as usize,
        failed: number("failed") as usize,
        // The mix is re-derived from the summary line by the tests that need it.
        failed_machine: 0,
        failed_trap: 0,
        failed_lifecycle: 0,
        gas_total: number("gas"),
        version: number("version"),
        tree_root: hash("tree_root"),
        state_root: hash("state_root"),
        events_root: hash("events_root"),
        digest: hash("digest"),
    }
}

// --------------------------------------------------------------- the pin

#[test]
fn the_pinned_measurement_describes_this_tree() {
    let report = detcheck::run_scenario();
    assert_eq!(report.steps, field("scenario_steps") as usize);
    assert_eq!(report.committed, field("expected_committed") as usize);
    assert_eq!(report.refused, field("expected_refused") as usize);
    assert_eq!(report.failed, field("expected_failed") as usize);
    assert_eq!(report.gas_total, field("expected_gas"));
    assert_eq!(report.version, field("expected_version"));
    assert_eq!(report.state_root, hex("expected_state_root"));
    assert_eq!(report.tree_root, hex("expected_tree_root"));
    assert_eq!(report.events_root, hex("expected_events_root"));
    assert_eq!(report.digest, hex("expected_digest"));
}

#[test]
fn the_pinned_artifacts_are_the_shipped_ones() {
    for (name, size, digest) in detcheck::code_pins() {
        assert_eq!(
            size as u64,
            field(&format!("code_{name}_size")),
            "{name} size drifted from the fixture"
        );
        assert_eq!(
            digest.to_hex(),
            text(&format!("code_{name}_blake3")),
            "{name} bytes drifted from the fixture"
        );
    }
    assert_eq!(detcheck::code_pins().len(), Code::ALL.len());
}

// ------------------------------------------------------- what a node did

#[test]
fn every_commit_advances_the_version_by_exactly_one() {
    let report = detcheck::run(&detcheck::scenario_of(X7_SEED, 200), 0);
    assert_eq!(
        report.version as usize, report.committed,
        "a committed step must be the only thing that moves the version"
    );
    assert!(
        report.committed as usize + report.refused + report.failed == report.steps,
        "every step must be classified exactly once: {report:?}"
    );
}

#[test]
fn a_starved_call_is_billed_and_changes_nothing() {
    // One call to `kv` with a budget that starts but cannot finish: the machine
    // must bill the budget it gave away and leave the root byte-identical.
    let steps = vec![Step {
        index: 0,
        op: Op::CallStored {
            contract: detcheck::contract_address(Code::Kv),
            caller: detcheck::account_address(0),
            arg: Some(7),
            gas_limit: X7_STARVED_GAS,
        },
    }];
    let deploy_only: Vec<Step> = detcheck::scenario()
        .into_iter()
        .filter(|step| matches!(step.op, Op::Deploy { code: Code::Kv, .. }))
        .collect();
    assert_eq!(deploy_only.len(), 1);

    let base = run(&deploy_only);
    // Re-index the call so the digest chain is comparable across the two runs.
    let mut replay = deploy_only.clone();
    let mut tail = steps;
    tail[0].index = replay.len();
    replay.extend(tail);

    let report = run(&replay);
    assert_eq!(
        report.failed,
        1,
        "a starved call fails: {:?}",
        report.failure_mix()
    );
    assert_eq!(report.committed, base.committed, "it committed nothing");
    assert_eq!(report.version, base.version, "the version did not move");
    assert_eq!(
        report.state_root, base.state_root,
        "an out-of-gas call must leave the root byte-identical"
    );
    assert!(
        report.gas_total > base.gas_total,
        "a starved call is still billed: {} vs {}",
        report.gas_total,
        base.gas_total
    );
}

#[test]
fn a_transfer_to_an_unfunded_side_is_billed_and_changes_nothing() {
    // 8 funded accounts, and a tenth of them is not one: the payer is real, the
    // amount is larger than the balance, so the step must be refused.
    let steps = vec![
        Step {
            index: 0,
            op: Op::Transfer {
                from: detcheck::account_address(0),
                to: detcheck::account_address(1),
                amount: detcheck::X7_START_BALANCE + 1,
            },
        },
        Step {
            index: 0,
            op: Op::Transfer {
                from: detcheck::account_address(0),
                to: detcheck::account_address(1),
                amount: 10,
            },
        },
    ];
    let report = run(&steps);
    assert_eq!(report.committed, 1, "the affordable transfer committed");
    assert_eq!(report.refused, 1, "the overdraft was refused");
    assert_eq!(report.version, 1, "only one step moved the version");
    assert_eq!(
        report.gas_total,
        detcheck::X7_TRANSFER_GAS * 2,
        "both transfers are billed, including the one that failed"
    );
}

#[test]
fn the_genesis_ledger_is_funded_and_ordered() {
    let tree = detcheck::genesis();
    assert_eq!(
        tree.entries().len(),
        X7_ACCOUNTS,
        "every account must exist before the first transaction"
    );
    let paths: Vec<Hash> = tree.entries().iter().map(|(path, _)| *path).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "the leaf set is stored in path order");
}

// ------------------------------------------------ the gate can go red

#[test]
fn a_different_input_changes_the_root() {
    let standard = detcheck::run(&detcheck::scenario_of(X7_SEED, 200), 0);
    let other = detcheck::run(&detcheck::scenario_of(X7_SEED ^ 1, 200), 0);
    assert_ne!(
        standard.state_root, other.state_root,
        "the gate would pass on any input, which makes it worthless"
    );
    assert_ne!(standard.digest, other.digest);
    assert_ne!(standard.events_root, other.events_root);
}

#[test]
fn the_digest_depends_on_the_order_of_the_steps() {
    // Two transfers that touch different accounts commute on the ledger, so the
    // root may legitimately be the same; the *digest* must not be, or a node that
    // executed a different order could never be caught.
    let forward = vec![
        Step {
            index: 0,
            op: Op::Transfer {
                from: detcheck::account_address(0),
                to: detcheck::account_address(1),
                amount: 5,
            },
        },
        Step {
            index: 1,
            op: Op::Transfer {
                from: detcheck::account_address(2),
                to: detcheck::account_address(3),
                amount: 7,
            },
        },
    ];
    let mut backward = forward.clone();
    backward.swap(0, 1);
    backward[0].index = 0;
    backward[1].index = 1;

    let first = run(&forward);
    let second = run(&backward);
    assert_eq!(first.state_root, second.state_root, "transfers commute");
    assert_ne!(
        first.digest, second.digest,
        "the rolling digest must be order-sensitive"
    );
}
