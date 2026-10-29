//! `aura-exec` — deterministic WASM execution for AuraLite-Crypto.
//!
//! Two phases live here:
//!
//! * **X0 — [`census`]:** the static determinism census. It parses a WASM
//!   binary, applies the restrictive configuration
//!   ([`ModuleLimits::restrictive`]) and decides whether a module *can* be
//!   deterministic: floats, imports, the start section, shared memory, mutable
//!   globals, SIMD/atomics/bulk ops and indirect calls are banned, with a
//!   fail-closed opcode whitelist.
//! * **X1 — [`sandbox`]:** the runtime half. A module that survives the census
//!   is compiled and instantiated by an engine with a fuel budget, a memory
//!   ceiling, a stack ceiling and **zero host imports**, then `run` is called
//!   once and the trap is classified into a closed set.
//!
//! The engine is behind the [`sandbox::Engine`] trait. The default build links
//! `wasmi` ([`wasmi_engine`]), a deterministic interpreter that builds and runs
//! everywhere; `wasmtime` is behind the `wasmtime-engine` feature
//! ([`wasmtime_engine`]) and is gated by its own CI job. Both are configured
//! from the same [`SandboxLimits`], which is what makes the X7 cross-engine
//! determinism gate meaningful.
//!
//! Golden vectors: `tools/gen_x0_vectors.py`, `tools/gen_x1_vectors.py`
//! (independent Python assembler — the crate never builds its own inputs).
//! Reports: `docs/evidence/x0/report.md`, `docs/evidence/x1/report.md`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod abi;
pub mod census;
pub mod contract;
pub mod error;
pub mod lifecycle;
pub mod limits;
pub mod sandbox;
pub mod wasmi_engine;

#[cfg(feature = "wasmtime-engine")]
pub mod wasmtime_engine;

pub use abi::{ABI_FUNCTIONS, ABI_MODULE, ABI_VERSION, AbiError, GAS_TABLE, GasRow};
pub use census::{
    Ban, CENSUS, CensusReport, CensusRow, MAX_MEMORY_PAGES, MAX_MODULE_BYTES, MAX_TABLE_ENTRIES,
    ModuleLimits, Policy, Verdict, WASM_MAGIC, WASM_VERSION, census,
};
pub use contract::{
    BlockContext, ContractCall, ContractReceipt, ExecStatus, Executor, HostState, Session,
};
pub use error::{SandboxError, TrapKind};
pub use lifecycle::{CodeRecord, Lifecycle, LifecycleError, LifecycleReceipt, LifecycleStatus};
pub use limits::{DEFAULT_FUEL, DEFAULT_MAX_STACK_BYTES, DEFAULT_MAX_TRACE_EVENTS, SandboxLimits};
pub use sandbox::{Engine, Event, Execution, Outcome, Sandbox, run, run_with_arg};
pub use wasmi_engine::WasmiEngine;
