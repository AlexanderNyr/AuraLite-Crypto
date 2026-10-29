//! ABI v1 as a contract sees it: seven functions, typed.
//!
//! The host side of this ABI lives in `aura_exec::abi`; the *contract* side is
//! here. Both list the same seven names because both are checked against the
//! node's published table by `tests/x4_sdk.rs` — a divergence between the SDK
//! and the node is a build-time test failure, not a deployed bug.
//!
//! # The contract
//!
//! Every function takes `(ptr, len)` pairs that must lie inside the contract's
//! linear memory and returns a status:
//!
//! * `>= 0` — success; for `storage_read` this is the number of bytes returned,
//! * `-1` … `-11` — the closed error set in [`Error`],
//! * `-9` — `storage_read` only: the key is **absent** (a status, not an error),
//! * `-10` — `block_ctx` only: unknown field (a status, not an error).
//!
//! Nothing panics, nothing allocates: a bad pointer becomes [`Error::OutOfMemory`]
//! instead of a corrupt read, because the node bounds-checks every access.

/// Module name every ABI import must use (the census enforces it).
pub const ABI_MODULE: &str = "env";

/// ABI version this SDK speaks.
pub const ABI_VERSION: u32 = 1;

/// Longest storage key, in bytes.
pub const MAX_KEY_LEN: usize = 32;

/// Longest storage value the ABI accepts, in bytes.
pub const MAX_VALUE_LEN: usize = 4096;

/// Longest event topic, in bytes.
pub const MAX_TOPIC_LEN: usize = 64;

/// Length of an address, in bytes.
pub const ADDRESS_LEN: usize = 20;

/// Length of an [`Amount`] in bytes (a `u128` of base units).
pub const AMOUNT_LEN: usize = 16;

/// The seven ABI functions, in the order the node publishes them.
pub const FUNCTIONS: [&str; 7] = [
    "storage_read",
    "storage_write",
    "balance",
    "transfer",
    "emit",
    "hash",
    "block_ctx",
];

/// Arity of each ABI function, in the node's order.
pub const ARITIES: [usize; 7] = [4, 4, 3, 3, 4, 3, 2];

/// A non-zero ABI status.
///
/// `Absent` (-9) and `UnknownField` (-10) are *statuses*, not failures: the SDK
/// turns them into `Ok(None)`/an error only where that is the honest reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `-1`: a pointer/length pair pointed outside linear memory.
    OutOfMemory,
    /// `-2`: the key is longer than [`MAX_KEY_LEN`].
    KeyTooLong,
    /// `-3`: the value is longer than [`MAX_VALUE_LEN`].
    ValueTooLong,
    /// `-4`: the topic is longer than [`MAX_TOPIC_LEN`].
    TopicTooLong,
    /// `-5`: the contract tried to write outside its own namespace.
    NotOwned,
    /// `-6`: not enough balance.
    InsufficientBalance,
    /// `-7`: account arithmetic overflowed.
    Overflow,
    /// `-8`: the gas budget is exhausted.
    OutOfGas,
    /// `-11`: the receipt already holds the maximum number of events.
    TooManyEvents,
    /// `-14`: X6 — the target is already on the call stack. Refused, not queued.
    Reentrancy,
    /// `-15`: X6 — the call stack reached its published depth.
    CallDepth,
    /// `-16`: X6 — the address called holds no code.
    NoCode,
    /// `-17`: X6 — the callee reverted; its writes and its value were discarded.
    CalleeFailed,
    /// A status the SDK does not know — the node grew a code and the contract
    /// was built against an older SDK. Carried verbatim rather than flattened.
    Unknown(i32),
}

impl Error {
    /// Map a raw status code.
    pub const fn from_code(code: i32) -> Self {
        match code {
            -1 => Self::OutOfMemory,
            -2 => Self::KeyTooLong,
            -3 => Self::ValueTooLong,
            -4 => Self::TopicTooLong,
            -5 => Self::NotOwned,
            -6 => Self::InsufficientBalance,
            -7 => Self::Overflow,
            -8 => Self::OutOfGas,
            -11 => Self::TooManyEvents,
            -14 => Self::Reentrancy,
            -15 => Self::CallDepth,
            -16 => Self::NoCode,
            -17 => Self::CalleeFailed,
            other => Self::Unknown(other),
        }
    }

    /// The raw status code.
    pub const fn code(self) -> i32 {
        match self {
            Self::OutOfMemory => -1,
            Self::KeyTooLong => -2,
            Self::ValueTooLong => -3,
            Self::TopicTooLong => -4,
            Self::NotOwned => -5,
            Self::InsufficientBalance => -6,
            Self::Overflow => -7,
            Self::OutOfGas => -8,
            Self::TooManyEvents => -11,
            Self::Reentrancy => -14,
            Self::CallDepth => -15,
            Self::NoCode => -16,
            Self::CalleeFailed => -17,
            Self::Unknown(code) => code,
        }
    }

    /// Stable name, for logs and for the test vectors.
    pub const fn name(self) -> &'static str {
        match self {
            Self::OutOfMemory => "out_of_memory",
            Self::KeyTooLong => "key_too_long",
            Self::ValueTooLong => "value_too_long",
            Self::TopicTooLong => "topic_too_long",
            Self::NotOwned => "not_owned",
            Self::InsufficientBalance => "insufficient_balance",
            Self::Overflow => "overflow",
            Self::OutOfGas => "out_of_gas",
            Self::TooManyEvents => "too_many_events",
            Self::Reentrancy => "reentrancy",
            Self::CallDepth => "call_depth",
            Self::NoCode => "no_code",
            Self::CalleeFailed => "callee_failed",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// `-9`: the key is absent. Not an error — the SDK returns `Ok(None)` instead.
const STATUS_ABSENT: i32 = -9;
/// `-10`: unknown `block_ctx` field.
const STATUS_UNKNOWN_FIELD: i32 = -10;

// The raw imports live in one private module so a wrapper can have the same
// human name as the node's function without colliding with the symbol.
mod raw {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        pub fn storage_read(key: *const u8, key_len: usize, out: *mut u8, out_len: usize) -> i32;
        pub fn storage_write(
            key: *const u8,
            key_len: usize,
            value: *const u8,
            value_len: usize,
        ) -> i32;
        pub fn balance(address: *const u8, address_len: usize, out: *mut u8) -> i32;
        pub fn transfer(address: *const u8, address_len: usize, amount: *const u8) -> i32;
        pub fn emit(topic: *const u8, topic_len: usize, data: *const u8, data_len: usize) -> i32;
        pub fn hash(data: *const u8, data_len: usize, out: *mut u8) -> i32;
        pub fn block_ctx(field: i32, out: *mut u64) -> i32;
    }

    // The host build resolves the same seven symbols to `super::mock`.
    #[cfg(not(target_arch = "wasm32"))]
    unsafe extern "C" {
        pub fn storage_read(key: *const u8, key_len: usize, out: *mut u8, out_len: usize) -> i32;
        pub fn storage_write(
            key: *const u8,
            key_len: usize,
            value: *const u8,
            value_len: usize,
        ) -> i32;
        pub fn balance(address: *const u8, address_len: usize, out: *mut u8) -> i32;
        pub fn transfer(address: *const u8, address_len: usize, amount: *const u8) -> i32;
        pub fn emit(topic: *const u8, topic_len: usize, data: *const u8, data_len: usize) -> i32;
        pub fn hash(data: *const u8, data_len: usize, out: *mut u8) -> i32;
        pub fn block_ctx(field: i32, out: *mut u64) -> i32;
    }
}

fn status(code: i32) -> Result<i32, Error> {
    if code >= 0 {
        Ok(code)
    } else {
        Err(Error::from_code(code))
    }
}

/// Read `key` into `out`. `Ok(None)` means the key is absent; `Ok(Some(n))` is
/// the number of bytes written.
///
/// # Safety
///
/// `out` is a slice the caller owns; the node bounds-checks both it and `key`
/// against linear memory, so a buggy pointer becomes `Err(OutOfMemory)`.
pub fn storage_read(key: &[u8], out: &mut [u8]) -> Result<Option<usize>, Error> {
    // SAFETY: both ranges come from live slices; the host validates them and
    // returns -1 rather than writing out of bounds.
    let code = unsafe { raw::storage_read(key.as_ptr(), key.len(), out.as_mut_ptr(), out.len()) };
    match code {
        STATUS_ABSENT => Ok(None),
        other => status(other).map(|written| Some(written as usize)),
    }
}

/// Write `value` under `key`. An empty `value` deletes the slot (the ABI rule).
pub fn storage_write(key: &[u8], value: &[u8]) -> Result<(), Error> {
    // SAFETY: as above — pointers come from slices.
    let code = unsafe { raw::storage_write(key.as_ptr(), key.len(), value.as_ptr(), value.len()) };
    status(code).map(|_| ())
}

/// Balance of `address`, in base units.
pub fn balance(address: &[u8; ADDRESS_LEN]) -> Result<u128, Error> {
    let mut out = [0u8; AMOUNT_LEN];
    // SAFETY: the host writes exactly AMOUNT_LEN bytes into `out`.
    let code = unsafe { raw::balance(address.as_ptr(), ADDRESS_LEN, out.as_mut_ptr()) };
    status(code).map(|_| u128::from_le_bytes(out))
}

/// Move `amount` base units from the calling contract to `address`.
pub fn transfer(address: &[u8; ADDRESS_LEN], amount: u128) -> Result<(), Error> {
    let encoded = amount.to_le_bytes();
    // SAFETY: fixed-size buffers, host-checked.
    let code = unsafe { raw::transfer(address.as_ptr(), ADDRESS_LEN, encoded.as_ptr()) };
    status(code).map(|_| ())
}

/// Append an event.
pub fn emit(topic: &[u8], data: &[u8]) -> Result<(), Error> {
    // SAFETY: pointers come from live slices; the host bounds-checks.
    let code = unsafe { raw::emit(topic.as_ptr(), topic.len(), data.as_ptr(), data.len()) };
    status(code).map(|_| ())
}

/// BLAKE3 of `data`, computed by the node (contracts have no hash implementation
/// of their own — X0 bans the instruction mix that would make one portable).
pub fn hash(data: &[u8]) -> Result<[u8; 32], Error> {
    let mut out = [0u8; 32];
    // SAFETY: the host writes exactly 32 bytes into `out`.
    let code = unsafe { raw::hash(data.as_ptr(), data.len(), out.as_mut_ptr()) };
    status(code).map(|_| out)
}

/// `block_ctx` field ids, published so a contract never passes a bare number.
pub mod field {
    /// Block height.
    pub const HEIGHT: i32 = 0;
    /// Block timestamp, milliseconds since the Unix epoch.
    pub const TIMESTAMP_MS: i32 = 1;
    /// ABI version the node speaks.
    pub const ABI_VERSION: i32 = 2;
}

fn context(field: i32) -> Result<u64, Error> {
    let mut out = 0u64;
    // SAFETY: the host writes exactly 8 bytes into `out`.
    let code = unsafe { raw::block_ctx(field, &mut out) };
    match code {
        STATUS_UNKNOWN_FIELD => Err(Error::Unknown(STATUS_UNKNOWN_FIELD)),
        other => status(other).map(|_| out),
    }
}

/// Height of the block this call runs in.
///
/// On the host build the mock returns the height the test set; on wasm it is
/// consensus data, identical on every node.
pub fn block_height() -> Result<u64, Error> {
    context(field::HEIGHT)
}

/// Block timestamp in milliseconds since the Unix epoch.
pub fn block_timestamp_ms() -> Result<u64, Error> {
    context(field::TIMESTAMP_MS)
}

/// ABI version the node speaks — a contract can assert it was built for it.
pub fn abi_version() -> Result<u64, Error> {
    context(field::ABI_VERSION)
}

/// A backend that behaves like the node, for host tests.
///
/// It is not a simulation layer: the mock *is* reachable through the same
/// `extern "C"` symbols, with the same status codes and the same limits, so the
/// wrappers above are exercised exactly as they are on chain. What it does not
/// have is gas — metering is the engine's job (X2), not the SDK's.
#[cfg(not(target_arch = "wasm32"))]
pub mod mock {
    use super::{
        ADDRESS_LEN, AMOUNT_LEN, Error, MAX_KEY_LEN, MAX_TOPIC_LEN, MAX_VALUE_LEN, STATUS_ABSENT,
        STATUS_UNKNOWN_FIELD,
    };
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct State {
        storage: BTreeMap<Vec<u8>, Vec<u8>>,
        balances: BTreeMap<[u8; ADDRESS_LEN], u128>,
        events: Vec<(Vec<u8>, Vec<u8>)>,
        height: u64,
        timestamp_ms: u64,
        /// The address whose balance `transfer` spends from — the contract.
        this: [u8; ADDRESS_LEN],
        /// Number of ABI calls made, so a test can assert "no host call happened".
        calls: usize,
        /// ABI v2: the bytes the "caller" sent.
        calldata: Vec<u8>,
        /// ABI v2: the bytes the contract answered with.
        ret: Vec<u8>,
        /// ABI v2: the address that signed the "transaction".
        caller: [u8; ADDRESS_LEN],
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    /// Reset the mock and set the block context.
    pub fn reset(height: u64, timestamp_ms: u64) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            *state = State {
                height,
                timestamp_ms,
                ..State::default()
            };
        });
    }

    /// The contract address `transfer` spends from, and `balance` is measured on.
    pub fn set_this(address: [u8; ADDRESS_LEN]) {
        STATE.with(|state| state.borrow_mut().this = address);
    }

    /// Give `address` a balance.
    pub fn set_balance(address: [u8; ADDRESS_LEN], units: u128) {
        STATE.with(|state| {
            state.borrow_mut().balances.insert(address, units);
        });
    }

    /// Balance of `address`, as the mock holds it.
    pub fn balance_of(address: [u8; ADDRESS_LEN]) -> u128 {
        STATE.with(|state| state.borrow().balances.get(&address).copied().unwrap_or(0))
    }

    /// Seed the calldata of the next call (ABI v2).
    pub fn set_calldata(bytes: &[u8]) {
        STATE.with(|state| state.borrow_mut().calldata = bytes.to_vec());
    }

    /// The return data the contract produced (ABI v2).
    pub fn returned() -> Vec<u8> {
        STATE.with(|state| state.borrow().ret.clone())
    }

    /// Set the address `caller()` reports (ABI v2).
    pub fn set_caller(address: [u8; ADDRESS_LEN]) {
        STATE.with(|state| state.borrow_mut().caller = address);
    }

    /// Raw storage as the mock holds it.
    pub fn storage() -> BTreeMap<Vec<u8>, Vec<u8>> {
        STATE.with(|state| state.borrow().storage.clone())
    }

    /// Seed a storage slot without going through a contract.
    pub fn seed(key: &[u8], value: &[u8]) {
        STATE.with(|state| {
            state
                .borrow_mut()
                .storage
                .insert(key.to_vec(), value.to_vec());
        });
    }

    /// Events emitted so far, in order.
    pub fn events() -> Vec<(Vec<u8>, Vec<u8>)> {
        STATE.with(|state| state.borrow().events.clone())
    }

    /// How many ABI calls the mock has served.
    pub fn calls() -> usize {
        STATE.with(|state| state.borrow().calls)
    }

    fn count() {
        STATE.with(|state| state.borrow_mut().calls += 1);
    }

    /// Read `len` bytes through a raw pointer, the way the node reads linear
    /// memory. Returns `None` for a null/huge range.
    unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
        if len == 0 {
            return Some(&[]);
        }
        if ptr.is_null() || len > MAX_VALUE_LEN * 16 {
            return None;
        }
        // SAFETY: the caller (a wrapper) passes a live slice; the length bound
        // is the ABI's own buffer cap, mirrored here.
        Some(unsafe { std::slice::from_raw_parts(ptr, len) })
    }

    #[unsafe(no_mangle)]
    /// `env.storage_read`: read the value under a key into `out`.
    ///
    /// # Safety
    ///
    /// `key_ptr`/`out_ptr` must be valid for `key_len`/`out_len` bytes; the
    /// SDK wrapper ([`storage_read`]) is the only caller and passes slices.
    pub unsafe extern "C" fn storage_read(
        key_ptr: *const u8,
        key_len: usize,
        out_ptr: *mut u8,
        out_len: usize,
    ) -> i32 {
        count();
        let Some(key) = (unsafe { bytes(key_ptr, key_len) }) else {
            return Error::OutOfMemory.code();
        };
        if key.len() > MAX_KEY_LEN {
            return Error::KeyTooLong.code();
        }
        STATE.with(|state| {
            let state = state.borrow();
            let Some(value) = state.storage.get(key) else {
                return STATUS_ABSENT;
            };
            let written = value.len().min(out_len);
            // SAFETY: `out_len` bytes are writable by contract with the caller.
            unsafe { std::ptr::copy_nonoverlapping(value.as_ptr(), out_ptr, written) };
            written as i32
        })
    }

    #[unsafe(no_mangle)]
    /// `env.storage_write`: write `value` under `key`; an empty value deletes.
    ///
    /// # Safety
    ///
    /// Pointers must be valid for their lengths; the wrapper passes slices.
    pub unsafe extern "C" fn storage_write(
        key_ptr: *const u8,
        key_len: usize,
        value_ptr: *const u8,
        value_len: usize,
    ) -> i32 {
        count();
        let (Some(key), Some(value)) = (
            (unsafe { bytes(key_ptr, key_len) }),
            (unsafe { bytes(value_ptr, value_len) }),
        ) else {
            return Error::OutOfMemory.code();
        };
        if key.len() > MAX_KEY_LEN {
            return Error::KeyTooLong.code();
        }
        if value.len() > MAX_VALUE_LEN {
            return Error::ValueTooLong.code();
        }
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if value.is_empty() {
                state.storage.remove(key);
            } else {
                state.storage.insert(key.to_vec(), value.to_vec());
            }
        });
        0
    }

    #[unsafe(no_mangle)]
    /// `env.balance`: write an account balance into `out`.
    ///
    /// # Safety
    ///
    /// `out` must be valid for 16 bytes.
    pub unsafe extern "C" fn balance(address: *const u8, address_len: usize, out: *mut u8) -> i32 {
        count();
        let Some(raw) = (unsafe { bytes(address, address_len) }) else {
            return Error::OutOfMemory.code();
        };
        if raw.len() != ADDRESS_LEN {
            return Error::OutOfMemory.code();
        }
        let mut address = [0u8; ADDRESS_LEN];
        address.copy_from_slice(raw);
        let units = balance_of(address);
        // SAFETY: the wrapper passes a 16-byte buffer.
        unsafe { std::ptr::copy_nonoverlapping(units.to_le_bytes().as_ptr(), out, AMOUNT_LEN) };
        0
    }

    #[unsafe(no_mangle)]
    /// `env.transfer`: move value from the mock's contract address.
    ///
    /// # Safety
    ///
    /// `address` must be valid for `address_len` bytes and `amount` for 16.
    pub unsafe extern "C" fn transfer(
        address: *const u8,
        address_len: usize,
        amount: *const u8,
    ) -> i32 {
        count();
        let (Some(raw), Some(raw_amount)) = (
            (unsafe { bytes(address, address_len) }),
            (unsafe { bytes(amount, AMOUNT_LEN) }),
        ) else {
            return Error::OutOfMemory.code();
        };
        if raw.len() != ADDRESS_LEN {
            return Error::OutOfMemory.code();
        }
        let mut to = [0u8; ADDRESS_LEN];
        to.copy_from_slice(raw);
        let mut units = [0u8; AMOUNT_LEN];
        units.copy_from_slice(raw_amount);
        let units = u128::from_le_bytes(units);
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let this = state.this;
            let from_balance = state.balances.get(&this).copied().unwrap_or(0);
            if from_balance < units {
                return Error::InsufficientBalance.code();
            }
            state.balances.insert(this, from_balance - units);
            let to_balance = state.balances.get(&to).copied().unwrap_or(0);
            let Some(next) = to_balance.checked_add(units) else {
                return Error::Overflow.code();
            };
            state.balances.insert(to, next);
            0
        })
    }

    #[unsafe(no_mangle)]
    /// `env.emit`: append an event.
    ///
    /// # Safety
    ///
    /// Pointers must be valid for their lengths.
    pub unsafe extern "C" fn emit(
        topic: *const u8,
        topic_len: usize,
        data: *const u8,
        data_len: usize,
    ) -> i32 {
        count();
        let (Some(topic), Some(data)) = (
            (unsafe { bytes(topic, topic_len) }),
            (unsafe { bytes(data, data_len) }),
        ) else {
            return Error::OutOfMemory.code();
        };
        if topic.len() > MAX_TOPIC_LEN {
            return Error::TopicTooLong.code();
        }
        if data.len() > MAX_VALUE_LEN {
            return Error::ValueTooLong.code();
        }
        STATE.with(|state| {
            state
                .borrow_mut()
                .events
                .push((topic.to_vec(), data.to_vec()));
        });
        0
    }

    #[unsafe(no_mangle)]
    /// `env.hash`: write a stand-in digest into `out`.
    ///
    /// # Safety
    ///
    /// `out` must be valid for 32 bytes.
    pub unsafe extern "C" fn hash(data: *const u8, data_len: usize, out: *mut u8) -> i32 {
        count();
        let Some(data) = (unsafe { bytes(data, data_len) }) else {
            return Error::OutOfMemory.code();
        };
        // The node hashes with BLAKE3 (C3); the mock uses a documented stand-in
        // so the SDK tests do not need the crypto crate. Any test that asserts a
        // *digest* asserts it through the real engine, not through the mock.
        let digest = stand_in_digest(data);
        // SAFETY: the wrapper passes a 32-byte buffer.
        unsafe { std::ptr::copy_nonoverlapping(digest.as_ptr(), out, 32) };
        0
    }

    fn stand_in_digest(data: &[u8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        let mut acc: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in data {
            acc ^= u64::from(*byte);
            acc = acc.wrapping_mul(0x0000_0100_0000_01b3);
        }
        out[..8].copy_from_slice(&acc.to_le_bytes());
        out[8..16].copy_from_slice(&(data.len() as u64).to_le_bytes());
        out
    }

    #[unsafe(no_mangle)]
    /// `env.block_ctx`: write a block field into `out`.
    ///
    /// # Safety
    ///
    /// `out` must be valid and aligned for one `u64`.
    pub unsafe extern "C" fn block_ctx(field: i32, out: *mut u64) -> i32 {
        count();
        let value = STATE.with(|state| {
            let state = state.borrow();
            match field {
                0 => Some(state.height),
                1 => Some(state.timestamp_ms),
                2 => Some(u64::from(super::ABI_VERSION)),
                _ => None,
            }
        });
        match value {
            Some(value) => {
                // SAFETY: the wrapper passes a &mut u64.
                unsafe { *out = value };
                0
            }
            None => STATUS_UNKNOWN_FIELD,
        }
    }

    /// `env2.calldata_len`.
    #[unsafe(no_mangle)]
    pub extern "C" fn calldata_len() -> i32 {
        count();
        STATE.with(|state| state.borrow().calldata.len() as i32)
    }

    /// `env2.calldata_read`.
    ///
    /// # Safety
    ///
    /// `dst` must be valid for `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn calldata_read(dst: *mut u8, len: usize) -> i32 {
        count();
        let bytes = STATE.with(|state| {
            let state = state.borrow();
            let n = len.min(state.calldata.len());
            state.calldata[..n].to_vec()
        });
        // SAFETY: the wrapper passes a live slice of `len` bytes.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len()) };
        bytes.len() as i32
    }

    /// `env2.return_write`.
    ///
    /// # Safety
    ///
    /// `src` must be valid for `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn return_write(src: *const u8, len: usize) -> i32 {
        count();
        if len > super::super::abi2::MAX_RETURN_LEN {
            return -3;
        }
        // SAFETY: the wrapper passes a live slice of `len` bytes.
        let bytes = unsafe { std::slice::from_raw_parts(src, len) }.to_vec();
        STATE.with(|state| state.borrow_mut().ret = bytes);
        0
    }

    /// `env2.caller`.
    ///
    /// # Safety
    ///
    /// `out` must be valid for `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn caller(out: *mut u8, len: usize) -> i32 {
        count();
        if len < ADDRESS_LEN {
            return -13;
        }
        let address = STATE.with(|state| state.borrow().caller);
        // SAFETY: the wrapper passes a live slice of at least ADDRESS_LEN bytes.
        unsafe { std::ptr::copy_nonoverlapping(address.as_ptr(), out, ADDRESS_LEN) };
        ADDRESS_LEN as i32
    }

    /// `env2.self_address`.
    ///
    /// # Safety
    ///
    /// `out` must be valid for `len` bytes.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn self_address(out: *mut u8, len: usize) -> i32 {
        count();
        if len < ADDRESS_LEN {
            return -13;
        }
        let address = STATE.with(|state| state.borrow().this);
        // SAFETY: the wrapper passes a live slice of at least ADDRESS_LEN bytes.
        unsafe { std::ptr::copy_nonoverlapping(address.as_ptr(), out, ADDRESS_LEN) };
        ADDRESS_LEN as i32
    }

    /// `env2.abi_version`.
    #[unsafe(no_mangle)]
    pub extern "C" fn abi_version() -> i32 {
        count();
        super::super::abi2::ABI2_VERSION as i32
    }
}
