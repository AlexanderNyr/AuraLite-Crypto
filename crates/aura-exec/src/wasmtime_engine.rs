//! wasmtime backend — optional, behind the `wasmtime-engine` feature.
//!
//! # Why this is not the default
//!
//! Cranelift's codegen does not fit in 2 GB of RAM: `rustc` is killed by the
//! OOM killer while compiling `cranelift-codegen`, even with `-j1` and
//! `debug = 0` (measured 2026-10-28). It does build with a 4 GB swap file, which
//! is how the X1 cross-engine check was run, but a 20-minute first build is not
//! the inner loop of a repository whose whole point is that every gate runs
//! everywhere. So the **default** backend is the interpreter in
//! [`crate::wasmi_engine`] and this module is additionally gated by the
//! `exec-wasmtime` CI job.
//!
//! # What it is for
//!
//! X7 requires two different engines to agree byte-for-byte on `state_root` for
//! the same transaction sequence. Two backends configured from the same
//! [`SandboxLimits`] and returning the same [`Execution`] shape make that gate
//! expressible. X1 measures the weaker, already useful statement: both engines
//! agree on the OUTCOME of all 19 fixture programs
//! (`wasmtime_backend_agrees_on_outcomes`). Fuel numbers are `wasmi`-specific
//! and are pinned for `wasmi` only; mapping fuel across engines is X2's gas
//! model.
//!
//! # Trap mapping
//!
//! `wasmtime` reports traps through `anyhow`-style errors. X1 classifies them
//! by message text instead of matching `wasmtime::Trap` variants, because the
//! variant set changes between releases and a `Cargo.lock` bump must not
//! silently change what the sandbox considers a trap. The strings are
//! **measured**, not guessed: the cross-engine test found three that a
//! hand-written classifier missed —
//! `"all fuel consumed by WebAssembly"`,
//! `"memory fault at wasm address … in linear memory of size …"`, and the
//! component-model/`reference-types` conflict. An unrecognised message becomes
//! [`TrapKind::Other`], and X7 fails the gate on any `Other` it has not seen.

use wasmtime::{
    Config, Engine as WasmtimeCore, Instance, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder,
};

use crate::error::{SandboxError, TrapKind};
use crate::limits::SandboxLimits;
use crate::sandbox::{Engine, Execution, Outcome};

/// Exact `wasmtime` version pinned in `Cargo.lock`.
pub const WASMTIME_VERSION: &str = "50.0.0";

/// Store data: the resource limiter and the fuel budget.
struct State {
    limits: StoreLimits,
}

fn engine_config(limits: &SandboxLimits) -> Config {
    let mut config = Config::new();
    config
        .consume_fuel(true)
        // no threads, no SIMD, no GC, no components: one module, one memory
        .wasm_threads(false)
        .wasm_simd(false)
        .wasm_relaxed_simd(false)
        .wasm_bulk_memory(false)
        .wasm_reference_types(false)
        .wasm_tail_call(false)
        .wasm_multi_memory(false)
        .wasm_memory64(false)
        .wasm_custom_page_sizes(false)
        .wasm_extended_const(false)
        .wasm_gc(false)
        // The component model (and its async variants) needs reference types,
        // and we disable reference types. Turning the whole proposal off is
        // both the restrictive choice and the only consistent one — MEASURED:
        // without this line `wasmtime` refuses every module with
        // "the component-model-async feature requires the wasm reference-types
        // proposal", which is exactly what the cross-engine gate caught.
        .wasm_component_model(false)
        .wasm_component_model_async(false)
        .wasm_component_model_more_async_builtins(false)
        .wasm_component_model_async_stackful(false)
        .wasm_component_model_threading(false)
        .wasm_component_model_gc(false)
        .wasm_component_model_error_context(false)
        .wasm_component_model_map(false)
        .wasm_component_model_memory64(false)
        .wasm_component_model_fixed_length_lists(false)
        .wasm_component_model_implements(false)
        .wasm_component_model_canonical_names(false)
        .wasm_component_model_accessors(false)
        // determinism: canonical NaN, no float recompilation surprises
        .cranelift_nan_canonicalization(true)
        .max_wasm_stack(limits.max_stack_bytes)
        .wasm_backtrace_max_frames(None)
        .parallel_compilation(false);
    config
}

/// Classify a `wasmtime` error message into the closed trap set.
pub fn classify(message: &str) -> Option<TrapKind> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("fuel") && (lower.contains("out of") || lower.contains("consumed")) {
        Some(TrapKind::OutOfFuel)
    } else if lower.contains("unreachable") {
        Some(TrapKind::Unreachable)
    } else if lower.contains("memory fault")
        || lower.contains("out of bounds memory")
        || lower.contains("memory out of bounds")
    {
        Some(TrapKind::MemoryOutOfBounds)
    } else if lower.contains("divide by zero") || lower.contains("division by zero") {
        Some(TrapKind::IntegerDivideByZero)
    } else if lower.contains("call stack exhausted") || lower.contains("stack overflow") {
        Some(TrapKind::StackExhausted)
    } else {
        None
    }
}

/// The `wasmtime` (JIT) backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct WasmtimeEngine;

impl Engine for WasmtimeEngine {
    fn name(&self) -> &'static str {
        "wasmtime"
    }

    fn version(&self) -> &'static str {
        WASMTIME_VERSION
    }

    fn traces(&self) -> bool {
        // No call hook in this backend yet: X7 compares outcomes and roots,
        // not event streams, so an empty trace is honest rather than wrong.
        false
    }

    fn execute(
        &self,
        wasm: &[u8],
        limits: &SandboxLimits,
        arg: Option<i32>,
    ) -> Result<Execution, SandboxError> {
        let engine = WasmtimeCore::new(&engine_config(limits)).map_err(SandboxError::compile)?;
        let module = Module::new(&engine, wasm).map_err(SandboxError::compile)?;

        let imports = module.imports().count();
        if imports > 0 {
            let first = module
                .imports()
                .next()
                .map(|import| format!("{}.{}", import.module(), import.name()));
            return Err(SandboxError::ImportsNotAllowed {
                count: imports,
                first,
            });
        }

        let mut exports: Vec<String> = Vec::new();
        let mut run_params: Option<usize> = None;
        for export in module.exports() {
            exports.push(export.name().to_string());
            if export.name() == "run"
                && let wasmtime::ExternType::Func(func) = export.ty()
            {
                run_params = Some(func.params().len());
            }
        }
        exports.sort();
        exports.dedup();

        let mut store = Store::new(
            &engine,
            State {
                limits: StoreLimitsBuilder::new()
                    .memory_size(limits.memory_bytes())
                    .table_elements(limits.table_entries as usize)
                    .instances(1)
                    .memories(1)
                    .tables(1)
                    .trap_on_grow_failure(true)
                    .build(),
            },
        );
        store.limiter(|state: &mut State| &mut state.limits);
        store
            .set_fuel(limits.fuel)
            .map_err(SandboxError::instantiate)?;

        let linker: Linker<State> = Linker::new(&engine);
        let instance: Instance = linker
            .instantiate(&mut store, &module)
            .map_err(SandboxError::instantiate)?;

        let result = match run_params {
            Some(1) => instance
                .get_typed_func::<i32, i32>(&mut store, "run")
                .map_err(SandboxError::instantiate)?
                .call(&mut store, arg.unwrap_or(0)),
            Some(0) => instance
                .get_typed_func::<(), i32>(&mut store, "run")
                .map_err(SandboxError::instantiate)?
                .call(&mut store, ()),
            Some(_) => return Err(SandboxError::Limit("run_arity")),
            None => {
                return Err(SandboxError::instantiate(
                    "module does not export a `run` function",
                ));
            }
        };

        let outcome = match result {
            Ok(value) => Outcome::Returned(i64::from(value)),
            Err(err) => match classify(&err.to_string()) {
                Some(kind) => Outcome::Trap(kind),
                None => return Err(SandboxError::instantiate(err)),
            },
        };

        let remaining = store.get_fuel().map_err(SandboxError::instantiate)?;
        let fuel_used = limits.fuel.saturating_sub(remaining);
        Ok(Execution {
            outcome,
            fuel_used,
            trace: Vec::new(),
            exports,
        })
    }
}
