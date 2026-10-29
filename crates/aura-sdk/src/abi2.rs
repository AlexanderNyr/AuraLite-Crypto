//! ABI v2 (`env2`) as a contract sees it: calldata, return data, caller.
//!
//! ABI v1 gave a contract seven functions but only one `i32` of input from the
//! outside world, which is enough for a counter and not enough for a token. v2
//! adds the four things a **standard** needs, in a *separate import module name*
//! so a v1 contract is untouched:
//!
//! | function | what it answers |
//! |---|---|
//! | [`calldata`] | the bytes the caller sent, decoded into a buffer you own |
//! | [`reply`] | the bytes the caller gets back — only if the call commits |
//! | [`caller`] | the address that signed the transaction |
//! | [`self_address`] | the address this contract is deployed at |
//! | [`version`] | the ABI v2 surface version (2) |
//!
//! # Why `caller` and not a `from` argument
//!
//! `transfer(from, to, amount)` with a caller-supplied `from` is not a transfer:
//! anyone can spend anyone's tokens by naming them. The address a contract acts
//! on behalf of must come from the chain, so it comes from here and nowhere
//! else. A standard that takes `from` from calldata is a bug, not a style
//! choice.
//!
//! # Two builds, one source
//!
//! As in [`crate::abi`], the same `extern "C"` signatures resolve to the node on
//! `wasm32` and to the mock on the host.

use crate::abi::{ADDRESS_LEN, Error};

/// Import module of ABI v2.
pub const ABI2_MODULE: &str = "env2";

/// The ABI v2 surface version this SDK speaks.
pub const ABI2_VERSION: u32 = 2;

/// Longest calldata a call may carry (bytes) — the node refuses more.
pub const MAX_CALLDATA_LEN: usize = 1024;

/// Longest return data a call may produce (bytes) — the node refuses more.
pub const MAX_RETURN_LEN: usize = 1024;

/// `-13`: the output buffer was smaller than the value the host must write.
const STATUS_BUFFER_TOO_SMALL: i32 = -13;

/// `-20`: the calldata does not start with a selector the contract knows.
///
/// SDK-level, not host-level: the node never returns it. It is negative so a
/// caller that checks `status < 0` sees the failure without knowing the layout.
pub const ERR_BAD_SELECTOR: i32 = -20;

/// `-21`: the argument list did not decode — wrong count, wrong width, or a
/// trailing byte. Refusing is the only safe answer: guessing would invent a
/// transfer amount.
pub const ERR_BAD_ARGS: i32 = -21;

// The raw imports live in one private module so a wrapper can carry the same
// human name as the node's function without colliding with the symbol (the same
// rule `abi` follows).
mod raw {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "env2")]
    unsafe extern "C" {
        pub fn calldata_len() -> i32;
        pub fn calldata_read(dst: *mut u8, len: usize) -> i32;
        pub fn return_write(src: *const u8, len: usize) -> i32;
        pub fn caller(out: *mut u8, len: usize) -> i32;
        pub fn self_address(out: *mut u8, len: usize) -> i32;
        pub fn abi_version() -> i32;
    }

    #[cfg(not(target_arch = "wasm32"))]
    unsafe extern "C" {
        pub fn calldata_len() -> i32;
        pub fn calldata_read(dst: *mut u8, len: usize) -> i32;
        pub fn return_write(src: *const u8, len: usize) -> i32;
        pub fn caller(out: *mut u8, len: usize) -> i32;
        pub fn self_address(out: *mut u8, len: usize) -> i32;
        pub fn abi_version() -> i32;
    }
}

fn status(code: i32) -> Result<i32, Error> {
    if code >= 0 {
        Ok(code)
    } else if code == STATUS_BUFFER_TOO_SMALL {
        Err(Error::ValueTooLong)
    } else {
        Err(Error::from_code(code))
    }
}

/// Read the call's calldata into `out`; `Ok(n)` is the number of bytes copied.
///
/// The buffer is the limit: calldata longer than `out.len()` is **truncated** by
/// the host, so a contract that must see everything compares `n` with
/// [`calldata_len`].
pub fn calldata(out: &mut [u8]) -> Result<usize, Error> {
    // SAFETY: `out` is a live slice; the host bounds-checks the range and
    // returns -1 instead of writing out of bounds.
    let code = unsafe { raw::calldata_read(out.as_mut_ptr(), out.len()) };
    status(code).map(|n| n as usize)
}

/// Length of the call's calldata, without copying it.
pub fn calldata_len() -> Result<usize, Error> {
    // SAFETY: no pointers; the host returns a length or an error code.
    status(unsafe { raw::calldata_len() }).map(|n| n as usize)
}

/// Set the call's return data. Only the last call counts.
///
/// The value reaches the caller **only when the call commits**: a revert drops
/// it, so "returned 0" and "reverted after writing" cannot be confused.
pub fn reply(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_RETURN_LEN {
        return Err(Error::ValueTooLong);
    }
    // SAFETY: `bytes` is a live slice; the host copies it before returning.
    let code = unsafe { raw::return_write(bytes.as_ptr(), bytes.len()) };
    status(code).map(|_| ())
}

/// The address that signed the transaction.
///
/// For a contract called from a transaction this is the transaction's sender.
/// For a contract calling another contract it will be the immediate caller once
/// X6 lands; this phase has no cross-contract calls, so there is exactly one
/// meaning.
pub fn caller() -> Result<[u8; ADDRESS_LEN], Error> {
    let mut out = [0u8; ADDRESS_LEN];
    // SAFETY: `out` is exactly ADDRESS_LEN bytes, which is what the host writes.
    let code = unsafe { raw::caller(out.as_mut_ptr(), out.len()) };
    status(code).map(|_| out)
}

/// The address this contract is deployed at.
///
/// A contract cannot read its own account without it: a multisig that wants to
/// know whether it can cover a payment has no other way to name itself, and a
/// contract that hardcodes its address is one of the reasons upgradeable
/// contracts break.
pub fn self_address() -> Result<[u8; ADDRESS_LEN], Error> {
    let mut out = [0u8; ADDRESS_LEN];
    // SAFETY: `out` is exactly ADDRESS_LEN bytes, which is what the host writes.
    let code = unsafe { raw::self_address(out.as_mut_ptr(), out.len()) };
    status(code).map(|_| out)
}

/// The ABI v2 surface version, as the node reports it.
pub fn version() -> Result<u32, Error> {
    // SAFETY: no pointers.
    status(unsafe { raw::abi_version() }).map(|v| v as u32)
}
