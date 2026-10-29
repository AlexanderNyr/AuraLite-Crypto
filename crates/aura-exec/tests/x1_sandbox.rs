//! X1 gate: the sandbox runs, meters and refuses.
//!
//! Three sources of truth meet here:
//!
//! * `tests/fixtures/x1_golden.txt` — written by `tools/gen_x1_vectors.py`
//!   (independent Python assembler: expectations) and by
//!   `cargo run -p aura-exec --bin x1-record` (measurement: fuel and trace);
//! * `aura_exec::census` — X0's static verdict, which must run first;
//! * the engine, which must turn every input into either a value or a member of
//!   the closed trap set.
//!
//! The tests never build WASM themselves. A test that can only fail when its own
//! input changes proves nothing.

use std::collections::BTreeMap;

use aura_exec::wasmi_engine::WASMI_VERSION;
use aura_exec::{
    Engine, Outcome, Sandbox, SandboxError, SandboxLimits, TrapKind, run, run_with_arg,
};

const FIXTURE: &str = include_str!("fixtures/x1_golden.txt");
const MARKER: &str = "# --- measured by `cargo run -p aura-exec --bin x1-record` ---";

struct Vector {
    label: String,
    wasm: Vec<u8>,
    expect: String,
    input: Option<i32>,
    measured_outcome: Option<String>,
    measured_fuel: Option<u64>,
    measured_trace: Option<usize>,
}

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn vectors() -> Vec<Vector> {
    let mut hexes: BTreeMap<String, String> = BTreeMap::new();
    let mut expects: BTreeMap<String, String> = BTreeMap::new();
    let mut inputs: BTreeMap<String, Option<i32>> = BTreeMap::new();
    let mut outcomes: BTreeMap<String, String> = BTreeMap::new();
    let mut fuels: BTreeMap<String, u64> = BTreeMap::new();
    let mut traces: BTreeMap<String, usize> = BTreeMap::new();

    for line in FIXTURE.lines() {
        if line.starts_with('#') || line.is_empty() || line == MARKER {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(rest) = key
            .strip_prefix("program_")
            .or_else(|| key.strip_prefix("measured_"))
        else {
            continue;
        };
        let measured = key.starts_with("measured_");
        let (label, field) = rest.rsplit_once('_').expect("label_field");
        match (measured, field) {
            (false, "hex") => {
                hexes.insert(label.to_string(), value.to_string());
            }
            (false, "expect") => {
                expects.insert(label.to_string(), value.to_string());
            }
            (false, "input") => {
                inputs.insert(label.to_string(), value.parse::<i32>().ok());
            }
            (true, "outcome") => {
                outcomes.insert(label.to_string(), value.to_string());
            }
            (true, "fuel") => {
                fuels.insert(label.to_string(), value.parse::<u64>().expect("fuel"));
            }
            (true, "trace") => {
                traces.insert(label.to_string(), value.parse::<usize>().expect("trace"));
            }
            _ => {}
        }
    }

    hexes
        .into_iter()
        .map(|(label, hex)| Vector {
            wasm: decode(&hex),
            expect: expects.get(&label).cloned().unwrap_or_default(),
            input: inputs.get(&label).cloned().flatten(),
            measured_outcome: outcomes.get(&label).cloned(),
            measured_fuel: fuels.get(&label).cloned(),
            measured_trace: traces.get(&label).cloned(),
            label,
        })
        .collect()
}

fn classify(result: Result<aura_exec::Execution, SandboxError>) -> (String, u64, usize) {
    match result {
        Ok(execution) => {
            let outcome = match &execution.outcome {
                Outcome::Returned(value) => format!("return:{value}"),
                Outcome::Trap(kind) => format!("trap:{}", kind.code()),
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

#[test]
fn every_fixture_program_meets_its_expectation() {
    let sandbox = Sandbox::new();
    let mut allowed = 0;
    let mut rejected = 0;
    let mut traps = 0;
    for vector in vectors() {
        assert!(
            !vector.expect.is_empty(),
            "{}: fixture row has no expectation",
            vector.label
        );
        let raw = sandbox.execute(&vector.wasm, vector.input);
        let (outcome, _, _) = classify(raw.clone());
        // `trap` is the wildcard the fixture uses when the trap kind is measured
        // rather than predicted (a stack limit is an engine parameter).
        let matches = if vector.expect == "trap" {
            outcome.starts_with("trap:")
        } else {
            outcome == vector.expect
        };
        assert!(
            matches,
            "{}: expected {}, sandbox produced {outcome}",
            vector.label, vector.expect
        );
        if outcome.starts_with("return:") {
            allowed += 1;
        } else if outcome.starts_with("trap:") {
            traps += 1;
        } else {
            rejected += 1;
        }
    }
    println!("X1 gate: {allowed} executed, {traps} trapped, {rejected} rejected");
    assert!(allowed >= 6, "the fixture must exercise real execution");
    assert!(traps >= 4, "the fixture must exercise trap classification");
    assert!(rejected >= 8, "the fixture must exercise the gates");
}

#[test]
fn measured_fuel_and_trace_are_pinned() {
    let sandbox = Sandbox::new();
    let mut checked = 0;
    for vector in vectors() {
        let (outcome, fuel, trace) = classify(sandbox.execute(&vector.wasm, vector.input));
        if let Some(recorded) = &vector.measured_outcome {
            assert_eq!(
                &outcome, recorded,
                "{}: live outcome differs from the recorded measurement",
                vector.label
            );
        }
        let expected_fuel = vector.measured_fuel.unwrap_or_else(|| {
            panic!(
                "{}: no measured fuel (run `make gen-x1-vectors`)",
                vector.label
            )
        });
        let expected_trace = vector
            .measured_trace
            .unwrap_or_else(|| panic!("{}: no measured trace", vector.label));
        assert_eq!(
            fuel, expected_fuel,
            "{}: fuel drifted from the pinned value — a metering change is a protocol decision",
            vector.label
        );
        assert_eq!(
            trace, expected_trace,
            "{}: trace length drifted",
            vector.label
        );
        checked += 1;
    }
    assert_eq!(checked, 19, "the whole fixture is fuel-pinned");
}

#[test]
fn census_runs_before_the_engine() {
    let sandbox = Sandbox::new();
    // A float module, a WASI import module and a mutable global must all be
    // stopped by X0's census, never by the engine.
    for label in ["f32_add", "wasi_clock", "mutable_global", "start_section"] {
        let vector = vectors()
            .into_iter()
            .find(|vector| vector.label == label)
            .expect("fixture row");
        match sandbox.execute(&vector.wasm, vector.input) {
            Err(SandboxError::Censored(ban)) => {
                assert_eq!(
                    format!("censored:{}", ban.code()),
                    vector.expect,
                    "{label}: X0 and X1 disagree about the ban code"
                );
            }
            other => panic!("{label}: expected a census ban, got {other:?}"),
        }
    }
}

#[test]
fn limits_are_validated_and_tightenable() {
    let mut limits = SandboxLimits::restrictive();
    limits.memory_pages = 4097;
    assert!(
        Sandbox::with_limits(limits).is_err(),
        "a limit above the census ceiling must be refused"
    );
    limits = SandboxLimits::restrictive();
    limits.fuel = 0;
    assert!(
        Sandbox::with_limits(limits).is_err(),
        "a zero fuel budget is not a budget"
    );
    limits = SandboxLimits::restrictive();
    limits.module_bytes = 1 << 20;
    assert!(
        Sandbox::with_limits(limits).is_err(),
        "a module cap above the census ceiling must be refused"
    );

    // A tighter budget must turn a working program into an out-of-fuel trap:
    // this is the mechanism X2 maps to gas.
    let mut tight = SandboxLimits::restrictive();
    tight.fuel = 100;
    let sandbox = Sandbox::with_limits(tight).expect("valid limits");
    let sum_to_n = vectors()
        .into_iter()
        .find(|vector| vector.label == "sum_to_n")
        .expect("fixture row");
    let execution = sandbox
        .execute(&sum_to_n.wasm, sum_to_n.input)
        .expect("census passes");
    assert!(
        execution.trapped(TrapKind::OutOfFuel),
        "100 fuel must not be enough for sum_to_n(1000), got {:?}",
        execution.outcome
    );
    // MEASURED: when the budget runs out at call entry, `wasmi` keeps the fuel
    // cached in its interpreter frame and the store still reports the full
    // budget, so `fuel_used` under-reports (0 here). It never over-reports,
    // which is the safe direction for a gate; X2 must derive gas from a value
    // the store owns, and its gate re-measures this case.
    assert!(
        execution.fuel_used <= 100,
        "fuel_used must never exceed the budget, got {}",
        execution.fuel_used
    );

    // With a budget that is enough to enter the loop but not to finish it, the
    // engine reports a partial consumption and the trap is still exact.
    let mut partial = SandboxLimits::restrictive();
    partial.fuel = 5_000;
    let sandbox = Sandbox::with_limits(partial).expect("valid limits");
    let execution = sandbox
        .execute(&sum_to_n.wasm, sum_to_n.input)
        .expect("census passes");
    assert!(execution.trapped(TrapKind::OutOfFuel));
    assert!(
        execution.fuel_used > 0 && execution.fuel_used <= 5_000,
        "partial consumption must be reported, got {}",
        execution.fuel_used
    );
}

#[test]
fn memory_ceiling_is_enforced_at_runtime() {
    let mut limits = SandboxLimits::restrictive();
    limits.memory_pages = 1;
    let sandbox = Sandbox::with_limits(limits).expect("valid limits");
    let grow = vectors()
        .into_iter()
        .find(|vector| vector.label == "mem_grow")
        .expect("fixture row");
    let (outcome, _, _) = classify(sandbox.execute(&grow.wasm, grow.input));
    assert_ne!(
        outcome, "return:2",
        "memory.grow past the ceiling must not succeed (got {outcome})"
    );
}

#[test]
fn trace_is_capped() {
    let mut limits = SandboxLimits::restrictive();
    limits.max_trace_events = 1;
    let sandbox = Sandbox::with_limits(limits).expect("valid limits");
    let execution = run_with_arg(&vectors()[0].wasm, 0).expect("default sandbox runs");
    assert!(execution.trace.len() <= 2);
    let capped = sandbox
        .execute(&vectors()[0].wasm, None)
        .expect("capped sandbox runs");
    assert_eq!(capped.trace.len(), 1, "the trace cap must be honoured");
    assert_eq!(
        capped.outcome, execution.outcome,
        "the trace cap must not change the result"
    );
}

#[test]
fn repeated_runs_are_bit_identical() {
    // The engine-level determinism claim of X1: same bytes, same limits, same
    // input -> byte-identical Execution, 100 times, with no state carried over.
    let sum_to_n = vectors()
        .into_iter()
        .find(|vector| vector.label == "sum_to_n")
        .expect("fixture row");
    let first = run_with_arg(&sum_to_n.wasm, sum_to_n.input.unwrap()).expect("runs");
    for round in 0..100 {
        let again = run_with_arg(&sum_to_n.wasm, sum_to_n.input.unwrap()).expect("runs");
        assert_eq!(
            first, again,
            "run {round} diverged: {:?} vs {:?}",
            first, again
        );
    }
    assert!(first.returned(500_500));
}

#[test]
fn engine_identity_is_pinned() {
    let sandbox = Sandbox::new();
    assert_eq!(sandbox.engine().name(), "wasmi");
    assert_eq!(sandbox.engine().version(), WASMI_VERSION);
    assert!(
        sandbox.engine().traces(),
        "the default backend records traces"
    );
    assert!(run(&[]).is_err(), "empty bytes are not a module");
}

#[test]
fn module_size_cap_is_checked_before_parsing() {
    // The sandbox cap equals the census ceiling, so an oversized module is
    // stopped by the cheap length check before any parser runs.
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    wasm.resize(SandboxLimits::restrictive().module_bytes as usize + 1, 0);
    match run(&wasm) {
        Err(SandboxError::Limit(code)) => assert_eq!(code, "module_bytes"),
        other => panic!("expected a size limit, got {other:?}"),
    }

    let mut tight = SandboxLimits::restrictive();
    tight.module_bytes = 4;
    let sandbox = Sandbox::with_limits(tight).expect("valid limits");
    match sandbox.execute(b"\0asm\x01\0\0\0", None) {
        Err(SandboxError::Limit(code)) => assert_eq!(code, "module_bytes"),
        other => panic!("expected a limit error, got {other:?}"),
    }
}

// The optional JIT backend. Compiled only with `--features wasmtime-engine`
// (see the `exec-wasmtime` CI job). Fuel metering is engine-specific, so this
// test compares the OUTCOME CLASS only: a value, a trap, or a refusal. Mapping
// fuel across engines is X2's gas model, and byte-identical roots are X7.
#[cfg(feature = "wasmtime-engine")]
#[test]
fn wasmtime_backend_agrees_on_outcomes() {
    use aura_exec::wasmtime_engine::WasmtimeEngine;

    let sandbox =
        Sandbox::with(SandboxLimits::restrictive(), WasmtimeEngine).expect("valid limits");
    assert_eq!(sandbox.engine().name(), "wasmtime");
    let mut compared = 0;
    for vector in vectors() {
        let raw = sandbox.execute(&vector.wasm, vector.input);
        let (outcome, _, _) = classify(raw.clone());
        // Trap *kinds* are engine specific (`wasmi` says StackOverflow where a
        // JIT says "call stack exhausted"); the class is what must agree.
        let expected_class = if vector.expect.starts_with("trap") {
            "trap"
        } else {
            vector.expect.as_str()
        };
        let actual_class = if outcome.starts_with("trap:") {
            "trap"
        } else {
            outcome.as_str()
        };
        assert_eq!(
            actual_class, expected_class,
            "{}: engines disagree ({} vs {}); detail: {:?}",
            vector.label, outcome, vector.expect, raw
        );
        compared += 1;
    }
    assert_eq!(compared, 19, "every fixture program must be compared");
}
