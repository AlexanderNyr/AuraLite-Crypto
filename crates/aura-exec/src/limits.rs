//! Published sandbox limits and the fuel budget.
//!
//! These numbers are a **protocol parameter**, not a configuration knob: two
//! nodes that disagree about the fuel budget disagree about the block. They are
//! pinned here once, published in `docs/spec/` by the report, and any change is
//! a phase-level decision with its own patch.
//!
//! [`SandboxLimits::restrictive`] is deliberately *not* derived from engine
//! defaults — the engine is configured from these values, never the reverse.

use crate::census::{MAX_MEMORY_PAGES, MAX_MODULE_BYTES, MAX_TABLE_ENTRIES};

/// Fuel granted to one contract invocation in Stage 1.
///
/// Calibrated on the X1 fixture set: the heaviest allowed program
/// (`sum_to_n(1000)`) consumes **13,246** fuel, so 1,000,000 leaves a 75x
/// margin while still making an infinite loop terminate (measured: the `spin`
/// fixture burns the whole budget and traps `out_of_fuel`).
pub const DEFAULT_FUEL: u64 = 1_000_000;

/// Engine stack ceiling in bytes. Deep recursion must trap
/// (`trap_stack_exhausted`), not crash the node process.
pub const DEFAULT_MAX_STACK_BYTES: usize = 512 * 1024;

/// Maximum number of trace events recorded for one invocation.
///
/// A trace is an audit artefact, not consensus state; the cap keeps a hostile
/// contract from allocating unbounded memory in the observer.
pub const DEFAULT_MAX_TRACE_EVENTS: usize = 4096;

/// Maximum number of call frames, the top-level call included.
///
/// The same number as [`crate::abi_v3::MAX_CALL_DEPTH`]: the ABI refuses the
/// ninth frame to the contract, and the executor refuses it again if a session
/// is constructed with a stack that deep. Two checks over one number, because
/// the second one is what a host-side caller can trust.
pub const DEFAULT_MAX_CALL_DEPTH: u32 = crate::abi_v3::MAX_CALL_DEPTH;

/// The complete sandbox configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandboxLimits {
    /// Fuel for one invocation (1 fuel = 1 gas until X2 refines the model).
    pub fuel: u64,
    /// Maximum linear-memory pages (64 KiB each).
    pub memory_pages: u32,
    /// Maximum table entries.
    pub table_entries: u32,
    /// Maximum module size in bytes (checked before the engine sees it).
    pub module_bytes: u32,
    /// Engine stack ceiling in bytes.
    pub max_stack_bytes: usize,
    /// Maximum recorded trace events.
    pub max_trace_events: usize,
    /// Maximum call frames, the top-level call included (X6).
    pub max_call_depth: u32,
}

impl SandboxLimits {
    /// The Stage 1 configuration. Every field is the restrictive value the X0
    /// census already publishes, so the static gate and the runtime sandbox
    /// cannot drift apart.
    pub const fn restrictive() -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            memory_pages: MAX_MEMORY_PAGES,
            table_entries: MAX_TABLE_ENTRIES,
            module_bytes: MAX_MODULE_BYTES as u32,
            max_stack_bytes: DEFAULT_MAX_STACK_BYTES,
            max_trace_events: DEFAULT_MAX_TRACE_EVENTS,
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
        }
    }

    /// Bytes of linear memory allowed (`pages * 64 KiB`).
    pub const fn memory_bytes(&self) -> usize {
        self.memory_pages as usize * 65_536
    }

    /// Reject configurations that are looser than the published ceiling.
    ///
    /// The census is the authority: a caller may tighten a limit for a test,
    /// but a limit above `MAX_MEMORY_PAGES` / `MAX_MODULE_BYTES` would let a
    /// module pass X0 and then be executed under a different rule.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.module_bytes as usize > MAX_MODULE_BYTES {
            return Err("module_bytes above the census ceiling");
        }
        if self.memory_pages > MAX_MEMORY_PAGES {
            return Err("memory_pages above the census ceiling");
        }
        if self.table_entries > MAX_TABLE_ENTRIES {
            return Err("table_entries above the census ceiling");
        }
        if self.fuel == 0 {
            return Err("fuel budget must be positive");
        }
        if self.max_stack_bytes == 0 {
            return Err("stack ceiling must be positive");
        }
        if self.max_call_depth == 0 || self.max_call_depth > crate::abi_v3::MAX_CALL_DEPTH {
            return Err("max_call_depth outside the published ceiling");
        }
        Ok(())
    }
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self::restrictive()
    }
}
