//! X2 — the contract ABI, version 1, and its gas table.
//!
//! # Why an ABI is a consensus object
//!
//! A contract can only observe the outside world through host functions, so the
//! host function list **is** the observable surface of the chain. X1 proved the
//! sandbox can run a module with zero imports; X2 opens exactly seven imports,
//! all in module [`ABI_MODULE`], each with a fixed signature and a fixed price.
//! Anything else stays banned by the X0 census (fail-closed), which is why
//! `wasi_snapshot_preview1::clock_time_get` is still rejected.
//!
//! # Determinism rules
//!
//! * every function reads and writes a fixed number of bytes at fixed
//!   addresses; arguments are `i32` and results are `i32` status codes;
//! * no float, no clock, no randomness: `block_ctx` exposes the block height,
//!   the block timestamp **as decided by consensus**, and the ABI version;
//! * gas is charged **before** the effect, from a single budget shared with the
//!   instruction metering (1 fuel = 1 gas), so a call cannot do work and then
//!   discover it could not pay for it;
//! * events are appended in call order; storage goes through
//!   [`aura_state::assert_contract_may_write`], so a contract cannot write
//!   another account's namespace.
//!
//! # Memory protocol
//!
//! Pointers and lengths are `i32` byte offsets into the contract's own linear
//! memory. Every read and write is bounds-checked before it happens; a
//! misaligned or out-of-range access returns
//! [`AbiError::OutOfMemory`] and touches nothing.
//!
//! # Status codes
//!
//! `0` success, negative = failure. See [`AbiError::code`]. A negative code is
//! data returned to the contract, not a trap: the contract decides what to do
//! with it. Only running out of gas stops execution.

use aura_crypto::blake3_hash;
use aura_state::{Account, StateKey, assert_contract_may_write};
use aura_types::{Amount, Hash};

use crate::contract::HostState;

/// Module name every ABI function lives in.
pub const ABI_MODULE: &str = "env";

/// The ABI version this crate implements; also exposed to contracts through
/// `block_ctx(field = 2)`.
pub const ABI_VERSION: u32 = 1;

/// Maximum storage key length accepted by the ABI (bytes, zero-padded to 32).
pub const MAX_KEY_LEN: usize = 32;

/// Maximum value length a contract may write in one call.
pub const MAX_VALUE_LEN: usize = 4096;

/// Maximum topic length of one event.
pub const MAX_TOPIC_LEN: usize = 64;

/// Address length (C1/C7 canonical).
pub const ADDRESS_LEN: usize = 20;

/// `u128` little-endian bytes, the ABI's amount encoding.
pub const AMOUNT_LEN: usize = 16;

/// One row of the published gas table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GasRow {
    /// ABI function name.
    pub name: &'static str,
    /// Flat cost of one call.
    pub base: u64,
    /// Cost per payload byte.
    pub per_byte: u64,
    /// Why this price.
    pub note: &'static str,
}

/// The published gas table (protocol parameter, version [`ABI_VERSION`]).
///
/// The numbers are flat and byte-linear on purpose: a price that depends on
/// storage layout would make gas a function of state, and gas must be a
/// function of the call alone. A future phase may reprice; it must then bump
/// [`ABI_VERSION`] and ship a migration test.
pub const GAS_TABLE: [GasRow; 7] = [
    GasRow {
        name: "storage_read",
        base: 100,
        per_byte: 10,
        note: "one overlay/proof lookup plus the bytes returned",
    },
    GasRow {
        name: "storage_write",
        base: 200,
        per_byte: 25,
        note: "the expensive operation: it changes the state root",
    },
    GasRow {
        name: "balance",
        base: 50,
        per_byte: 0,
        note: "one account lookup, fixed size result",
    },
    GasRow {
        name: "transfer",
        base: 300,
        per_byte: 0,
        note: "two account updates, charged even when it fails",
    },
    GasRow {
        name: "emit",
        base: 200,
        per_byte: 15,
        note: "an event is hashed into the receipt, so it is priced per byte",
    },
    GasRow {
        name: "hash",
        base: 100,
        per_byte: 5,
        note: "BLAKE3 over the payload",
    },
    GasRow {
        name: "block_ctx",
        base: 20,
        per_byte: 0,
        note: "the cheapest call: it reads consensus context, not state",
    },
];

/// The ABI function set. Order is the canonical order used by the cargo
/// fixtures and by the census whitelist.
pub const ABI_FUNCTIONS: [&str; 7] = [
    "storage_read",
    "storage_write",
    "balance",
    "transfer",
    "emit",
    "hash",
    "block_ctx",
];

/// Signature of every ABI function: parameter count and whether it is exported
/// to the census (all are `(i32, …) -> i32`).
pub const fn arity(name: &str) -> Option<usize> {
    match name.as_bytes() {
        b"storage_read" => Some(4),
        b"storage_write" => Some(4),
        b"balance" => Some(3),
        b"transfer" => Some(3),
        b"emit" => Some(4),
        b"hash" => Some(3),
        b"block_ctx" => Some(2),
        _ => None,
    }
}

/// True when `module.name` is an ABI v1 import. The census uses this to decide
/// whether an import is allowed; everything else stays banned.
pub fn is_abi_import(module: &str, name: &str) -> bool {
    if module == crate::abi_v2::ABI2_MODULE {
        return crate::abi_v2::is_abi2_import(module, name);
    }
    if module == crate::abi_v3::ABI3_MODULE {
        return crate::abi_v3::is_abi3_import(module, name);
    }
    module == ABI_MODULE && arity(name).is_some()
}

impl GasRow {
    /// Cost of one call carrying `bytes` payload bytes.
    pub fn total(&self, bytes: u64) -> u64 {
        self.base
            .saturating_add(self.per_byte.saturating_mul(bytes))
    }
}

/// Gas for one call of `name` carrying `bytes` payload bytes.
pub fn gas_for(name: &str, bytes: u64) -> Option<u64> {
    GAS_TABLE
        .iter()
        .find(|row| row.name == name)
        .map(|row| row.total(bytes))
}

/// ABI failure codes. Negative values; `0` is success and is not listed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiError {
    /// A pointer/length pair pointed outside linear memory.
    OutOfMemory,
    /// Key longer than [`MAX_KEY_LEN`].
    KeyTooLong,
    /// Value longer than [`MAX_VALUE_LEN`].
    ValueTooLong,
    /// Topic longer than [`MAX_TOPIC_LEN`].
    TopicTooLong,
    /// The contract may not write this key ([`assert_contract_may_write`]).
    NotOwned,
    /// Not enough balance for the transfer.
    InsufficientBalance,
    /// Account arithmetic overflowed.
    Overflow,
    /// The budget is exhausted; the call must stop.
    OutOfGas,
    /// The receipt already holds the maximum number of events.
    TooManyEvents,
    /// X6: the target address is already on the call stack.
    Reentrancy,
    /// X6: the call stack has reached its published depth.
    CallDepth,
    /// X6: the address being called holds no code.
    NoCode,
    /// X6: the callee reverted; its writes and its value were discarded.
    CalleeFailed,
}

impl AbiError {
    /// Status code returned to the contract (and to the caller's receipt).
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
        }
    }

    /// Map a status code back to an `AbiError`; `None` for `0` (success) and
    /// for the two non-error statuses (`-9` absent key, `-10` unknown field).
    ///
    /// A receipt carries the raw `i32`, so this is the way back for anything
    /// that wants to report *why* a call returned what it did.
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            -1 => Some(Self::OutOfMemory),
            -2 => Some(Self::KeyTooLong),
            -3 => Some(Self::ValueTooLong),
            -4 => Some(Self::TopicTooLong),
            -5 => Some(Self::NotOwned),
            -6 => Some(Self::InsufficientBalance),
            -7 => Some(Self::Overflow),
            -8 => Some(Self::OutOfGas),
            -11 => Some(Self::TooManyEvents),
            -14 => Some(Self::Reentrancy),
            -15 => Some(Self::CallDepth),
            -16 => Some(Self::NoCode),
            -17 => Some(Self::CalleeFailed),
            _ => None,
        }
    }

    /// Stable name for logs and tests.
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
        }
    }
}

/// Largest single buffer the ABI will materialise from guest memory.
///
/// A contract can ask for any range it likes; without a cap a 4 GiB `len` would
/// be an allocation request, not a read. 64 KiB is 16x the largest value the
/// ABI accepts (`MAX_VALUE_LEN`) and keeps the host allocation bounded.
pub const MAX_ABI_BUFFER: u32 = 65_536;

/// Linear memory as the ABI sees it: bounds-checked byte access only.
///
/// Backends implement this over their own memory type, so every ABI rule lives
/// in this module exactly once.
pub trait LinearMemory {
    /// Read `len` bytes at `ptr`; `None` when the range is out of bounds.
    fn read(&self, ptr: u32, len: u32) -> Option<Vec<u8>>;
    /// Write `data` at `ptr`; `false` when the range is out of bounds.
    fn write(&mut self, ptr: u32, data: &[u8]) -> bool;
}

/// Which ABI function is being dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiFn {
    /// `storage_read`
    StorageRead,
    /// `storage_write`
    StorageWrite,
    /// `balance`
    Balance,
    /// `transfer`
    Transfer,
    /// `emit`
    Emit,
    /// `hash`
    Hash,
    /// `block_ctx`
    BlockCtx,
}

impl AbiFn {
    /// Map an import name to the dispatcher target.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "storage_read" => Some(Self::StorageRead),
            "storage_write" => Some(Self::StorageWrite),
            "balance" => Some(Self::Balance),
            "transfer" => Some(Self::Transfer),
            "emit" => Some(Self::Emit),
            "hash" => Some(Self::Hash),
            "block_ctx" => Some(Self::BlockCtx),
            _ => None,
        }
    }

    /// Published name (the gas table key).
    pub const fn name(self) -> &'static str {
        match self {
            Self::StorageRead => "storage_read",
            Self::StorageWrite => "storage_write",
            Self::Balance => "balance",
            Self::Transfer => "transfer",
            Self::Emit => "emit",
            Self::Hash => "hash",
            Self::BlockCtx => "block_ctx",
        }
    }
}

fn u32_of(value: i32) -> Result<u32, AbiError> {
    u32::try_from(value).map_err(|_| AbiError::OutOfMemory)
}

fn amount_from(bytes: &[u8]) -> Amount {
    let mut raw = [0u8; AMOUNT_LEN];
    raw.copy_from_slice(&bytes[..AMOUNT_LEN]);
    Amount::from_units(u128::from_le_bytes(raw))
}

/// Zero-pad an arbitrary storage key to the 32-byte slot the state model uses.
///
/// # Key identity
///
/// A key **is** its zero-padded slot, so trailing NUL bytes are not significant:
/// `b"a"` and `b"a\0"` address the same slot. This is deliberate — padding keeps
/// slots readable in proofs and keeps their order the byte order of the key — and
/// it is published so nobody relies on the opposite. The consequence is bounded:
/// a collision is inside one contract's own namespace, so a contract can only
/// shadow its own keys. A contract that needs both `a` and `a\0` must encode a
/// length prefix itself.
pub fn slot_of(key: &[u8]) -> Result<[u8; 32], AbiError> {
    if key.len() > MAX_KEY_LEN {
        return Err(AbiError::KeyTooLong);
    }
    let mut slot = [0u8; 32];
    slot[..key.len()].copy_from_slice(key);
    Ok(slot)
}

/// Dispatch one ABI call. Returns the status code the contract sees; any
/// out-of-gas condition is recorded on `host` and turned into a trap by the
/// engine adapter.
pub fn dispatch(
    host: &mut HostState,
    mem: &mut dyn LinearMemory,
    func: AbiFn,
    args: &[i32],
) -> i32 {
    let result = call(host, mem, func, args);
    match result {
        Ok(code) => code,
        Err(AbiError::OutOfGas) => {
            host.out_of_gas = true;
            AbiError::OutOfGas.code()
        }
        Err(error) => error.code(),
    }
}

fn charge(host: &mut HostState, func: AbiFn, bytes: u64) -> Result<(), AbiError> {
    let gas = gas_for(func.name(), bytes).ok_or(AbiError::Overflow)?;
    host.charge(gas)?;
    Ok(())
}

fn call(
    host: &mut HostState,
    mem: &mut dyn LinearMemory,
    func: AbiFn,
    args: &[i32],
) -> Result<i32, AbiError> {
    match func {
        AbiFn::StorageRead => {
            let [key_ptr, key_len, out_ptr, out_len] = expect(args, 4)?;
            let key = mem
                .read(u32_of(key_ptr)?, u32_of(key_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            let slot = slot_of(&key)?;
            let out_len = u32_of(out_len)?;
            // Charge for the bytes actually returned, not for the buffer size:
            // a contract cannot be billed for a read it did not get.
            let key_state = StateKey::storage(&host.call.contract, &slot);
            let value = host.view.get(&key_state).map(<[u8]>::to_vec);
            let returned = value
                .as_ref()
                .map_or(0, |bytes| bytes.len().min(out_len as usize));
            charge(host, func, returned as u64)?;
            let Some(bytes) = value else {
                return Ok(-9); // absent: a status, not an error
            };
            if !mem.write(u32_of(out_ptr)?, &bytes[..returned]) {
                return Err(AbiError::OutOfMemory);
            }
            Ok(returned as i32)
        }
        AbiFn::StorageWrite => {
            let [key_ptr, key_len, val_ptr, val_len] = expect(args, 4)?;
            let key = mem
                .read(u32_of(key_ptr)?, u32_of(key_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            let value = mem
                .read(u32_of(val_ptr)?, u32_of(val_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            let slot = slot_of(&key)?;
            if value.len() > MAX_VALUE_LEN {
                return Err(AbiError::ValueTooLong);
            }
            let key_state = StateKey::storage(&host.call.contract, &slot);
            // The contract may write only its own namespace (C7 rule, re-checked
            // at the ABI because a bug here is a fund-loss bug).
            assert_contract_may_write(&host.call.contract, &key_state)
                .map_err(|_| AbiError::NotOwned)?;
            charge(host, func, (key.len() + value.len()) as u64)?;
            if value.is_empty() {
                host.view.delete(&key_state);
            } else {
                host.view.set(&key_state, value);
            }
            host.writes += 1;
            Ok(0)
        }
        AbiFn::Balance => {
            let [addr_ptr, addr_len, out_ptr] = expect(args, 3)?;
            let raw = mem
                .read(u32_of(addr_ptr)?, u32_of(addr_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            if raw.len() != ADDRESS_LEN {
                return Err(AbiError::OutOfMemory);
            }
            let address =
                aura_types::Address::from_slice(&raw).map_err(|_| AbiError::OutOfMemory)?;
            charge(host, func, 0)?;
            let balance = host.balance_of(&address);
            if !mem.write(u32_of(out_ptr)?, &balance.units().to_le_bytes()) {
                return Err(AbiError::OutOfMemory);
            }
            Ok(0)
        }
        AbiFn::Transfer => {
            let [addr_ptr, addr_len, amount_ptr] = expect(args, 3)?;
            let raw = mem
                .read(u32_of(addr_ptr)?, u32_of(addr_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            if raw.len() != ADDRESS_LEN {
                return Err(AbiError::OutOfMemory);
            }
            let target =
                aura_types::Address::from_slice(&raw).map_err(|_| AbiError::OutOfMemory)?;
            let amount_raw = mem
                .read(u32_of(amount_ptr)?, AMOUNT_LEN as u32)
                .ok_or(AbiError::OutOfMemory)?;
            let amount = amount_from(&amount_raw);
            // Charged before the effect, and charged even when it fails: a
            // failed transfer still did the work of reading two accounts.
            charge(host, func, 0)?;
            host.transfer_to(&target, amount)?;
            Ok(0)
        }
        AbiFn::Emit => {
            let [topic_ptr, topic_len, data_ptr, data_len] = expect(args, 4)?;
            let topic = mem
                .read(u32_of(topic_ptr)?, u32_of(topic_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            let data = mem
                .read(u32_of(data_ptr)?, u32_of(data_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            if topic.len() > MAX_TOPIC_LEN {
                return Err(AbiError::TopicTooLong);
            }
            charge(host, func, (topic.len() + data.len()) as u64)?;
            host.push_event(&topic, &data)?;
            Ok(0)
        }
        AbiFn::Hash => {
            let [data_ptr, data_len, out_ptr] = expect(args, 3)?;
            let data = mem
                .read(u32_of(data_ptr)?, u32_of(data_len)?)
                .ok_or(AbiError::OutOfMemory)?;
            charge(host, func, data.len() as u64)?;
            let digest: Hash = blake3_hash(&data);
            if !mem.write(u32_of(out_ptr)?, digest.as_bytes()) {
                return Err(AbiError::OutOfMemory);
            }
            Ok(0)
        }
        AbiFn::BlockCtx => {
            let [field, out_ptr] = expect(args, 2)?;
            charge(host, func, 0)?;
            let value = match field {
                0 => host.call.block.height,
                1 => host.call.block.timestamp_ms,
                2 => u64::from(ABI_VERSION),
                _ => return Ok(-10), // unknown field: a status, not an error
            };
            if !mem.write(u32_of(out_ptr)?, &value.to_le_bytes()) {
                return Err(AbiError::OutOfMemory);
            }
            Ok(0)
        }
    }
}

fn expect<const N: usize>(args: &[i32], arity: usize) -> Result<[i32; N], AbiError> {
    debug_assert_eq!(N, arity);
    if args.len() != N {
        return Err(AbiError::OutOfMemory);
    }
    let mut out = [0i32; N];
    out.copy_from_slice(args);
    Ok(out)
}

/// Account value under a canonical address, for tests and for the executor.
pub fn account_of(host: &HostState, address: &aura_types::Address) -> Account {
    match host.view.get(&StateKey::account(address)) {
        Some(bytes) => {
            Account::from_canonical_bytes(bytes).unwrap_or_else(|_| Account::new_empty())
        }
        None => Account::new_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_row_has_an_arity_and_a_dispatcher_arm() {
        // A gas row without a function, or a function without a price, is a
        // billable call nobody can account for.
        for row in GAS_TABLE {
            assert_eq!(arity(row.name), Some(arity_of(row.name)), "{}", row.name);
            assert!(
                AbiFn::from_name(row.name).is_some(),
                "{} has no dispatcher arm",
                row.name
            );
            assert!(ABI_FUNCTIONS.contains(&row.name));
        }
        assert_eq!(ABI_FUNCTIONS.len(), GAS_TABLE.len());
    }

    fn arity_of(name: &str) -> usize {
        match AbiFn::from_name(name).expect("known") {
            AbiFn::StorageRead | AbiFn::StorageWrite | AbiFn::Emit => 4,
            AbiFn::Balance | AbiFn::Transfer | AbiFn::Hash => 3,
            AbiFn::BlockCtx => 2,
        }
    }

    #[test]
    fn missing_names_are_not_dispatchable() {
        assert!(AbiFn::from_name("storage_delete").is_none());
        assert!(AbiFn::from_name("").is_none());
        assert!(!is_abi_import("wasi_snapshot_preview1", "storage_read"));
    }

    #[test]
    fn slots_are_zero_padded_and_bounded() {
        assert_eq!(slot_of(b"counter").unwrap()[..7], *b"counter");
        assert_eq!(slot_of(b"counter").unwrap()[7..], [0u8; 25]);
        assert_eq!(slot_of(&[7u8; 32]).unwrap(), [7u8; 32]);
        assert_eq!(slot_of(&[7u8; 33]), Err(AbiError::KeyTooLong));
        // Documented property, asserted so that changing it is a decision:
        // padding makes trailing NULs insignificant.
        assert_eq!(slot_of(b"a").unwrap(), slot_of(b"a\0").unwrap());
    }

    #[test]
    fn gas_is_flat_plus_linear_and_saturating() {
        for row in GAS_TABLE {
            assert_eq!(row.total(0), row.base);
            assert_eq!(row.total(1), row.base + row.per_byte);
            assert_eq!(row.total(1_000), row.base + 1_000 * row.per_byte);
            if row.per_byte > 0 {
                assert_eq!(row.total(u64::MAX), u64::MAX, "{} saturates", row.name);
            }
        }
        assert_eq!(gas_for("nope", 0), None);
        assert_eq!(gas_for("storage_read", 10), Some(200));
    }

    #[test]
    fn amounts_round_trip_through_the_abi_encoding() {
        for units in [0u128, 1, 1_000, u64::MAX as u128, u128::MAX] {
            let encoded = units.to_le_bytes();
            assert_eq!(amount_from(&encoded).units(), units);
        }
    }

    #[test]
    fn status_codes_are_negative_and_distinct() {
        let all = [
            AbiError::OutOfMemory,
            AbiError::KeyTooLong,
            AbiError::ValueTooLong,
            AbiError::TopicTooLong,
            AbiError::NotOwned,
            AbiError::InsufficientBalance,
            AbiError::Overflow,
            AbiError::OutOfGas,
            AbiError::TooManyEvents,
        ];
        let mut codes: Vec<i32> = all.iter().map(|error| error.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), all.len(), "two status codes collide");
        assert!(codes.iter().all(|code| *code < 0), "0 is success");
    }
}
