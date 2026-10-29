//! `x7-run` — replay the X7 determinism scenario as one node.
//!
//! The gate spawns this binary eight times. Each process gets its own OS entropy
//! for Rust's per-process hasher seed, its own allocator history and its own
//! address layout, so agreement between processes is a stronger statement than
//! agreement inside one process.
//!
//! Usage:
//! ```text
//! x7-run                     # one node, one line on stdout
//! x7-run --node 3            # label only; never an input
//! x7-run --seed 42           # a different scenario (the sensitivity check)
//! x7-run --fixture           # the pinned lines tools/gen_x7_vectors.py writes
//! ```

use aura_exec::detcheck::{self, Op};

fn main() {
    let mut node: u64 = 0;
    let mut seed: Option<u64> = None;
    let mut fixture = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--node" => {
                node = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .expect("--node needs a number");
            }
            "--seed" => {
                seed = Some(
                    args.next()
                        .and_then(|value| value.parse().ok())
                        .expect("--seed needs a number"),
                );
            }
            "--fixture" => fixture = true,
            "--help" | "-h" => {
                println!("usage: x7-run [--node N] [--seed S] [--fixture]");
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let steps = match seed {
        Some(seed) => detcheck::scenario_with_seed(seed),
        None => detcheck::scenario(),
    };

    if fixture {
        let mut contract_calls = 0usize;
        let mut deploys = 0usize;
        let mut transfers = 0usize;
        let mut nested = 0usize;
        let vault = detcheck::contract_address(detcheck::Code::Vault);
        for step in &steps {
            match step.op {
                Op::Deploy { .. } => deploys += 1,
                Op::CallStored { .. } => contract_calls += 1,
                Op::CallData { contract, .. } => {
                    contract_calls += 1;
                    if contract == vault {
                        nested += 1;
                    }
                }
                Op::Transfer { .. } => transfers += 1,
            }
        }
        println!("scenario_seed={}", seed.unwrap_or(detcheck::X7_SEED));
        println!("scenario_steps={}", steps.len());
        println!("scenario_deploys={deploys}");
        println!("scenario_contract_calls={contract_calls}");
        println!("scenario_transfers={transfers}");
        println!("scenario_vault_calls={nested}");
        println!("scenario_accounts={}", detcheck::X7_ACCOUNTS);
        println!("scenario_gas_limit={}", detcheck::X7_GAS_LIMIT);
        println!("scenario_starved_gas={}", detcheck::X7_STARVED_GAS);
        println!("scenario_height={}", detcheck::X7_HEIGHT);
        println!("scenario_timestamp_ms={}", detcheck::X7_TIMESTAMP_MS);
        for (name, size, digest) in detcheck::code_pins() {
            println!("code_{name}_size={size}");
            println!("code_{name}_blake3={}", digest.to_hex());
        }
        let report = detcheck::run(&steps, 0);
        println!("expected_committed={}", report.committed);
        println!("expected_refused={}", report.refused);
        println!("expected_failed={}", report.failed);
        println!("expected_gas={}", report.gas_total);
        println!("expected_version={}", report.version);
        println!("expected_state_root={}", report.state_root.to_hex());
        println!("expected_tree_root={}", report.tree_root.to_hex());
        println!("expected_events_root={}", report.events_root.to_hex());
        println!("expected_digest={}", report.digest.to_hex());
        return;
    }

    let report = detcheck::run(&steps, node);
    println!("{}", report.summary());
}
