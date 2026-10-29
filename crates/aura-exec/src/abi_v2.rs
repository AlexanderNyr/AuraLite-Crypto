//! X5 — ABI v2 (`env2`): calldata, return data, and the caller's address.
//!
//! # Why a second module instead of a new version of `env`
//!
//! X2 froze ABI v1: seven functions, fixed arities, fixed prices, and 23 golden
//! vectors that are checksummed in `docs/evidence/x2/`. Renaming a function or
//! bumping a price there would invalidate a closed phase's receipts. So the
//! extension lives in a **different module name**, exactly the way
//! `wasi_snapshot_preview1` identifies a surface by its name. A v1 contract
//! imports `env` only and is bit-for-bit unaffected; a contract that wants
//! calldata imports `env2` and says so in its bytecode.
//!
//! # What v2 adds, and why each function is needed
//!
//! | function | why a standard cannot exist without it |
//! |---|---|
//! | `calldata_len` | the X3 lifecycle passes at most one `i32`; a token has many methods |
//! | `calldata_read` | method selector and arguments must arrive as bytes |
//! | `return_write` | `balance_of` has to answer with a `u128`, not a status |
//! | `caller` | `transfer` must move **the caller's** tokens; a caller-supplied `from` is spoofable |
//! | `self_address` | a contract that cannot name itself cannot read its own balance, and a multisig must |
//!
//! # Determinism and cost
//!
//! Calldata is a `Vec<u8>` fixed before execution and immutable during it, so
//! every node reads the same bytes. Reading is charged per byte actually copied;
//! writing the return buffer is charged per byte and bounded by
//! [`MAX_RETURN_LEN`], so a contract cannot make a receipt unbounded.
//!
//! # Memory protocol
//!
//! Same rule as v1: `i32` offsets into the contract's own memory, bounds-checked
//! before anything happens. A bad range is [`AbiError::OutOfMemory`] and is not
//! partially applied.

use aura_types::Address;

use crate::abi::{AbiError, GasRow, LinearMemory};
use crate::contract::HostState;

/// Module name every ABI v2 import must use.
pub const ABI2_MODULE: &str = "env2";

/// ABI v2 surface version, reported to contracts through `env2.abi_version`.
pub const ABI2_VERSION: u32 = 2;

/// Longest calldata a call may carry, in bytes.
///
/// One kilobyte is far more than any standard needs (ARC-20 `transfer_from` is
/// 4 + 20 + 20 + 16 = 60 bytes) and small enough that the per-call copy cannot
/// become a way to buy unbounded work.
pub const MAX_CALLDATA_LEN: usize = 1024;

/// Longest return data a call may produce, in bytes.
pub const MAX_RETURN_LEN: usize = 1024;

/// Address length, repeated here so the guest side can assert it.
pub const ADDRESS_LEN: usize = 20;

/// The ABI v2 function set, in canonical order.
pub const ABI2_FUNCTIONS: [&str; 6] = [
    "calldata_len",
    "calldata_read",
    "return_write",
    "caller",
    "self_address",
    "abi_version",
];

/// Gas for one ABI v2 call. Same shape as v1: a base plus a per-byte term.
pub const ABI2_GAS_TABLE: [GasRow; 6] = [
    GasRow {
        name: "calldata_len",
        base: 20,
        per_byte: 0,
        note: "a length, not a copy",
    },
    GasRow {
        name: "calldata_read",
        base: 20,
        per_byte: 3,
        note: "cheaper than storage: the bytes are already in memory",
    },
    GasRow {
        name: "return_write",
        base: 20,
        per_byte: 3,
        note: "bounded by MAX_RETURN_LEN, copied into the receipt",
    },
    GasRow {
        name: "caller",
        base: 20,
        per_byte: 0,
        note: "20 bytes of consensus identity",
    },
    GasRow {
        name: "self_address",
        base: 20,
        per_byte: 0,
        note: "20 bytes of consensus identity, like `caller`",
    },
    GasRow {
        name: "abi_version",
        base: 20,
        per_byte: 0,
        note: "a constant; a contract can assert the surface it was built for",
    },
];

/// ABI v2 failure codes. The v1 codes keep their numbers; v2 adds its own.
pub const ERR_BAD_POINTER: i32 = -1;
/// Calldata or return data longer than the published limit.
pub const ERR_TOO_LONG: i32 = -3;
/// The output buffer is smaller than the value the host must write.
pub const ERR_BUFFER_TOO_SMALL: i32 = -13;

/// Which ABI v2 function is being dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abi2Fn {
    /// `calldata_len` — `() -> i32` (the length, `>= 0`).
    CalldataLen,
    /// `calldata_read(ptr, len)` — `(i32, i32) -> i32` (bytes copied, or a status).
    CalldataRead,
    /// `return_write(ptr, len)` — `(i32, i32) -> i32`.
    ReturnWrite,
    /// `caller(ptr, len)` — `(i32, i32) -> i32` (20, or a status).
    Caller,
    /// `self_address(ptr, len)` — `(i32, i32) -> i32` (20, or a status).
    SelfAddress,
    /// `abi_version()` — `() -> i32`.
    AbiVersion,
}

impl Abi2Fn {
    /// Map an import name to the dispatcher target.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "calldata_len" => Some(Self::CalldataLen),
            "calldata_read" => Some(Self::CalldataRead),
            "return_write" => Some(Self::ReturnWrite),
            "caller" => Some(Self::Caller),
            "self_address" => Some(Self::SelfAddress),
            "abi_version" => Some(Self::AbiVersion),
            _ => None,
        }
    }

    /// Published name (the gas table key).
    pub const fn name(self) -> &'static str {
        match self {
            Self::CalldataLen => "calldata_len",
            Self::CalldataRead => "calldata_read",
            Self::ReturnWrite => "return_write",
            Self::Caller => "caller",
            Self::SelfAddress => "self_address",
            Self::AbiVersion => "abi_version",
        }
    }

    /// Parameter count of the import.
    pub const fn arity(self) -> usize {
        match self {
            Self::CalldataLen | Self::AbiVersion => 0,
            Self::CalldataRead | Self::ReturnWrite | Self::Caller | Self::SelfAddress => 2,
        }
    }
}

/// True when `module.name` is an ABI v2 import.
pub fn is_abi2_import(module: &str, name: &str) -> bool {
    module == ABI2_MODULE && Abi2Fn::from_name(name).is_some()
}

/// Gas for one ABI v2 call carrying `bytes` payload bytes.
pub fn gas_for(name: &str, bytes: u64) -> Option<u64> {
    ABI2_GAS_TABLE
        .iter()
        .find(|row| row.name == name)
        .map(|row| row.total(bytes))
}

fn u32_of(value: i32) -> Result<u32, AbiError> {
    u32::try_from(value).map_err(|_| AbiError::OutOfMemory)
}

/// Dispatch one ABI v2 call and return the status the contract sees.
///
/// Same contract as v1: the price is charged **before** the effect, and an
/// out-of-gas is recorded on `host` so the engine turns it into a trap.
pub fn dispatch(
    host: &mut HostState,
    mem: &mut dyn LinearMemory,
    func: Abi2Fn,
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

fn charge(host: &mut HostState, func: Abi2Fn, bytes: u64) -> Result<(), AbiError> {
    let gas = gas_for(func.name(), bytes).ok_or(AbiError::Overflow)?;
    host.charge(gas)
}

fn call(
    host: &mut HostState,
    mem: &mut dyn LinearMemory,
    func: Abi2Fn,
    args: &[i32],
) -> Result<i32, AbiError> {
    if args.len() != func.arity() {
        return Err(AbiError::OutOfMemory);
    }
    match func {
        // The calldata length needs no memory access and no variable charge: it
        // is the number the contract uses to decide whether to read at all.
        Abi2Fn::CalldataLen => {
            charge(host, func, 0)?;
            Ok(host.calldata.len() as i32)
        }
        Abi2Fn::AbiVersion => {
            charge(host, func, 0)?;
            Ok(ABI2_VERSION as i32)
        }
        Abi2Fn::CalldataRead => {
            let (dst, len) = (u32_of(args[0])?, u32_of(args[1])?);
            let n = (len as usize).min(host.calldata.len());
            charge(host, func, n as u64)?;
            let slice = host.calldata[..n].to_vec();
            if !mem.write(dst, &slice) {
                return Err(AbiError::OutOfMemory);
            }
            Ok(n as i32)
        }
        Abi2Fn::ReturnWrite => {
            let (src, len) = (u32_of(args[0])?, u32_of(args[1])?);
            if len as usize > MAX_RETURN_LEN {
                return Ok(ERR_TOO_LONG);
            }
            charge(host, func, u64::from(len))?;
            let bytes = mem.read(src, len).ok_or(AbiError::OutOfMemory)?;
            // Appending twice is a contract bug, not a host decision: the second
            // write replaces the first, so the receipt always holds one answer.
            host.return_data = bytes;
            Ok(0)
        }
        Abi2Fn::Caller | Abi2Fn::SelfAddress => {
            let (dst, len) = (u32_of(args[0])?, u32_of(args[1])?);
            charge(host, func, 0)?;
            if (len as usize) < ADDRESS_LEN {
                return Ok(ERR_BUFFER_TOO_SMALL);
            }
            let address = match func {
                Abi2Fn::Caller => host.call.caller,
                _ => host.call.contract,
            };
            let bytes = address.as_bytes().to_vec();
            if !mem.write(dst, &bytes) {
                return Err(AbiError::OutOfMemory);
            }
            Ok(ADDRESS_LEN as i32)
        }
    }
}

/// Bridge from an `Address` to the fixed-size array a contract stores.
pub fn address_bytes(address: &Address) -> [u8; ADDRESS_LEN] {
    *address.as_bytes()
}
