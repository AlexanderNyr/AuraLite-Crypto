//! `x3-record` — run the canonical X3 lifecycle scenarios and pin their gas.
//!
//! The expectation block of `tests/fixtures/x3_golden.txt` is written by the
//! independent Python assembler; this binary writes the second block: the exact
//! gas of `deploy(v1)`, `call`, `upgrade(v1 -> v2)` and `call` again. A metering
//! change or a change in the lifecycle's own charges therefore turns the X3 gate
//! red instead of silently re-pricing every deploy.
//!
//! ```text
//! cargo run -p aura-exec --bin x3-record
//! make gen-x3-vectors
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use aura_exec::lifecycle::Lifecycle;
use aura_exec::{BlockContext, ContractCall};
use aura_state::{Account, MerkleTree, StateKey, apply_diff};
use aura_types::{Address, Amount, Hash, Nonce};

const MARKER: &str = "# --- measured by `cargo run -p aura-exec --bin x3-record` ---";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/x3_golden.txt")
}

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn programs(text: &str) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for line in text.lines().take_while(|line| *line != MARKER) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if let Some(name) = key
            .strip_prefix("program_")
            .and_then(|rest| rest.strip_suffix("_hex"))
        {
            out.insert(name.to_string(), decode(value));
        }
    }
    out
}

fn call(gas: u64) -> ContractCall {
    ContractCall {
        contract: Address::from_slice(&[0x11u8; 20]).expect("address"),
        caller: Address::from_slice(&[0x22u8; 20]).expect("address"),
        block: BlockContext {
            height: 7,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: gas,
    }
}

fn base() -> MerkleTree {
    let caller = Account::new(
        Nonce::new(0),
        Amount::from_units(10_000),
        Hash::ZERO,
        Hash::ZERO,
    );
    let caller_address = Address::from_slice(&[0x22u8; 20]).expect("address");
    MerkleTree::from_entries(
        1,
        &[(
            StateKey::account(&caller_address).digest(),
            caller.to_bytes(),
        )],
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = fixture_path();
    let text = std::fs::read_to_string(&path)?;
    let programs = programs(&text);
    let get = |name: &str| -> Vec<u8> {
        programs
            .get(name)
            .unwrap_or_else(|| panic!("missing program {name}"))
            .clone()
    };

    let lifecycle = Lifecycle::new();
    let start = base();

    let deployed = lifecycle.deploy(&start, call(1_000_000), get("v1"), Amount::ZERO, None)?;
    println!(
        "deploy(v1)      {:?} gas={}",
        deployed.lifecycle, deployed.gas_used
    );
    let after_deploy = apply_diff(&start, deployed.diff.as_ref().expect("diff"))?;

    let called = lifecycle.call_stored(&after_deploy, call(1_000_000), None)?;
    println!(
        "call(v1)        {:?} gas={}",
        called.status, called.gas_used
    );

    let upgraded = lifecycle.upgrade(&after_deploy, call(1_000_000), 1, get("v2"))?;
    println!(
        "upgrade(v1->v2) {:?} gas={}",
        upgraded.lifecycle, upgraded.gas_used
    );
    let after_upgrade = apply_diff(&after_deploy, upgraded.diff.as_ref().expect("diff"))?;

    let called_v2 = lifecycle.call_stored(&after_upgrade, call(1_000_000), None)?;
    println!(
        "call(v2)        {:?} gas={}",
        called_v2.status, called_v2.gas_used
    );

    let run_only =
        lifecycle.deploy(&start, call(1_000_000), get("run_only"), Amount::ZERO, None)?;
    println!(
        "deploy(run_only) {:?} gas={}",
        run_only.lifecycle, run_only.gas_used
    );

    let measured = format!(
        "measured_deploy_v1_gas={}\nmeasured_call_v1_gas={}\nmeasured_upgrade_v1_v2_gas={}\n\
         measured_call_v2_gas={}\nmeasured_deploy_run_only_gas={}\n",
        deployed.gas_used,
        called.gas_used,
        upgraded.gas_used,
        called_v2.gas_used,
        run_only.gas_used,
    );

    let mut out: Vec<String> = text
        .lines()
        .take_while(|line| *line != MARKER)
        .map(str::to_string)
        .collect();
    out.push(MARKER.to_string());
    out.extend(measured.lines().map(str::to_string));
    std::fs::write(&path, out.join("\n") + "\n")?;
    println!();
    println!("wrote {} ({} programs)", path.display(), programs.len());
    Ok(())
}
