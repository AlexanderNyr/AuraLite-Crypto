//! The engine-agnostic sandbox: census → compile → instantiate → run → trace.
//!
//! # Design
//!
//! The sandbox is a **pipeline with a fixed order**, and every step can only
//! reject:
//!
//! 1. size check against [`SandboxLimits::module_bytes`] (before any parser);
//! 2. the X0 determinism census (`census::census`) — floats, imports, start
//!    section, shared memory, mutable globals, SIMD/atomics/bulk ops, indirect
//!    calls are already gone at this point;
//! 3. engine compile with the restrictive feature set;
//! 4. **zero imports** — the sandbox resolves nothing, so a module that
//!    declares any import fails here even if the census were bypassed;
//! 5. instantiate with the store limiter and the fuel budget;
//! 6. call `run` and convert the engine trap into the closed [`TrapKind`];
//! 7. return an [`Execution`] with the outcome, the fuel used and the trace.
//!
//! # Backends
//!
//! [`Engine`] is a trait so that the rest of the stack never names a specific
//! VM. The default build links **wasmi** (`wasmi_engine::WasmiEngine`), a
//! deterministic interpreter: it builds and runs in every CI runner and on a
//! 2 GB laptop, and it gives typed `TrapCode(OutOfFuel)` plus a call hook.
//! `wasmtime` is available behind the `wasmtime-engine` feature
//! (`wasmtime_engine::WasmtimeEngine`); it needs Cranelift, which does not build
//! in 2 GB of RAM, so it is exercised by a separate CI job and by the
//! `wasmtime_backend_agrees_on_outcomes` test (which needs swap to compile).
//! Both backends are configured from the SAME [`SandboxLimits`] and produce the
//! SAME [`Execution`] shape, which is what makes the X7 cross-engine
//! determinism gate possible.
//!
//! # What is a claim and what is not
//!
//! X1 claims: a listed module runs and returns a value; an infinite loop traps
//! `out_of_fuel`; a module with imports is refused; memory growth past the
//! ceiling is refused; the engine does not accept floats. It does **not**
//! claim gas prices (X2), storage ABI (X2), lifecycle (X3) or cross-engine
//! byte-identical state roots (X7).

use crate::census::{self, MAX_MODULE_BYTES, ModuleLimits};
use crate::error::{SandboxError, TrapKind};
use crate::limits::SandboxLimits;

/// One entry in the invocation trace.
///
/// The trace is deterministic given (module, limit, input) for a fixed backend,
/// which is what the X1 replay gate measures. Records are intentionally small
/// and content-free: an event is a call into or out of WASM plus the module
/// function index, so a divergence shows up as a differing event stream. A host
/// call would add its own codes in X2 — in X1 there are none, by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// `enter` when WASM is entered, `exit` when it returns.
    pub code: &'static str,
    /// Module function index the engine reported.
    pub func_index: u32,
}

/// What an invocation produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// `run` returned this value.
    Returned(i64),
    /// `run` trapped.
    Trap(TrapKind),
}

/// The result of one invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    /// Return value or trap.
    pub outcome: Outcome,
    /// Fuel consumed (`limits.fuel - remaining`).
    pub fuel_used: u64,
    /// The trace, capped at `limits.max_trace_events`.
    pub trace: Vec<Event>,
    /// Exports of the module as `name` (sorted, deduplicated).
    pub exports: Vec<String>,
}

impl Execution {
    /// True when the call returned normally.
    pub fn returned(&self, value: i64) -> bool {
        self.outcome == Outcome::Returned(value)
    }

    /// True when the call trapped with `kind`.
    pub fn trapped(&self, kind: TrapKind) -> bool {
        self.outcome == Outcome::Trap(kind)
    }
}

/// A WASM engine backend.
pub trait Engine: core::fmt::Debug {
    /// Identifier used in evidence (`wasmi`, `wasmtime`).
    fn name(&self) -> &'static str;

    /// Exact crate version compiled into this build.
    fn version(&self) -> &'static str;

    /// Whether this backend reports a per-call fuel/depth trace.
    fn traces(&self) -> bool;

    /// Run `wasm` under `limits`. The caller has already run the census.
    ///
    /// `arg` is passed to a `(i32) -> i32` `run` export; `None` selects a
    /// `() -> i32` export. The export type decides, so a wrong call shape is an
    /// instantiate error rather than a silent zero.
    fn execute(
        &self,
        wasm: &[u8],
        limits: &SandboxLimits,
        arg: Option<i32>,
    ) -> Result<Execution, SandboxError>;

    /// Run `wasm` with the X2 host ABI linked, calling the entry point `entry`
    /// with `args`.
    ///
    /// The host state is moved in and handed back, which is what lets a backend
    /// keep it inside its store without any `unsafe` aliasing. A backend that
    /// does not implement hosted execution must say so — the default body
    /// returns [`SandboxError::EngineUnavailable`], never a fake success.
    fn execute_hosted(
        &self,
        _wasm: &[u8],
        _limits: &SandboxLimits,
        _entry: &str,
        _args: &[i32],
        _host: crate::contract::HostState,
    ) -> Result<(Execution, crate::contract::HostState), SandboxError> {
        Err(SandboxError::EngineUnavailable(
            "this backend does not implement hosted execution",
        ))
    }

    /// Arity of the export `name`, or `None` when the module does not export it.
    ///
    /// X3 needs this to distinguish "no `init`" (fine) from "`init` with the
    /// wrong signature" (a refusal). The default body refuses rather than
    /// guessing, so a backend cannot silently claim an entry point it does not
    /// understand.
    fn entry_arity(
        &self,
        _wasm: &[u8],
        _limits: &SandboxLimits,
        _name: &str,
    ) -> Result<Option<u32>, SandboxError> {
        Err(SandboxError::EngineUnavailable(
            "this backend cannot inspect entry points",
        ))
    }
}

/// A sandbox bound to one engine implementation.
pub struct Sandbox<E: Engine = crate::wasmi_engine::WasmiEngine> {
    limits: SandboxLimits,
    engine: E,
}

impl Sandbox<crate::wasmi_engine::WasmiEngine> {
    /// The default sandbox: Stage 1 limits on the default (wasmi) backend.
    pub fn new() -> Self {
        Self::with_limits(SandboxLimits::restrictive())
            .expect("SandboxLimits::restrictive() is valid by construction")
    }

    /// The default backend with explicit limits (tests tighten one limit).
    pub fn with_limits(limits: SandboxLimits) -> Result<Self, SandboxError> {
        Self::with(limits, crate::wasmi_engine::WasmiEngine)
    }
}

impl Default for Sandbox<crate::wasmi_engine::WasmiEngine> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Engine> Sandbox<E> {
    /// Build a sandbox with explicit limits and an explicit backend.
    pub fn with(limits: SandboxLimits, engine: E) -> Result<Self, SandboxError> {
        limits.validate().map_err(SandboxError::Limit)?;
        Ok(Self { limits, engine })
    }

    /// The limits in force.
    pub fn limits(&self) -> &SandboxLimits {
        &self.limits
    }

    /// The backend in force.
    pub fn engine(&self) -> &E {
        &self.engine
    }

    /// Full pipeline: size → census → compile → imports → run.
    ///
    /// `arg` is passed to an exported `run` of type `(i32) -> i32`; pass
    /// `None` for a `() -> i32` entry point. Which one is used is decided by
    /// the module's actual export type, not by a flag, so a wrong call shape
    /// cannot be silently accepted.
    pub fn execute(&self, wasm: &[u8], arg: Option<i32>) -> Result<Execution, SandboxError> {
        self.gate(wasm)?;
        self.engine.execute(wasm, &self.limits, arg)
    }

    /// The static half of the pipeline, exposed so that a deploy path (X3) can
    /// validate without executing.
    pub fn gate(&self, wasm: &[u8]) -> Result<(), SandboxError> {
        if wasm.len() > self.limits.module_bytes as usize {
            return Err(SandboxError::Limit("module_bytes"));
        }
        if wasm.len() > MAX_MODULE_BYTES {
            return Err(SandboxError::Censored(census::Ban::ModuleTooLarge(
                wasm.len(),
            )));
        }
        let census_limits = ModuleLimits::restrictive();
        let report = census::census(wasm, &census_limits);
        if let Some(ban) = report.ban() {
            return Err(SandboxError::Censored(ban.clone()));
        }
        Ok(())
    }
}

/// Run `wasm` once with the Stage 1 configuration on the default backend.
pub fn run(wasm: &[u8]) -> Result<Execution, SandboxError> {
    Sandbox::new().execute(wasm, None)
}

/// Run `wasm` once with an `i32` argument on the default backend.
pub fn run_with_arg(wasm: &[u8], arg: i32) -> Result<Execution, SandboxError> {
    Sandbox::new().execute(wasm, Some(arg))
}
