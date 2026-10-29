//! wasmi backend — the default engine.
//!
//! `wasmi` is a deterministic interpreter, and that is the point: the same
//! bytes produce the same result on every OS and CPU, with no JIT, no signal
//! handlers and no address-space tricks. It also builds in ~20 s, so every gate
//! in `make test` really runs the sandbox instead of skipping it (measured on
//! this repository: `wasmtime`'s Cranelift codegen does not fit in a 2 GB
//! sandbox and is a separate CI job).
//!
//! The configuration below mirrors [`crate::census::CENSUS`] one row at a time:
//! floats off, mutable globals off, bulk memory off, reference types off, tail
//! calls off, multi-memory off, start functions off, strict enforced limits,
//! fuel on. The feature is enabled with `default-features = false` +
//! `deterministic` + `std` (see `Cargo.toml`), so SIMD is not even compiled in.

use std::collections::BTreeMap;

use wasmi::{
    CallHook, Caller, Config, EnforcedLimits, Engine as WasmiCore, Instance, Linker, Memory,
    Module, Store, StoreLimits, StoreLimitsBuilder, TrapCode,
};

use crate::abi::{self, AbiError, AbiFn, LinearMemory};
use crate::contract::HostState;
use crate::error::{SandboxError, TrapKind};
use crate::limits::SandboxLimits;
use crate::sandbox::{Engine, Event, Execution, Outcome};

/// Exact `wasmi` version pinned in `Cargo.lock`.
pub const WASMI_VERSION: &str = "2.0.0";

/// Store data: the resource limiter, the trace buffer, and (X2) the host state.
///
/// `host` is `None` for X1-style unhosted runs; the ABI is only reachable when
/// a [`HostState`] was moved into the store, so an X1 module can never observe
/// a half-initialised host.
struct State {
    limits: StoreLimits,
    fuel_total: u64,
    max_trace: usize,
    trace: Vec<Event>,
    host: Option<HostState>,
    /// Fuel value at the last host-call boundary — the instruction meter is
    /// reset to `gas_left` there, so this is where the current segment starts.
    boundary: u64,
}

impl State {
    /// Data for an unhosted (X1) run.
    fn new(limits: &SandboxLimits) -> Self {
        Self::with_host(limits, None)
    }

    /// Data for a run with (or without) a host.
    fn with_host(limits: &SandboxLimits, host: Option<HostState>) -> Self {
        Self {
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_bytes())
                .table_elements(limits.table_entries as usize)
                .instances(1)
                .memories(1)
                .tables(1)
                .trap_on_grow_failure(true)
                .build(),
            fuel_total: limits.fuel,
            max_trace: limits.max_trace_events,
            trace: Vec::new(),
            host,
            boundary: limits.fuel,
        }
    }
}

/// Guest memory as the ABI sees it: bounds-checked, size-capped byte access.
struct GuestMemory<'a, 'b> {
    caller: &'a mut Caller<'b, State>,
    memory: Memory,
}

impl LinearMemory for GuestMemory<'_, '_> {
    fn read(&self, ptr: u32, len: u32) -> Option<Vec<u8>> {
        if len > abi::MAX_ABI_BUFFER {
            return None;
        }
        let mut buffer = vec![0u8; len as usize];
        self.memory
            .read(&*self.caller, ptr as usize, &mut buffer)
            .ok()?;
        Some(buffer)
    }

    fn write(&mut self, ptr: u32, data: &[u8]) -> bool {
        if data.len() as u64 > u64::from(abi::MAX_ABI_BUFFER) {
            return false;
        }
        self.memory
            .write(&mut *self.caller, ptr as usize, data)
            .is_ok()
    }
}

/// One ABI call, with the budget settled on both sides of it.
///
/// The invariant: `store fuel == gas_left` at every host-call boundary. Before
/// the call we bill the instructions of the segment that just ran, then the ABI
/// bills its own work, then the meter is reset to what is left. One counter,
/// two consumers — this is what removes the X1 `fuel_used` under-report.
fn host_abi(caller: &mut Caller<'_, State>, func: AbiFn, args: &[i32]) -> i32 {
    let Some(memory) = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
    else {
        // No memory export: the module cannot be executed at all. The status is
        // still well defined, so the contract sees a failure instead of a trap.
        return AbiError::OutOfMemory.code();
    };
    let Some(mut host) = caller.data_mut().host.take() else {
        return AbiError::OutOfMemory.code();
    };

    // 1. Bill the instructions that ran since the last boundary.
    let current = caller.get_fuel().unwrap_or(0);
    let boundary = caller.data().boundary;
    host.settle_instructions(boundary.saturating_sub(current));

    // 2. Run the ABI; it bills its own gas into the same view.
    let code = {
        let mut memory = GuestMemory { caller, memory };
        abi::dispatch(&mut host, &mut memory, func, args)
    };

    // 3. Reset the instruction meter to what the call has left.
    let left = host.gas_left();
    let _ = caller.set_fuel(left);
    let data = caller.data_mut();
    data.boundary = left;
    data.host = Some(host);
    code
}

/// Link the whole X2 ABI (`env.*`, version 1) into `linker`.
fn link_abi(linker: &mut Linker<State>) -> Result<(), SandboxError> {
    macro_rules! wrap {
        ($name:literal, $func:expr, $($arg:ident),+) => {
            linker
                .func_wrap(
                    abi::ABI_MODULE,
                    $name,
                    |mut caller: Caller<'_, State>, $($arg: i32),+| -> i32 {
                        host_abi(&mut caller, $func, &[$($arg),+])
                    },
                )
                .map_err(SandboxError::instantiate)?;
        };
    }
    wrap!("storage_read", AbiFn::StorageRead, a, b, c, d);
    wrap!("storage_write", AbiFn::StorageWrite, a, b, c, d);
    wrap!("balance", AbiFn::Balance, a, b, c);
    wrap!("transfer", AbiFn::Transfer, a, b, c);
    wrap!("emit", AbiFn::Emit, a, b, c, d);
    wrap!("hash", AbiFn::Hash, a, b, c);
    wrap!("block_ctx", AbiFn::BlockCtx, a, b);
    Ok(())
}

/// Reject every import that is not part of ABI v1.
///
/// X1 refused *all* imports; X2 refuses every import it cannot account for.
/// The set is closed and named: an unknown `env.*` name, a different module, a
/// memory or a global import — all of them are refusal, not a link failure that
/// a later phase could paper over.
fn check_imports(module: &Module) -> Result<usize, SandboxError> {
    let mut allowed = 0usize;
    for import in module.imports() {
        let name = import.name();
        let ok = import.module() == abi::ABI_MODULE
            && AbiFn::from_name(name).is_some()
            && matches!(import.ty(), wasmi::ExternType::Func(_));
        if !ok {
            return Err(SandboxError::ImportsNotAllowed {
                count: allowed + 1,
                first: Some(format!("{}.{}", import.module(), name)),
            });
        }
        allowed += 1;
    }
    Ok(allowed)
}

fn record(state: &mut State, code: &'static str, func_index: u32) {
    if state.trace.len() < state.max_trace {
        state.trace.push(Event { code, func_index });
    }
}

fn map_trap(code: TrapCode) -> TrapKind {
    match code {
        TrapCode::OutOfFuel => TrapKind::OutOfFuel,
        TrapCode::UnreachableCodeReached => TrapKind::Unreachable,
        TrapCode::MemoryOutOfBounds => TrapKind::MemoryOutOfBounds,
        TrapCode::IntegerDivisionByZero | TrapCode::IntegerOverflow => {
            TrapKind::IntegerDivideByZero
        }
        TrapCode::StackOverflow => TrapKind::StackExhausted,
        _ => TrapKind::Other,
    }
}

fn engine_config(limits: &SandboxLimits) -> Config {
    let mut config = Config::default();
    config
        // determinism: no floats anywhere, no threads, no bulk helpers
        .floats(false)
        .wasm_mutable_global(false)
        .wasm_bulk_memory(false)
        .wasm_reference_types(false)
        .wasm_tail_call(false)
        .wasm_multi_memory(false)
        .wasm_extended_const(false)
        .wasm_sign_extension(true)
        .wasm_saturating_float_to_int(false)
        .wasm_multi_value(true)
        // no implicit execution: a start function is a census ban anyway
        .allow_start_fn(false)
        // strict engine-side limits, plus our own numbers
        .enforced_limits(EnforcedLimits::strict())
        .consume_fuel(true)
        .set_max_stack_height(limits.max_stack_bytes.max(64 * 1024));
    config
}

/// The default backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct WasmiEngine;

impl WasmiEngine {
    /// Build the engine for `limits` (validated by the sandbox before this).
    pub fn new() -> Self {
        Self
    }
}

impl Engine for WasmiEngine {
    fn name(&self) -> &'static str {
        "wasmi"
    }

    fn version(&self) -> &'static str {
        WASMI_VERSION
    }

    fn traces(&self) -> bool {
        true
    }

    fn execute(
        &self,
        wasm: &[u8],
        limits: &SandboxLimits,
        arg: Option<i32>,
    ) -> Result<Execution, SandboxError> {
        let (engine, module, arity, ready) = prepare(wasm, limits, false)?;

        let mut store = Store::new(&engine, State::new(limits));
        store.limiter(|state: &mut State| &mut state.limits);
        store
            .set_fuel(limits.fuel)
            .map_err(SandboxError::instantiate)?;
        store.call_hook(trace_hook);

        let linker: Linker<State> = Linker::new(&engine);
        let instance: Instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(SandboxError::instantiate)?;

        let args: &[i32] = match (arity, arg) {
            (1, Some(value)) => &[value],
            (1, None) => &[0],
            _ => &[],
        };
        let outcome = match call_entry(&mut store, &instance, "run", args) {
            Ok(value) => Outcome::Returned(i64::from(value)),
            Err(err) => match err.as_trap_code() {
                Some(code) => Outcome::Trap(map_trap(code)),
                None => return Err(SandboxError::instantiate(err)),
            },
        };

        let remaining = store.get_fuel().map_err(SandboxError::instantiate)?;
        let state = store.into_data();
        let fuel_used = state.fuel_total.saturating_sub(remaining);
        Ok(Execution {
            outcome,
            fuel_used,
            trace: state.trace,
            exports: ready,
        })
    }

    fn entry_arity(
        &self,
        wasm: &[u8],
        limits: &SandboxLimits,
        name: &str,
    ) -> Result<Option<u32>, SandboxError> {
        // Deliberately NOT `prepare`: a module may legitimately export `init`
        // or `migrate` without exporting `run`, and X3 must be able to say
        // "no such entry point" instead of failing on a different requirement.
        let engine = WasmiCore::new(&engine_config(limits));
        let module = Module::new(&engine, wasm).map_err(SandboxError::compile)?;
        check_imports(&module)?;
        Ok(arities(&module).remove(name).map(|arity| arity as u32))
    }

    fn execute_hosted(
        &self,
        wasm: &[u8],
        limits: &SandboxLimits,
        entry: &str,
        args: &[i32],
        host: HostState,
    ) -> Result<(Execution, HostState), SandboxError> {
        let gas_limit = host.call.gas_limit.min(limits.fuel);
        let (engine, module, _run_arity, exports) = prepare(wasm, limits, true)?;

        let mut state = State::with_host(limits, Some(host));
        state.boundary = gas_limit;
        let mut store = Store::new(&engine, state);
        store.limiter(|state: &mut State| &mut state.limits);
        store
            .set_fuel(gas_limit)
            .map_err(SandboxError::instantiate)?;
        store.call_hook(trace_hook);

        let mut linker: Linker<State> = Linker::new(&engine);
        link_abi(&mut linker)?;
        let instance: Instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(SandboxError::instantiate)?;

        let outcome = match call_entry(&mut store, &instance, entry, args) {
            Ok(value) => Outcome::Returned(i64::from(value)),
            Err(err) => match err.as_trap_code() {
                Some(code) => Outcome::Trap(map_trap(code)),
                None => return Err(SandboxError::instantiate(err)),
            },
        };

        // Close the last segment: whatever the contract spent after the final
        // host call is still instruction gas and must appear on the bill.
        let remaining = store.get_fuel().map_err(SandboxError::instantiate)?;
        let State {
            trace,
            host,
            boundary,
            ..
        } = store.into_data();
        let mut host = host.ok_or_else(|| SandboxError::instantiate("host state vanished"))?;
        host.settle_instructions(boundary.saturating_sub(remaining));

        // An exhausted budget is billed in full: partial billing would let a
        // contract buy unbounded work with a failing call.
        let fuel_used = if host.out_of_gas {
            host.call.gas_limit
        } else {
            host.view.gas_used()
        };
        Ok((
            Execution {
                outcome,
                fuel_used,
                trace,
                exports,
            },
            host,
        ))
    }
}

/// Trace hook shared by both entry points.
fn trace_hook(state: &mut State, hook: CallHook) -> Result<(), wasmi::Error> {
    // `wasmi` 2.0 reports the *kind* of transition, not the function index. The
    // sequence is still exact and comparable, which is what the replay gate
    // needs; the host ABI adds its own status codes on top.
    match hook {
        CallHook::CallingWasm => record(state, "enter", state.trace.len() as u32),
        CallHook::ReturningFromWasm => record(state, "exit", state.trace.len() as u32),
        CallHook::CallingHost | CallHook::ReturningFromHost => {
            record(state, "host", state.trace.len() as u32)
        }
    }
    Ok(())
}

/// Config, module, import census and `run` arity — shared by both entry points.
fn prepare(
    wasm: &[u8],
    limits: &SandboxLimits,
    allow_imports: bool,
) -> Result<(WasmiCore, Module, usize, Vec<String>), SandboxError> {
    let engine = WasmiCore::new(&engine_config(limits));
    let module = Module::new(&engine, wasm).map_err(SandboxError::compile)?;

    let imports = module.imports().count();
    if allow_imports {
        check_imports(&module)?;
    } else if imports > 0 {
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
            && let wasmi::ExternType::Func(func) = export.ty()
        {
            run_params = Some(func.params().len());
        }
    }
    exports.sort();
    exports.dedup();
    let arity = match run_params {
        Some(0) | Some(1) => run_params.unwrap_or(0),
        Some(_) => return Err(SandboxError::Limit("run_arity")),
        None => {
            return Err(SandboxError::instantiate(
                "module does not export a `run` function",
            ));
        }
    };
    Ok((engine, module, arity, exports))
}

/// Call the named entry point with `args` (0 to 2 `i32` parameters).
///
/// The export type decides the call shape, so a wrong signature is an
/// instantiate error rather than a silent success. X3 needs three entry points:
/// `run` (0/1 args), `init` (0/1) and `migrate` (2).
fn call_entry(
    store: &mut Store<State>,
    instance: &Instance,
    entry: &str,
    args: &[i32],
) -> Result<i32, wasmi::Error> {
    match args {
        [] => {
            let func = instance.get_typed_func::<(), i32>(&*store, entry)?;
            func.call(store, ())
        }
        [a] => {
            let func = instance.get_typed_func::<i32, i32>(&*store, entry)?;
            func.call(store, *a)
        }
        [a, b] => {
            let func = instance.get_typed_func::<(i32, i32), i32>(&*store, entry)?;
            func.call(store, (*a, *b))
        }
        _ => Err(wasmi::Error::new(format!(
            "entry `{entry}` takes at most 2 arguments"
        ))),
    }
}

/// Every exported function and its arity, in one pass.
fn arities(module: &Module) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for export in module.exports() {
        if let wasmi::ExternType::Func(func) = export.ty() {
            out.insert(export.name().to_string(), func.params().len());
        }
    }
    out
}
