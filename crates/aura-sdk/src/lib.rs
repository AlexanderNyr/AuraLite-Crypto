//! `aura-sdk` — the AuraLite-Crypto contract SDK (X4).
//!
//! A contract is Rust, compiled to `wasm32-unknown-unknown`, and it talks to the
//! chain through **ABI v1** (X2). This crate is that ABI, typed:
//!
//! | module | what it gives a contract |
//! |---|---|
//! | [`abi`] | the seven v1 functions as safe Rust with a typed [`abi::Error`] |
//! | [`storage`] | typed, key-scoped state: [`storage::Slot`] (`u64`, `u128`, `[u8; 32]`, …) |
//! | [`codec`] | the fixed-layout encoding those slots use — no serde, no derive |
//! | [`events`] | `emit(topic, data)` with the ABI limits checked before the call |
//! | [`prelude`] | one import line for a contract crate |
//!
//! # Two builds, one source
//!
//! For `wasm32-unknown-unknown` the crate is `#![no_std]`: no allocator, no
//! panics with formatting, no float formatting machinery — nothing that would
//! drag `core::fmt` and its size into every contract. On the host the same code
//! compiles against a **mock backend** ([`abi::mock`]) that behaves like the
//! node, so the wrappers, the codec and the storage layer are unit-tested
//! rather than trusted.
//!
//! The two backends are the *same* `extern "C"` signature either way: pointers
//! and lengths into linear memory. On wasm those are the real pointers; on the
//! host they are ordinary process pointers, which is why the mock can honour the
//! exact contract the node's dispatcher honours.
//!
//! # A contract
//!
//! ```ignore
//! use aura_sdk::prelude::*;
//!
//! const COUNT: Slot<u64> = Slot::at(b"count");
//!
//! #[entrypoint]
//! pub fn run() -> u32 {
//!     let next = COUNT.get_or(0) + 1;
//!     COUNT.set(next);
//!     emit(b"count", &next.to_le_bytes());
//!     0
//! }
//! ```
//!
//! `Slot::get_or` returns the value or the default; a *failed* read is a
//! contract bug (a bad key, a truncated value), so it reverts the call instead
//! of inventing a value.
//!
//! # What the SDK deliberately does not do
//!
//! * **No allocation.** Values are fixed-layout and stack-buffered. A contract
//!   that wants variable-length data reads into a buffer it owns.
//! * **No serde / no derive macros beyond the three attributes.** Contracts are
//!   small and their encoding is published ([`codec`]) rather than inferred.
//! * **No float.** `f32`/`f64` are banned by the X0 census; the SDK has no
//!   wrapper for a banned operation.
//! * **No error type gymnastics.** [`abi::Error`] is a status code with a name.

#![cfg_attr(target_arch = "wasm32", no_std)]
#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

pub mod abi;
pub mod abi2;
pub mod abi3;
pub mod codec;
pub mod events;
pub mod storage;

/// The three attributes, re-exported so a contract imports one crate.
///
/// Available unless the SDK is built with `--no-default-features` (see
/// `Cargo.toml`): the macro crate is the only proc-macro dependency in the tree.
#[cfg(feature = "macros")]
pub use aura_sdk_macros::{constructor, entrypoint, migrate};

/// Everything a contract crate needs, in one import.
pub mod prelude {
    pub use crate::abi::{
        self, Error, abi_version, balance, block_height, block_timestamp_ms, hash, transfer,
    };
    pub use crate::abi2::{calldata, calldata_len, caller, reply, self_address};
    pub use crate::abi3::{call_contract, depth, returndata};
    pub use crate::codec::Reader;
    pub use crate::events::{emit, emit_or_abort, emit_u64, emit_u128};
    pub use crate::storage::{Key, Slot};
    pub use crate::{abi_status, dispatch};
    pub use crate::{abort, unwrap};
    #[cfg(feature = "macros")]
    pub use aura_sdk_macros::{constructor, entrypoint, migrate};
}

/// Value of the `init` export's argument when a deploy has no argument.
///
/// `Lifecycle::deploy` passes either nothing (`init` arity 0) or one `i32`
/// (arity 1); this is the "there was no argument" value, published so a
/// contract does not invent a different sentinel.
pub const NO_ARG: i32 = 0;

/// Terminate the call with a trap.
///
/// A trap reverts: the X2/X3 rule is that anything but `Returned` leaves the
/// state root byte-identical. Use this for "this call must not happen" — an
/// insufficient balance, a malformed argument — instead of returning a magic
/// number, which a caller could ignore.
#[cfg(target_arch = "wasm32")]
#[inline(never)]
pub fn abort() -> ! {
    core::arch::wasm32::unreachable()
}

/// Terminate the call with a panic on the host build.
///
/// Host tests never call a contract's `abort`; if one does, the panic names the
/// call site instead of silently returning a value.
#[cfg(not(target_arch = "wasm32"))]
#[inline(never)]
pub fn abort() -> ! {
    panic!("aura_sdk::abort: a contract call reverted")
}

/// Unwrap a result, reverting the call on error.
///
/// ```ignore
/// let balance: u128 = #unwrap(BALANCE.get()?);
/// ```
#[track_caller]
pub fn unwrap<T>(result: Result<T, abi::Error>) -> T {
    match result {
        Ok(value) => value,
        Err(_) => abort(),
    }
}

/// The `#[panic_handler]` a contract needs. `#![no_std]` requires exactly one
/// per final binary, so the SDK cannot provide it — it provides the expansion.
///
/// ```ignore
/// aura_sdk::panic_handler!();
/// ```
#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! panic_handler {
    () => {
        #[panic_handler]
        fn panic(_info: &::core::panic::PanicInfo<'_>) -> ! {
            ::core::arch::wasm32::unreachable()
        }
    };
}

/// Dispatch a call by its 4-byte selector and decode each method's arguments.
///
/// This is the shape a standard needs: one stored contract, many methods, a
/// 4-byte little-endian selector at the start of calldata and the arguments
/// after it. The macro is plain `macro_rules!` on purpose — the list of methods
/// is written **once**, in the contract, where a reader can audit it.
///
/// ```ignore
/// #[entrypoint]
/// pub fn run() -> u32 {
///     dispatch! {
///         0x01 => transfer([u8; 20], u128),
///         0x02 => approve([u8; 20], u128),
///         0x03 => balance_of([u8; 20]),
///     }
/// }
/// ```
///
/// Every refusal is explicit, none is a guess:
///
/// * calldata shorter than 4 bytes → [`abi_status::BAD_SELECTOR`];
/// * an unknown selector → [`abi_status::BAD_SELECTOR`];
/// * too few bytes for the declared arguments, or **any** byte left over →
///   [`abi_status::BAD_ARGS`]. Trailing bytes are refused so a method's meaning
///   can never depend on what a later version appends.
///
/// The buffer is [`abi2::MAX_CALLDATA_LEN`] bytes, on the stack; calldata longer
/// than that is truncated by the host and therefore fails the leftover check
/// instead of silently losing arguments.
#[macro_export]
macro_rules! dispatch {
    ($($selector:expr => $method:ident ( $($ty:ty),* )),* $(,)?) => {{
        let mut buf = [0u8; $crate::abi2::MAX_CALLDATA_LEN];
        let n = match $crate::abi2::calldata(&mut buf) {
            ::core::result::Result::Ok(n) => n,
            ::core::result::Result::Err(_) => return $crate::abi_status::OUT_OF_MEMORY as u32,
        };
        if n < 4 {
            return $crate::abi_status::BAD_SELECTOR as u32;
        }
        let selector = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let mut reader = $crate::codec::Reader::new(&buf[4..n]);
        let _ = &mut reader;
        // Chained comparisons rather than `match`: a contract writes its selectors
        // as `const`, and a `const` path is not an `expr` a match arm accepts.
        let status: i32 = {
            $(
                if selector == $selector {
                    $crate::dispatch!(@call reader, $method $(, $ty)*)
                } else
            )*
            {
                $crate::abi_status::BAD_SELECTOR
            }
        };
        status as u32
    }};

    (@call $reader:ident, $method:ident, $a:ty, $b:ty, $c:ty, $d:ty) => {{
        let first = $reader.get::<$a>();
        let second = $reader.get::<$b>();
        let third = $reader.get::<$c>();
        let fourth = $reader.get::<$d>();
        match (first, second, third, fourth) {
            (
                ::core::result::Result::Ok(a),
                ::core::result::Result::Ok(b),
                ::core::result::Result::Ok(c),
                ::core::result::Result::Ok(d),
            ) => {
                if !$reader.is_empty() {
                    $crate::abi_status::BAD_ARGS
                } else {
                    $method(a, b, c, d) as i32
                }
            }
            _ => $crate::abi_status::BAD_ARGS,
        }
    }};
    // One arm per supported arity: decode in order, then require that nothing is
    // left over. Written out instead of built from a repetition so the failure
    // path of every method is the same three lines.
    (@call $reader:ident, $method:ident) => {{
        if !$reader.is_empty() {
            $crate::abi_status::BAD_ARGS
        } else {
            $method() as i32
        }
    }};
    (@call $reader:ident, $method:ident, $a:ty) => {{
        match $reader.get::<$a>() {
            ::core::result::Result::Err(_) => $crate::abi_status::BAD_ARGS,
            ::core::result::Result::Ok(a) => {
                if !$reader.is_empty() {
                    $crate::abi_status::BAD_ARGS
                } else {
                    $method(a) as i32
                }
            }
        }
    }};
    (@call $reader:ident, $method:ident, $a:ty, $b:ty) => {{
        match ($reader.get::<$a>(), $reader.get::<$b>()) {
            (::core::result::Result::Ok(a), ::core::result::Result::Ok(b)) => {
                if !$reader.is_empty() {
                    $crate::abi_status::BAD_ARGS
                } else {
                    $method(a, b) as i32
                }
            }
            _ => $crate::abi_status::BAD_ARGS,
        }
    }};
    (@call $reader:ident, $method:ident, $a:ty, $b:ty, $c:ty) => {{
        let first = $reader.get::<$a>();
        let second = $reader.get::<$b>();
        let third = $reader.get::<$c>();
        match (first, second, third) {
            (
                ::core::result::Result::Ok(a),
                ::core::result::Result::Ok(b),
                ::core::result::Result::Ok(c),
            ) => {
                if !$reader.is_empty() {
                    $crate::abi_status::BAD_ARGS
                } else {
                    $method(a, b, c) as i32
                }
            }
            _ => $crate::abi_status::BAD_ARGS,
        }
    }};
}

/// Status codes the **SDK** returns from a dispatched call.
///
/// Negative, like the node's codes, so a caller that only checks the sign still
/// sees the failure. They are produced by contract code, never by the host, and
/// therefore live here rather than in [`abi::Error`].
pub mod abi_status {
    /// `-20`: no known selector — calldata is shorter than 4 bytes, or the
    /// selector is not in the `dispatch!` table.
    pub const BAD_SELECTOR: i32 = -20;
    /// `-21`: the arguments did not decode exactly.
    pub const BAD_ARGS: i32 = -21;
    /// `-1`: the host refused a calldata read (a pointer bug in the SDK itself).
    pub const OUT_OF_MEMORY: i32 = -1;
}
