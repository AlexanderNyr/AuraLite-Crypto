//! ABI v3 (`env3`) as a contract sees it: one contract calling another.
//!
//! | function | what it does |
//! |---|---|
//! | [`call_contract`] | run another deployed contract, optionally sending value |
//! | [`returndata`] | read what that call answered |
//! | [`depth`] | how many frames are live, this one included |
//!
//! # What a contract may not do
//!
//! A frame may not call an address that is already on the stack (`-14`), and the
//! stack may not exceed eight frames (`-15`). Both are refused by the node
//! before any code is loaded, so a contract cannot use a refused call to discover
//! whether an address exists. A contract that wants to know how deep it is can
//! ask [`depth`] and refuse the call itself.
//!
//! # Atomicity
//!
//! [`call_contract`] returns `Ok(())` only when the callee **committed**. A
//! `CalleeFailed` means every write the callee attempted, and any value sent with
//! it, was discarded — there is no half-applied callee to clean up, and no need
//! for a contract to compensate.

use crate::abi::{ADDRESS_LEN, Error};

/// Import module of ABI v3.
pub const ABI3_MODULE: &str = "env3";

/// Maximum number of frames the node allows, this contract's own included.
pub const MAX_CALL_DEPTH: u32 = 8;

mod raw {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "env3")]
    unsafe extern "C" {
        pub fn call(
            addr: *const u8,
            addr_len: usize,
            data: *const u8,
            data_len: usize,
            value_lo: i64,
            value_hi: i64,
        ) -> i32;
        pub fn returndata_len() -> i32;
        pub fn returndata_read(dst: *mut u8, len: usize) -> i32;
        pub fn depth() -> i32;
    }

    #[cfg(not(target_arch = "wasm32"))]
    unsafe extern "C" {
        pub fn call(
            addr: *const u8,
            addr_len: usize,
            data: *const u8,
            data_len: usize,
            value_lo: i64,
            value_hi: i64,
        ) -> i32;
        pub fn returndata_len() -> i32;
        pub fn returndata_read(dst: *mut u8, len: usize) -> i32;
        pub fn depth() -> i32;
    }
}

/// Call `target` with `calldata`, sending `value` units of the native coin.
///
/// The answer is available afterwards through [`returndata`]. An empty answer is
/// not distinguishable from "the callee wrote nothing" by design: a method that
/// must answer has to be read back explicitly.
pub fn call_contract(
    target: &[u8; ADDRESS_LEN],
    calldata: &[u8],
    value: u128,
) -> Result<(), Error> {
    let lo = value as u64 as i64;
    let hi = (value >> 64) as u64 as i64;
    let code = unsafe {
        raw::call(
            target.as_ptr(),
            ADDRESS_LEN,
            calldata.as_ptr(),
            calldata.len(),
            lo,
            hi,
        )
    };
    if code == 0 {
        Ok(())
    } else {
        Err(Error::from_code(code))
    }
}

/// Copy the last callee's answer into `buf`; returns how many bytes were copied.
pub fn returndata(buf: &mut [u8]) -> Result<usize, Error> {
    let code = unsafe { raw::returndata_read(buf.as_mut_ptr(), buf.len()) };
    if code < 0 {
        return Err(Error::from_code(code));
    }
    Ok(code as usize)
}

/// Length of the last callee's answer.
pub fn returndata_len() -> Result<usize, Error> {
    let code = unsafe { raw::returndata_len() };
    if code < 0 {
        return Err(Error::from_code(code));
    }
    Ok(code as usize)
}

/// How many frames are live, this contract's own included.
///
/// A top-level transaction is depth 1; the deepest frame the node allows is
/// [`MAX_CALL_DEPTH`].
pub fn depth() -> Result<u32, Error> {
    let code = unsafe { raw::depth() };
    if code < 0 {
        return Err(Error::from_code(code));
    }
    Ok(code as u32)
}
