//! wasmi backend — the default engine.
//!
//! `wasmi` is a deterministic interpreter, and that is the point: the same
//! bytes produce the same result on every OS and CPU, with no JIT, no signal
//! handlers and no address-space tricks. It also builds in ~20 s, so every gate
//! in `make test` really runs the sandbox instead of skipping it (measured on
//! this repository: `wasmtime`'s Cranelift codegen does not fit in a 2 GB
//! sandbox and is a separate CI job).
//!
//! X6 adds cross-contract calls: a host function that instantiates and runs a
//! *second* module inside the first. The nested store is a fresh one, so the two
//! executions cannot share a stack; what they share is the parent's gas, the
//! frame stack and one rule — a frame commits wholly or not at all.
//!
//! The configuration below mirrors [`crate::census::CENSUS`] one row at a time:
//! floats off, reference types off, tail
//! calls off, multi-memory off, start functions off, strict enforced limits,
//! fuel on. The feature is enabled with `default-features = false` +
//! `deterministic` + `std` (see `Cargo.toml`), so SIMD is not even compiled in.

use std::collections::BTreeMap;

use wasmi::{
    CallHook, Caller, Config, EnforcedLimits, Engine as WasmiCore, Instance, Linker, Memory,
    Module, Store, StoreLimits, StoreLimitsBuilder, TrapCode,
};

use crate::abi::{self, AbiError, AbiFn, LinearMemory};
use crate::abi_v2::{self, Abi2Fn};
use crate::abi_v3::{self, Abi3Fn};
use crate::contract::{ContractCall, Executor, HostState, RUN_ENTRY};
use crate::error::{SandboxError, TrapKind};
use crate::lifecycle::code_of;
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
    /// The sandbox configuration, kept so a nested call can build a child store
    /// under exactly the same limits as its parent.
    sandbox: SandboxLimits,
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
            sandbox: *limits,
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
    host_call(caller, args, |host, memory, args| {
        abi::dispatch(host, memory, func, args)
    })
}

/// One ABI v2 (`env2`) call, with the same fuel settlement as v1.
fn host_abi2(caller: &mut Caller<'_, State>, func: Abi2Fn, args: &[i32]) -> i32 {
    host_call(caller, args, |host, memory, args| {
        abi_v2::dispatch(host, memory, func, args)
    })
}

fn host_call(
    caller: &mut Caller<'_, State>,
    args: &[i32],
    run: impl FnOnce(&mut HostState, &mut GuestMemory<'_, '_>, &[i32]) -> i32,
) -> i32 {
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
        run(&mut host, &mut memory, args)
    };

    // 3. Reset the instruction meter to what the call has left.
    let left = host.gas_left();
    let _ = caller.set_fuel(left);
    let data = caller.data_mut();
    data.boundary = left;
    data.host = Some(host);
    code
}

/// One `env3.call`: bill the segment, run the callee, settle the meter.
///
/// The callee runs in its own [`Store`], built from the same
/// [`SandboxLimits`] as this frame — a nested call cannot loosen a limit its
/// parent was given. The two stores never share a stack, which is what makes the
/// depth limit meaningful: without it, depth would be the host's stack, and a
/// hostile contract would find it as a segfault instead of a status code.
fn host_abi3_call(
    caller: &mut Caller<'_, State>,
    addr_ptr: i32,
    addr_len: i32,
    data_ptr: i32,
    data_len: i32,
    value_lo: i64,
    value_hi: i64,
) -> i32 {
    let Some(memory) = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
    else {
        return AbiError::OutOfMemory.code();
    };
    let Some(mut host) = caller.data_mut().host.take() else {
        return AbiError::OutOfMemory.code();
    };
    let limits = caller.data().sandbox;

    let current = caller.get_fuel().unwrap_or(0);
    let boundary = caller.data().boundary;
    host.settle_instructions(boundary.saturating_sub(current));

    let code = {
        let mut memory = GuestMemory { caller, memory };
        nested_call(
            &mut host,
            &mut memory,
            &limits,
            addr_ptr,
            addr_len,
            data_ptr,
            data_len,
            value_lo,
            value_hi,
        )
    };

    let left = host.gas_left();
    let _ = caller.set_fuel(left);
    let data = caller.data_mut();
    data.boundary = left;
    data.host = Some(host);
    code
}

/// The body of `env3.call`, engine-independent apart from the child run.
#[allow(clippy::too_many_arguments)]
fn nested_call(
    host: &mut HostState,
    mem: &mut GuestMemory<'_, '_>,
    limits: &SandboxLimits,
    addr_ptr: i32,
    addr_len: i32,
    data_ptr: i32,
    data_len: i32,
    value_lo: i64,
    value_hi: i64,
) -> i32 {
    let Ok(addr_ptr) = u32::try_from(addr_ptr) else {
        return AbiError::OutOfMemory.code();
    };
    let Ok(addr_len) = u32::try_from(addr_len) else {
        return AbiError::OutOfMemory.code();
    };
    let Ok(data_ptr) = u32::try_from(data_ptr) else {
        return AbiError::OutOfMemory.code();
    };
    let Ok(data_len) = u32::try_from(data_len) else {
        return AbiError::OutOfMemory.code();
    };
    let Some(raw_address) = mem.read(addr_ptr, addr_len) else {
        return AbiError::OutOfMemory.code();
    };
    if raw_address.len() != abi_v2::ADDRESS_LEN {
        // A 19- or 21-byte "address" is a caller bug, and guessing which byte to
        // drop would be a way to reach an account the caller did not name.
        return AbiError::OutOfMemory.code();
    }
    let Ok(target) = aura_types::Address::from_slice(&raw_address) else {
        return AbiError::OutOfMemory.code();
    };
    let Some(calldata) = mem.read(data_ptr, data_len) else {
        return AbiError::OutOfMemory.code();
    };
    if calldata.len() > abi_v2::MAX_CALLDATA_LEN {
        return AbiError::ValueTooLong.code();
    }
    let gas = abi_v3::gas_for("call", (raw_address.len() + calldata.len()) as u64).unwrap_or(0);
    if host.charge(gas).is_err() {
        return AbiError::OutOfGas.code();
    }
    let value = match abi_v3::amount_from_halves(value_lo, value_hi) {
        Ok(value) => value,
        Err(error) => return error.code(),
    };
    // Policy before code: a refused call must not even reveal whether the target
    // is a contract.
    if let Err(error) = abi_v3::admit(&host.frames, &target) {
        return error.code();
    }
    let record = match code_of(&host.view, &target) {
        Ok(Some(record)) => record,
        _ => return AbiError::NoCode.code(),
    };
    let undo = match host.stage_value(&target, aura_types::Amount::from_units(value)) {
        Ok(undo) => undo,
        Err(error) => return error.code(),
    };

    // The child sees this frame's writes — including the value just staged — and
    // nothing else: its base is our overlay materialised at our version.
    let version = host.view.base_version() + 1;
    let base = host.view.materialize(version);
    let mut frames = host.frames.clone();
    frames.push(target);
    let child_call = ContractCall {
        contract: target,
        caller: host.call.contract,
        block: host.call.block,
        gas_limit: host.gas_left(),
    };
    let Ok(executor) = Executor::with_engine(*limits, WasmiEngine) else {
        return AbiError::OutOfMemory.code();
    };
    let mut session = executor.open(&base, child_call);
    session.set_frames(frames);
    if session.set_calldata(calldata).is_err() {
        if let Some(undo) = undo {
            host.undo_value(undo);
        }
        return AbiError::ValueTooLong.code();
    }
    if let Err(error) = session.run(&record.code, RUN_ENTRY, &[]) {
        if let Some(undo) = undo {
            host.undo_value(undo);
        }
        host.returndata.clear();
        // A callee the sandbox refused outright is a failed call, not a trap in
        // this frame: the caller gets `-17` and decides what to do with it.
        let _ = error;
        return AbiError::CalleeFailed.code();
    }
    let receipt = session.finish();
    host.calls += 1;

    // Gas is spent either way: a failed callee still ran, and its instructions
    // were metered by its own store.
    let charged = host.charge(receipt.gas_used).is_ok();
    if !charged {
        host.out_of_gas = true;
    }
    if !receipt.status.is_success() {
        if let Some(undo) = undo {
            host.undo_value(undo);
        }
        host.returndata.clear();
        if matches!(receipt.status, crate::contract::ExecStatus::OutOfGas) {
            host.out_of_gas = true;
            return AbiError::OutOfGas.code();
        }
        return AbiError::CalleeFailed.code();
    }
    if let Some(diff) = &receipt.diff {
        host.merge_child(diff);
    }
    host.merge_events(&receipt.events);
    host.returndata = receipt.return_data;
    if host.out_of_gas {
        return AbiError::OutOfGas.code();
    }
    // A committed callee answers with its own status code, and the caller sees
    // it: `0` is success, a negative value is the callee's refusal — the X5
    // standards answer `-6` and `-24`, and a caller must be able to read that.
    // Anything wider than an `i32` is a status this ABI cannot carry, so it is
    // clamped to the ABI's own "unknown" answer rather than truncated silently.
    match receipt.status {
        crate::contract::ExecStatus::Returned(value) => {
            i32::try_from(value).unwrap_or(AbiError::CalleeFailed.code())
        }
        _ => 0,
    }
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

    // ABI v2 lives in its own module name, so a v1 module is unaffected and a
    // contract declares which surface it speaks by what it imports.
    macro_rules! wrap2 {
        ($name:literal, $func:expr $(, $arg:ident)*) => {
            linker
                .func_wrap(
                    abi_v2::ABI2_MODULE,
                    $name,
                    |mut caller: Caller<'_, State>, $($arg: i32),*| -> i32 {
                        host_abi2(&mut caller, $func, &[$($arg),*])
                    },
                )
                .map_err(SandboxError::instantiate)?;
        };
    }
    macro_rules! wrap3 {
        ($name:literal, $func:expr $(, $arg:ident)*) => {
            linker
                .func_wrap(
                    abi_v3::ABI3_MODULE,
                    $name,
                    |mut caller: Caller<'_, State>, $($arg: i32),*| -> i32 {
                        host_call(&mut caller, &[$($arg),*], |host, memory, args| {
                            abi_v3::dispatch(host, memory, $func, args)
                        })
                    },
                )
                .map_err(SandboxError::instantiate)?;
        };
    }
    wrap2!("calldata_len", Abi2Fn::CalldataLen);
    wrap2!("calldata_read", Abi2Fn::CalldataRead, a, b);
    wrap2!("return_write", Abi2Fn::ReturnWrite, a, b);
    wrap2!("caller", Abi2Fn::Caller, a, b);
    wrap2!("self_address", Abi2Fn::SelfAddress, a, b);
    wrap2!("abi_version", Abi2Fn::AbiVersion);

    // ABI v3 (`env3`): the only surface that can re-enter the machine. `call`
    // takes two `i64` halves for the `u128` value, so it is linked by hand
    // rather than through the `i32`-only macro above.
    linker
        .func_wrap(
            abi_v3::ABI3_MODULE,
            "call",
            |mut caller: Caller<'_, State>,
             addr_ptr: i32,
             addr_len: i32,
             data_ptr: i32,
             data_len: i32,
             value_lo: i64,
             value_hi: i64|
             -> i32 {
                host_abi3_call(
                    &mut caller,
                    addr_ptr,
                    addr_len,
                    data_ptr,
                    data_len,
                    value_lo,
                    value_hi,
                )
            },
        )
        .map_err(SandboxError::instantiate)?;
    wrap3!("returndata_len", Abi3Fn::ReturnDataLen);
    wrap3!("returndata_read", Abi3Fn::ReturnDataRead, a, b);
    wrap3!("depth", Abi3Fn::Depth);
    Ok(())
}

/// Reject every import that is not part of ABI v1, v2 or v3.
///
/// X1 refused *all* imports; X2 refuses every import it cannot account for, and
/// X5 adds the `env2` surface and X6 the `env3` one — three closed lists,
/// checked the same way.
/// The set is closed and named: an unknown `env.*` name, a different module, a
/// memory or a global import — all of them are refusal, not a link failure that
/// a later phase could paper over.
fn check_imports(module: &Module) -> Result<usize, SandboxError> {
    let mut allowed = 0usize;
    for import in module.imports() {
        let name = import.name();
        let func_import = matches!(import.ty(), wasmi::ExternType::Func(_));
        let ok = func_import
            && ((import.module() == abi::ABI_MODULE && AbiFn::from_name(name).is_some())
                || (import.module() == abi_v2::ABI2_MODULE && Abi2Fn::from_name(name).is_some())
                || (import.module() == abi_v3::ABI3_MODULE && Abi3Fn::from_name(name).is_some()));
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
        // determinism: no floats anywhere, no threads.
        //
        // Mutable globals and bulk memory are **enabled** because the engine
        // mirrors the *hosted* policy: `memory.copy`/`memory.fill` are exactly
        // specified byte moves, and a module-level mutable global is private to
        // one instantiation (reset on every call). What is allowed is decided by
        // the X0 census, which runs before the engine and is what X4 relaxed —
        // the engine flag only stops a valid module from being rejected after
        // the census accepted it.
        .floats(false)
        .wasm_mutable_global(true)
        .wasm_bulk_memory(true)
        .wasm_reference_types(true)
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
