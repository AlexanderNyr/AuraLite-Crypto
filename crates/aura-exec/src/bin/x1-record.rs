//! `x1-record` — run the X1 fixture through the real sandbox and pin the
//! measured block.
//!
//! The expectation block of `tests/fixtures/x1_golden.txt` is written by the
//! independent Python assembler. This binary writes the second block: the exact
//! `wasmi` fuel and trace length for every program. That is what turns "the
//! sandbox runs it" into "the sandbox burns exactly this much fuel", so a
//! dependency bump that changes metering cannot pass unnoticed.
//!
//! ```text
//! cargo run -p aura-exec --bin x1-record
//! make gen-x1-vectors
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use aura_exec::{Engine, Sandbox, SandboxError, SandboxLimits};

const MARKER: &str = "# --- measured by `cargo run -p aura-exec --bin x1-record` ---";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/x1_golden.txt")
}

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn classify(result: Result<aura_exec::Execution, SandboxError>) -> (String, u64, usize) {
    match result {
        Ok(execution) => {
            let outcome = match &execution.outcome {
                aura_exec::Outcome::Returned(value) => format!("return:{value}"),
                aura_exec::Outcome::Trap(kind) => format!("trap:{}", kind.code()),
            };
            (outcome, execution.fuel_used, execution.trace.len())
        }
        Err(SandboxError::Censored(ban)) => (format!("censored:{}", ban.code()), 0, 0),
        Err(SandboxError::Limit(code)) => (format!("limit:{code}"), 0, 0),
        Err(SandboxError::ImportsNotAllowed { .. }) => ("imports_not_allowed".to_string(), 0, 0),
        Err(SandboxError::Instantiate(_)) => ("instantiate".to_string(), 0, 0),
        Err(SandboxError::Compile(_)) => ("compile".to_string(), 0, 0),
        Err(SandboxError::EngineUnavailable(_)) => ("engine_unavailable".to_string(), 0, 0),
        Err(SandboxError::Trap(kind)) => (format!("error:{}", kind.code()), 0, 0),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = fixture_path();
    let text = std::fs::read_to_string(&path)?;
    let header: Vec<&str> = text
        .lines()
        .take_while(|line| *line != MARKER)
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect();

    let mut programs: BTreeMap<String, (String, Option<i32>)> = BTreeMap::new();
    for line in header {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if let Some(label) = key
            .strip_prefix("program_")
            .and_then(|rest| rest.strip_suffix("_hex"))
        {
            programs
                .entry(label.to_string())
                .or_insert_with(|| (String::new(), None))
                .0 = value.to_string();
        } else if let Some(label) = key
            .strip_prefix("program_")
            .and_then(|rest| rest.strip_suffix("_input"))
        {
            let arg = if value.is_empty() {
                None
            } else {
                Some(value.parse::<i32>()?)
            };
            programs
                .entry(label.to_string())
                .or_insert_with(|| (String::new(), None))
                .1 = arg;
        }
    }

    let only = std::env::var("AURA_X1_ONLY").ok();
    let sandbox = Sandbox::new();
    let mut measured: Vec<String> = Vec::new();
    for (label, (hex, arg)) in &programs {
        if let Some(filter) = &only
            && !label.contains(filter.as_str())
        {
            continue;
        }
        let wasm = decode(hex);
        let (outcome, fuel, trace) = classify(sandbox.execute(&wasm, *arg));
        measured.push(format!("measured_{label}_outcome={outcome}"));
        measured.push(format!("measured_{label}_fuel={fuel}"));
        measured.push(format!("measured_{label}_trace={trace}"));
        println!("{label:<20} {outcome:<26} fuel={fuel:<8} trace={trace}");
    }

    let mut out: Vec<String> = text
        .lines()
        .take_while(|line| *line != MARKER)
        .map(|line| line.to_string())
        .collect();
    out.push(MARKER.to_string());
    out.extend(measured);
    std::fs::write(&path, out.join("\n") + "\n")?;
    println!();
    println!(
        "wrote {} ({} programs, engine wasmi {})",
        path.display(),
        programs.len(),
        sandbox.engine().version()
    );
    println!(
        "limits: fuel={} memory_pages={} module_bytes={} stack={} trace={}",
        sandbox.limits().fuel,
        sandbox.limits().memory_pages,
        sandbox.limits().module_bytes,
        SandboxLimits::restrictive().max_stack_bytes,
        sandbox.limits().max_trace_events
    );
    Ok(())
}
