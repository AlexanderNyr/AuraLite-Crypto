//! Sandbox errors — one closed set for every engine backend.
//!
//! The error type is engine-agnostic on purpose: a caller (X2 gas, X3
//! lifecycle, X6 calls) must never be able to tell which backend ran the
//! module. Backend-specific detail is kept in [`SandboxError::detail`] as a
//! string for logs only; no consensus decision may depend on it.

use core::fmt;

use crate::census::Ban;

/// Everything that can go wrong between "here are bytes" and "here is a
/// `Trap`".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SandboxError {
    /// The determinism census rejected the module (X0).
    Censored(Ban),
    /// The engine refused the module: invalid WASM, unsupported feature,
    /// too large, too deep.
    Compile(String),
    /// Instantiation failed: an import could not be resolved, a limit was
    /// exceeded, or the module tried to run a start function.
    Instantiate(String),
    /// The module declares imports. The sandbox resolves none, so this is a
    /// policy verdict, not an engine failure.
    ImportsNotAllowed {
        /// Number of declared imports.
        count: usize,
        /// First offending `module.name`, if the engine exposed one.
        first: Option<String>,
    },
    /// The request violates a published limit (module bytes, memory pages,
    /// fuel budget, trace recordings).
    Limit(&'static str),
    /// The engine is not linked into this build.
    ///
    /// This is an honest refusal, not a fake success: `wasmtime` is behind the
    /// `wasmtime-engine` feature and the default build does not have it.
    EngineUnavailable(&'static str),
    /// The module trapped at runtime.
    Trap(TrapKind),
}

impl SandboxError {
    /// Construct a compile error from anything printable.
    pub fn compile(err: impl fmt::Display) -> Self {
        Self::Compile(err.to_string())
    }

    /// Construct an instantiate error from anything printable.
    pub fn instantiate(err: impl fmt::Display) -> Self {
        Self::Instantiate(err.to_string())
    }

    /// A stable, machine-readable code. This is the only string that may
    /// appear in logs that a comparison depends on.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Censored(_) => "censored",
            Self::Compile(_) => "compile",
            Self::Instantiate(_) => "instantiate",
            Self::ImportsNotAllowed { .. } => "imports_not_allowed",
            Self::Limit(_) => "limit",
            Self::EngineUnavailable(_) => "engine_unavailable",
            Self::Trap(kind) => kind.code(),
        }
    }
}

impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Censored(ban) => write!(f, "censored: {ban}"),
            Self::Compile(detail) => write!(f, "compile: {detail}"),
            Self::Instantiate(detail) => write!(f, "instantiate: {detail}"),
            Self::ImportsNotAllowed { count, first } => match first {
                Some(name) => write!(f, "imports_not_allowed: {count} import(s), first {name}"),
                None => write!(f, "imports_not_allowed: {count} import(s)"),
            },
            Self::Limit(what) => write!(f, "limit: {what}"),
            Self::EngineUnavailable(engine) => write!(f, "engine_unavailable: {engine}"),
            Self::Trap(kind) => write!(f, "trap: {kind}"),
        }
    }
}

impl std::error::Error for SandboxError {}

/// The subset of traps the sandbox is willing to expose.
///
/// The list is closed: a backend trap that does not match one of these becomes
/// [`TrapKind::Other`] with a detail string, and X7 fails the determinism gate
/// on any `Other` it has not seen before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapKind {
    /// Fuel exhausted by instruction metering (X1).
    OutOfFuel,
    /// The gas budget ran out, either in instructions or in a host call (X2).
    OutOfGas,
    /// `unreachable` executed.
    Unreachable,
    /// Linear-memory access outside the declared bounds.
    MemoryOutOfBounds,
    /// Integer division or remainder by zero.
    IntegerDivideByZero,
    /// The engine hit its stack / recursion limit.
    StackExhausted,
    /// Anything else. Recorded with a detail string, never silently allowed.
    Other,
}

impl TrapKind {
    /// Stable code used in traces and comparisons.
    pub fn code(self) -> &'static str {
        match self {
            Self::OutOfFuel => "trap_out_of_fuel",
            Self::OutOfGas => "trap_out_of_gas",
            Self::Unreachable => "trap_unreachable",
            Self::MemoryOutOfBounds => "trap_memory_oob",
            Self::IntegerDivideByZero => "trap_div_by_zero",
            Self::StackExhausted => "trap_stack_exhausted",
            Self::Other => "trap_other",
        }
    }
}

impl fmt::Display for TrapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
