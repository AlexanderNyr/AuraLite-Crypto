//! X6 — ABI v3 (`env3`): one contract calling another.
//!
//! # Why this is a third module and not more functions in `env2`
//!
//! X5's `env2` describes a **leaf**: a contract that reads its calldata, answers
//! and touches nobody. Calling another contract changes the shape of execution
//! itself — a frame stack, a shared budget, value that moves before the callee
//! runs and comes back if it fails — so it gets its own surface, its own prices
//! and its own status codes. A contract that imports `env3` is saying "this code
//! can re-enter the machine"; a contract that does not cannot, and that is
//! visible in its bytecode rather than promised in a document.
//!
//! # The four functions
//!
//! | function | arity | why |
//! |---|---|---|
//! | `call(addr, data, value_lo, value_hi)` | 6 | the call itself; value is a `u128` in two halves |
//! | `returndata_len()` | 0 | how much the callee answered |
//! | `returndata_read(ptr, len)` | 2 | the answer, bounded by [`MAX_RETURN_LEN`] |
//! | `depth()` | 0 | the caller can see how deep it is and refuse to go deeper |
//!
//! # The two guards this module exists for
//!
//! **Depth.** [`MAX_CALL_DEPTH`] frames, the top-level call included. A contract
//! that calls itself a thousand times costs a bounded amount of host stack; the
//! ninth frame is refused with `-15`, before any code is loaded.
//!
//! **Reentrancy.** A frame may not call an address that is already on the stack,
//! which is `-14`. This is the guard that makes the classic
//! withdraw-then-reenter attack impossible: the evil contract's second entry into
//! the vault is refused while the first is still running, so the vault's balance
//! bookkeeping cannot be read twice. It is deliberately **not** configurable —
//! single-entry reentrancy locks are the Stage 1 rule.
//!
//! # Value
//!
//! `call` may carry native balance. The transfer is written **before** the
//! callee runs (so the callee sees a funded account) into the *caller's* view,
//! and is written back if the callee fails, together with every write the callee
//! tried to make. One rule, no exceptions: a frame either commits wholly or not
//! at all.

use crate::abi::{AbiError, GasRow, LinearMemory};
use crate::contract::HostState;

/// Module name every ABI v3 import must use.
pub const ABI3_MODULE: &str = "env3";

/// Version of the surface this module publishes.
///
/// There is no `env3.abi_version` function: a contract either links `env3` or it
/// does not, and `env2.abi_version` still answers for the surface that has the
/// question. The constant exists so the census, the gas table and the docs quote
/// one number.
pub const ABI3_VERSION: u32 = 3;

/// Maximum number of frames, the top-level call included.
///
/// Eight is a **protocol parameter**. It is deep enough for a router → pool →
/// token path (3 frames) with room for a standard to call a helper, and shallow
/// enough that the host's stack is bounded by a number a reviewer can hold in
/// their head. A call that needs more is a design error, not a limit to raise.
pub const MAX_CALL_DEPTH: u32 = 8;

/// The ABI v3 function set, in canonical order.
pub const ABI3_FUNCTIONS: [&str; 4] = ["call", "returndata_len", "returndata_read", "depth"];

/// Gas table for ABI v3.
///
/// `call` is the expensive one: it may load and run a whole module. The base is
/// 100 against 20 for a storage write of the same size, because the callee's own
/// instructions are metered separately and the base must not make recursion
/// free; the per-byte term covers both directions of the copy (arguments in,
/// nothing out — the answer is read back through `returndata_read`, which is
/// priced on its own).
pub const ABI3_GAS_TABLE: [GasRow; 4] = [
    GasRow {
        name: "call",
        base: 100,
        per_byte: 3,
        note: "loads and runs another module; its instructions are metered separately",
    },
    GasRow {
        name: "returndata_len",
        base: 20,
        per_byte: 0,
        note: "a length, not a copy",
    },
    GasRow {
        name: "returndata_read",
        base: 20,
        per_byte: 3,
        note: "bounded by MAX_RETURN_LEN",
    },
    GasRow {
        name: "depth",
        base: 20,
        per_byte: 0,
        note: "the caller's own frame index",
    },
];

/// Which ABI v3 function is being dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abi3Fn {
    /// `call(addr_ptr, addr_len, data_ptr, data_len, value_lo, value_hi)`.
    Call,
    /// `returndata_len() -> i32`.
    ReturnDataLen,
    /// `returndata_read(ptr, len) -> i32`.
    ReturnDataRead,
    /// `depth() -> i32`.
    Depth,
}

impl Abi3Fn {
    /// Map an import name to the dispatcher target.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "call" => Some(Self::Call),
            "returndata_len" => Some(Self::ReturnDataLen),
            "returndata_read" => Some(Self::ReturnDataRead),
            "depth" => Some(Self::Depth),
            _ => None,
        }
    }

    /// Published name (the gas table key).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::ReturnDataLen => "returndata_len",
            Self::ReturnDataRead => "returndata_read",
            Self::Depth => "depth",
        }
    }

    /// Parameter count of the import.
    pub const fn arity(self) -> usize {
        match self {
            Self::Call => 6,
            Self::ReturnDataLen | Self::Depth => 0,
            Self::ReturnDataRead => 2,
        }
    }
}

/// True when `module.name` is an ABI v3 import.
pub fn is_abi3_import(module: &str, name: &str) -> bool {
    module == ABI3_MODULE && Abi3Fn::from_name(name).is_some()
}

/// Gas for one ABI v3 call carrying `bytes` payload bytes.
pub fn gas_for(name: &str, bytes: u64) -> Option<u64> {
    ABI3_GAS_TABLE
        .iter()
        .find(|row| row.name == name)
        .map(|row| row.total(bytes))
}

/// Rebuild a `u128` amount from the two `i64` halves the ABI carries.
///
/// Negative halves are refused rather than reinterpreted: a caller that passes
/// `-1` for the high half is asking to move 2^128-1 units, and the answer must be
/// a refusal, not an overflow.
pub fn amount_from_halves(lo: i64, hi: i64) -> Result<u128, AbiError> {
    if lo < 0 || hi < 0 {
        return Err(AbiError::Overflow);
    }
    Ok((u128::from(hi as u64) << 64) | u128::from(lo as u64))
}

/// The policy check every call passes before a module is loaded.
///
/// Two rules, in this order: a frame may not call an address already on the
/// stack ([`AbiError::Reentrancy`]), and the stack may not exceed
/// [`MAX_CALL_DEPTH`] frames ([`AbiError::CallDepth`]). Both are refused *before*
/// any code lookup, so a refused call cannot even be used to probe whether an
/// address is a contract.
pub fn admit(frames: &[aura_types::Address], target: &aura_types::Address) -> Result<(), AbiError> {
    if frames.iter().any(|frame| frame == target) {
        return Err(AbiError::Reentrancy);
    }
    if frames.len() as u32 >= MAX_CALL_DEPTH {
        return Err(AbiError::CallDepth);
    }
    Ok(())
}

fn u32_of(value: i32) -> Result<u32, AbiError> {
    u32::try_from(value).map_err(|_| AbiError::OutOfMemory)
}

/// Dispatch one **pure** ABI v3 call and return the status the contract sees.
///
/// `call` is deliberately not here: it executes another module, so the engine
/// owns it (see `wasmi_engine::host_abi3_call`). Everything that only reads the
/// host state — the answer of the last call, and the current depth — is priced
/// and bounds-checked here, next to its gas row.
pub fn dispatch(
    host: &mut HostState,
    mem: &mut dyn LinearMemory,
    func: Abi3Fn,
    args: &[i32],
) -> i32 {
    match call(host, mem, func, args) {
        Ok(code) => code,
        Err(AbiError::OutOfGas) => {
            host.out_of_gas = true;
            AbiError::OutOfGas.code()
        }
        Err(error) => error.code(),
    }
}

fn charge(host: &mut HostState, func: Abi3Fn, bytes: u64) -> Result<(), AbiError> {
    let gas = gas_for(func.name(), bytes).ok_or(AbiError::Overflow)?;
    host.charge(gas)
}

fn call(
    host: &mut HostState,
    mem: &mut dyn LinearMemory,
    func: Abi3Fn,
    args: &[i32],
) -> Result<i32, AbiError> {
    if args.len() != func.arity() {
        return Err(AbiError::OutOfMemory);
    }
    match func {
        Abi3Fn::ReturnDataLen => {
            charge(host, func, 0)?;
            Ok(host.returndata.len() as i32)
        }
        Abi3Fn::ReturnDataRead => {
            let (dst, len) = (u32_of(args[0])?, u32_of(args[1])?);
            let n = (len as usize).min(host.returndata.len());
            charge(host, func, n as u64)?;
            let slice = host.returndata[..n].to_vec();
            if !mem.write(dst, &slice) {
                return Err(AbiError::OutOfMemory);
            }
            Ok(n as i32)
        }
        Abi3Fn::Depth => {
            charge(host, func, 0)?;
            Ok(host.frames.len() as i32)
        }
        // Executed by the engine, never by this module.
        Abi3Fn::Call => Err(AbiError::OutOfMemory),
    }
}
