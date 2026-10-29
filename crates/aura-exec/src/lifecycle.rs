//! X3 — contract lifecycle: deploy, code hash, code version, upgrade, migration.
//!
//! Before X3 a contract was a byte string handed to the executor. Now it is a
//! **record in state**:
//!
//! ```text
//! StateKey::code(address)     -> CodeRecord { abi_version, code_version, code }
//! StateKey::account(address)  -> Account { nonce, balance, code_hash, .. }
//! ```
//!
//! `Account::code_hash` is `BLAKE3(code)` — the identity of the logic, not of the
//! record that stores it — so an account without code has `Hash::ZERO` and every
//! call can tell "not a contract" from "a contract" without a side table.
//!
//! # Decisions (X3)
//!
//! * **Deploy is atomic with instantiation.** `deploy` stores the code, funds the
//!   account and runs the optional `init` entry in ONE session: if `init` traps,
//!   the account does not exist afterwards. A separate two-phase instantiate is
//!   deferred to the factory pattern (X6), where it belongs.
//! * **Upgrade is optimistic and explicit.** `upgrade(expected_code_version)`
//!   refuses unless the caller names the version it is upgrading *from*, so two
//!   concurrent upgrades cannot silently race. The version is part of state, not
//!   of the transaction.
//! * **Upgrade never touches balance or storage.** It rewrites the code record
//!   and `Account::code_hash`; every storage slot and the balance survive, and a
//!   storage migration is *contract logic* (`migrate(from, to)`), not a chain
//!   operation. Rolling it back is the same rollback rule as X2.
//! * **Code costs gas to store** (`DEPLOY_BASE_GAS + CODE_BYTE_GAS * len`), which
//!   makes "unlimited code" economically impossible instead of a hard limit that
//!   a future phase would have to renegotiate: with the Stage 1 budget of
//!   1,000,000 gas, at most ~100 KiB of code fits in one transaction.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use aura_exec::lifecycle::{Lifecycle, LifecycleStatus};
//! use aura_exec::{BlockContext, ContractCall};
//! use aura_state::MerkleTree;
//! use aura_types::{Address, Amount};
//!
//! let code = std::fs::read("contract.wasm")?;
//! let call = ContractCall {
//!     contract: Address::from_slice(&[1u8; 20])?,
//!     caller: Address::from_slice(&[2u8; 20])?,
//!     block: BlockContext { height: 7, timestamp_ms: 1_700_000_000_000 },
//!     gas_limit: 1_000_000,
//! };
//! let lifecycle = Lifecycle::new();
//! let base = MerkleTree::new();
//! let receipt = lifecycle.deploy(&base, call, code, Amount::ZERO, None)?;
//! assert!(matches!(receipt.lifecycle, LifecycleStatus::Deployed { .. }));
//! # Ok(())
//! # }
//! ```

use aura_crypto::blake3_hash;
use aura_state::{Account, MerkleTree, StateDiff, StateKey, StateView};
use aura_types::{Address, Amount, Hash, Nonce};

use crate::abi::ABI_VERSION;
use crate::census::{self, MAX_MODULE_BYTES, ModuleLimits};
use crate::contract::{ContractCall, ContractReceipt, ExecStatus, Executor, RUN_ENTRY, Session};
use crate::error::SandboxError;
use crate::limits::SandboxLimits;
use crate::sandbox::Engine;
use crate::wasmi_engine::WasmiEngine;

/// Magic prefix of the stored bytecode record.
pub const CODE_MAGIC: [u8; 8] = *b"ALCRYCD1";

/// Format version of the record itself (not of the contract, not of the ABI).
pub const CODE_FORMAT_VERSION: u32 = 1;

/// Flat cost of a deploy or an upgrade, before the per-byte price.
pub const DEPLOY_BASE_GAS: u64 = 1_000;

/// Cost of storing one byte of contract code.
pub const CODE_BYTE_GAS: u64 = 10;

/// Highest code version a contract may reach.
///
/// Bounded by `i32::MAX` because `migrate(from, to)` receives the two versions as
/// `i32` arguments — a version the ABI cannot express would be unmigratable.
pub const MAX_CODE_VERSION: u32 = i32::MAX as u32;

/// Optional constructor entry point, run inside `deploy`.
pub const ENTRY_INIT: &str = "init";

/// Optional migration entry point, run inside `upgrade`.
pub const ENTRY_MIGRATE: &str = "migrate";

/// The bytecode record stored under [`StateKey::code`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeRecord {
    /// ABI the code was compiled against.
    pub abi_version: u32,
    /// Monotonic code version, starting at 1.
    pub code_version: u32,
    /// The WASM module itself.
    pub code: Vec<u8>,
}

impl CodeRecord {
    /// A first deployment of `code` at ABI [`ABI_VERSION`].
    pub fn first(code: Vec<u8>) -> Self {
        Self {
            abi_version: ABI_VERSION,
            code_version: 1,
            code,
        }
    }

    /// Identity of the logic: `BLAKE3(code)`, independent of the record header.
    pub fn code_hash(&self) -> Hash {
        blake3_hash(&self.code)
    }

    /// Canonical bytes:
    /// `magic(8) || format u32 || abi u32 || version u32 || len u32 || code`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + self.code.len());
        out.extend_from_slice(&CODE_MAGIC);
        out.extend_from_slice(&CODE_FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.abi_version.to_le_bytes());
        out.extend_from_slice(&self.code_version.to_le_bytes());
        out.extend_from_slice(&(self.code.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.code);
        out
    }

    /// Strict decoder: exact magic, exact format, exact length, no trailing
    /// bytes. A corrupt record is a refusal, never a guess.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LifecycleError> {
        if bytes.len() < 24 || bytes[..8] != CODE_MAGIC {
            return Err(LifecycleError::CorruptCode);
        }
        let read = |at: usize| -> u32 {
            let mut raw = [0u8; 4];
            raw.copy_from_slice(&bytes[at..at + 4]);
            u32::from_le_bytes(raw)
        };
        let format = read(8);
        if format != CODE_FORMAT_VERSION {
            return Err(LifecycleError::CorruptCode);
        }
        let abi_version = read(12);
        let code_version = read(16);
        let len = read(20) as usize;
        if bytes.len() != 24 + len {
            return Err(LifecycleError::CorruptCode);
        }
        Ok(Self {
            abi_version,
            code_version,
            code: bytes[24..].to_vec(),
        })
    }
}

/// Why a lifecycle operation was refused.
///
/// A refusal happens **before** anything is committed, so it is an `Err` and not
/// a receipt: an execution that *ran* and failed is a receipt with a failed
/// status, which is a different fact a node must be able to tell apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifecycleError {
    /// The bytecode failed the X0 census or the size cap.
    Refused(SandboxError),
    /// The module does not export `run` with a usable signature.
    MissingRunEntry,
    /// The address already holds code; redeploy is refused (upgrade is the path).
    AlreadyDeployed(Address),
    /// There is no code at this address.
    NotDeployed(Address),
    /// The stored record is not a valid [`CodeRecord`].
    CorruptCode,
    /// `upgrade` was told to upgrade from a version that is not current.
    VersionMismatch {
        /// Version the caller believes is current.
        expected: u32,
        /// Version actually in state.
        actual: u32,
    },
    /// The code version would exceed [`MAX_CODE_VERSION`].
    VersionExhausted,
    /// The caller cannot cover the value being sent to the new contract.
    InsufficientBalance,
    /// An amount would overflow.
    Overflow,
    /// The engine refused the entry point or the invocation itself.
    Engine(SandboxError),
}

impl LifecycleError {
    /// Stable code for receipts, logs and tests.
    pub fn code(&self) -> String {
        match self {
            // The ban itself is the useful part of a censored refusal, so the
            // code carries it: `refused:censored:float_instruction`.
            Self::Refused(SandboxError::Censored(ban)) => {
                format!("refused:censored:{}", ban.code())
            }
            Self::Refused(SandboxError::Limit(name)) => format!("refused:limit:{name}"),
            Self::Refused(error) => format!("refused:{}", error.code()),
            Self::MissingRunEntry => "missing_run_entry".to_string(),
            Self::AlreadyDeployed(_) => "already_deployed".to_string(),
            Self::NotDeployed(_) => "not_deployed".to_string(),
            Self::CorruptCode => "corrupt_code".to_string(),
            Self::VersionMismatch { .. } => "version_mismatch".to_string(),
            Self::VersionExhausted => "version_exhausted".to_string(),
            Self::InsufficientBalance => "insufficient_balance".to_string(),
            Self::Overflow => "overflow".to_string(),
            Self::Engine(error) => format!("engine:{}", error.code()),
        }
    }
}

impl core::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.code())
    }
}

impl std::error::Error for LifecycleError {}

/// What a lifecycle operation did when it succeeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleStatus {
    /// Code stored, account created, `init` ran clean.
    Deployed {
        /// Identity of the deployed logic.
        code_hash: Hash,
        /// Version assigned (1).
        code_version: u32,
    },
    /// Code replaced, `migrate` ran clean.
    Upgraded {
        /// Version before.
        from: u32,
        /// Version after.
        to: u32,
        /// Identity of the new logic.
        code_hash: Hash,
    },
    /// The operation ran but did not commit (trap, out of gas).
    Reverted,
}

/// The receipt of one lifecycle operation.
#[derive(Clone, Debug)]
pub struct LifecycleReceipt {
    /// What happened.
    pub lifecycle: LifecycleStatus,
    /// Execution status of the entry point that ran (`init`, `migrate`).
    pub status: ExecStatus,
    /// Whole bill: instruction gas + host gas + lifecycle gas.
    pub gas_used: u64,
    /// The part of the bill that came from the ABI and lifecycle charges.
    pub host_gas: u64,
    /// The part of the bill charged for storing bytecode.
    pub code_gas: u64,
    /// Events of the entry point (empty when it was rolled back).
    pub events: Vec<Vec<u8>>,
    /// Writes the operation performed (code record + account + contract writes).
    pub writes: usize,
    /// Canonical diff to commit, or `None` when nothing was committed.
    pub diff: Option<StateDiff>,
    /// Root after the operation (unchanged when nothing was committed).
    pub result_root: Hash,
    /// ABI v2 return data of the entry point, empty when nothing committed.
    pub ret: Vec<u8>,
}

impl LifecycleReceipt {
    /// True when the operation committed.
    pub fn committed(&self) -> bool {
        self.diff.is_some()
    }
}

/// The account record under an address, or the empty account.
pub fn account_of(view: &StateView, address: &Address) -> Account {
    view.get(&StateKey::account(address))
        .and_then(|bytes| Account::from_canonical_bytes(bytes).ok())
        .unwrap_or_else(Account::new_empty)
}

/// The stored bytecode record of `address`, if any.
pub fn code_of(view: &StateView, address: &Address) -> Result<Option<CodeRecord>, LifecycleError> {
    match view.get(&StateKey::code(address)) {
        Some(bytes) => CodeRecord::from_bytes(bytes).map(Some),
        None => Ok(None),
    }
}

/// Deploy / upgrade / call, on one executor configuration.
#[derive(Clone, Debug)]
pub struct Lifecycle<E: Engine = WasmiEngine> {
    executor: Executor<E>,
}

impl Lifecycle<WasmiEngine> {
    /// Stage 1 lifecycle on the default backend.
    pub fn new() -> Self {
        Self {
            executor: Executor::new(),
        }
    }

    /// The same lifecycle with a different limit set.
    pub fn with_limits(limits: SandboxLimits) -> Result<Self, LifecycleError> {
        Ok(Self {
            executor: Executor::with_limits(limits).map_err(LifecycleError::Engine)?,
        })
    }
}

impl Default for Lifecycle<WasmiEngine> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Engine> Lifecycle<E> {
    /// Wrap an explicit executor.
    pub fn with_executor(executor: Executor<E>) -> Self {
        Self { executor }
    }

    /// The executor in force (limits, engine).
    pub fn executor(&self) -> &Executor<E> {
        &self.executor
    }

    /// Census + size + `run` export. Everything a deploy must refuse, refuses
    /// here — before a single byte reaches state.
    fn validate_code(&self, code: &[u8]) -> Result<(), LifecycleError> {
        if code.len() > MAX_MODULE_BYTES {
            return Err(LifecycleError::Refused(SandboxError::Limit("module_bytes")));
        }
        let report = census::census(code, &ModuleLimits::hosted());
        if let Some(ban) = report.ban() {
            return Err(LifecycleError::Refused(SandboxError::Censored(ban.clone())));
        }
        match self
            .executor
            .engine()
            .entry_arity(code, self.executor.limits(), RUN_ENTRY)
            .map_err(LifecycleError::Engine)?
        {
            Some(0) | Some(1) => Ok(()),
            _ => Err(LifecycleError::MissingRunEntry),
        }
    }

    /// Move `value` from `from` to `to` inside the session view.
    fn move_value(
        session: &mut Session<'_, E>,
        from: &Address,
        to: &Address,
        value: Amount,
    ) -> Result<(), LifecycleError> {
        if value == Amount::ZERO || from == to {
            return Ok(());
        }
        let mut sender = account_of(session.view(), from);
        if sender.balance < value {
            return Err(LifecycleError::InsufficientBalance);
        }
        let mut receiver = account_of(session.view(), to);
        sender.debit(value).map_err(|_| LifecycleError::Overflow)?;
        receiver
            .credit(value)
            .map_err(|_| LifecycleError::Overflow)?;
        session
            .view()
            .set(&StateKey::account(from), sender.to_bytes());
        session
            .view()
            .set(&StateKey::account(to), receiver.to_bytes());
        session.note_write(2);
        Ok(())
    }

    /// Store `record` and point the account's `code_hash` at it.
    fn store_code(
        session: &mut Session<'_, E>,
        address: &Address,
        record: &CodeRecord,
    ) -> Result<(), LifecycleError> {
        let mut account = account_of(session.view(), address);
        account.code_hash = record.code_hash();
        session
            .view()
            .set(&StateKey::code(address), record.to_bytes());
        session
            .view()
            .set(&StateKey::account(address), account.to_bytes());
        session.note_write(2);
        Ok(())
    }

    /// Charge for storing `len` bytes of code.
    fn charge_code(session: &mut Session<'_, E>, len: usize) -> Result<u64, LifecycleError> {
        let gas = DEPLOY_BASE_GAS.saturating_add(CODE_BYTE_GAS.saturating_mul(len as u64));
        session
            .charge(gas)
            .map_err(|_| LifecycleError::Refused(SandboxError::Limit("out_of_gas")))?;
        Ok(gas)
    }

    /// Deploy `code` at `call.contract`, fund it with `value` from the caller and
    /// run `init` (if the module exports one) in the same atomic step.
    pub fn deploy(
        &self,
        base: &MerkleTree,
        call: ContractCall,
        code: Vec<u8>,
        value: Amount,
        init_arg: Option<i32>,
    ) -> Result<LifecycleReceipt, LifecycleError> {
        self.validate_code(&code)?;
        let mut session = self.executor.open(base, call);
        if account_of(session.view(), &call.contract).has_code() {
            return Err(LifecycleError::AlreadyDeployed(call.contract));
        }
        Self::move_value(&mut session, &call.caller, &call.contract, value)?;
        let record = CodeRecord::first(code.clone());
        let code_gas = match Self::charge_code(&mut session, code.len()) {
            Ok(gas) => gas,
            Err(_) => {
                session.refuse(ExecStatus::OutOfGas);
                return Ok(receipt_of(session.finish(), LifecycleStatus::Reverted, 0));
            }
        };
        Self::store_code(&mut session, &call.contract, &record)?;

        let init_arity = self
            .executor
            .engine()
            .entry_arity(&code, self.executor.limits(), ENTRY_INIT)
            .map_err(LifecycleError::Engine)?;
        match init_arity {
            None => {}
            Some(0) => session
                .run(&code, ENTRY_INIT, &[])
                .map_err(LifecycleError::Engine)?,
            Some(1) => session
                .run(&code, ENTRY_INIT, &[init_arg.unwrap_or(0)])
                .map_err(LifecycleError::Engine)?,
            Some(_) => {
                return Err(LifecycleError::Refused(SandboxError::Limit("init_arity")));
            }
        }
        let receipt = session.finish();
        let status = if receipt.status.is_success() {
            LifecycleStatus::Deployed {
                code_hash: record.code_hash(),
                code_version: record.code_version,
            }
        } else {
            LifecycleStatus::Reverted
        };
        Ok(receipt_of(receipt, status, code_gas))
    }

    /// Replace the code at `call.contract`, bump the version and run
    /// `migrate(from, to)` (if exported). Balance and storage are untouched.
    pub fn upgrade(
        &self,
        base: &MerkleTree,
        call: ContractCall,
        expected_code_version: u32,
        code: Vec<u8>,
    ) -> Result<LifecycleReceipt, LifecycleError> {
        self.validate_code(&code)?;
        let mut session = self.executor.open(base, call);
        let current = code_of(session.view(), &call.contract)?
            .ok_or(LifecycleError::NotDeployed(call.contract))?;
        if current.code_version != expected_code_version {
            return Err(LifecycleError::VersionMismatch {
                expected: expected_code_version,
                actual: current.code_version,
            });
        }
        let next = current
            .code_version
            .checked_add(1)
            .filter(|version| *version <= MAX_CODE_VERSION)
            .ok_or(LifecycleError::VersionExhausted)?;

        let code_gas = match Self::charge_code(&mut session, code.len()) {
            Ok(gas) => gas,
            Err(_) => {
                session.refuse(ExecStatus::OutOfGas);
                return Ok(receipt_of(session.finish(), LifecycleStatus::Reverted, 0));
            }
        };
        let record = CodeRecord {
            abi_version: ABI_VERSION,
            code_version: next,
            code: code.clone(),
        };
        Self::store_code(&mut session, &call.contract, &record)?;

        let migrate_arity = self
            .executor
            .engine()
            .entry_arity(&code, self.executor.limits(), ENTRY_MIGRATE)
            .map_err(LifecycleError::Engine)?;
        match migrate_arity {
            None => {}
            Some(2) => session
                .run(
                    &code,
                    ENTRY_MIGRATE,
                    &[current.code_version as i32, next as i32],
                )
                .map_err(LifecycleError::Engine)?,
            Some(_) => {
                return Err(LifecycleError::Refused(SandboxError::Limit(
                    "migrate_arity",
                )));
            }
        }
        let receipt = session.finish();
        let status = if receipt.status.is_success() {
            LifecycleStatus::Upgraded {
                from: current.code_version,
                to: next,
                code_hash: record.code_hash(),
            }
        } else {
            LifecycleStatus::Reverted
        };
        Ok(receipt_of(receipt, status, code_gas))
    }

    /// Call the code stored at `call.contract`, with ABI v2 calldata.
    ///
    /// This is what a standard needs: one stored contract, many methods, each
    /// addressed by a selector inside the calldata. The return data is the
    /// contract's answer and is empty whenever the call did not commit.
    pub fn call_data(
        &self,
        base: &MerkleTree,
        call: ContractCall,
        calldata: Vec<u8>,
    ) -> Result<ContractReceipt, LifecycleError> {
        if calldata.len() > crate::abi_v2::MAX_CALLDATA_LEN {
            return Err(LifecycleError::Engine(SandboxError::Limit("calldata")));
        }
        let view = StateView::new(base.clone(), call.gas_limit);
        let record =
            code_of(&view, &call.contract)?.ok_or(LifecycleError::NotDeployed(call.contract))?;
        let mut session = self.executor.open(base, call);
        session
            .set_calldata(calldata)
            .map_err(|_| LifecycleError::Engine(SandboxError::Limit("calldata")))?;
        session
            .run(&record.code, RUN_ENTRY, &[])
            .map_err(LifecycleError::Engine)?;
        Ok(session.finish())
    }

    /// Call the code stored at `call.contract`.
    ///
    /// This is the entry point a node uses: the bytecode comes from state, so a
    /// caller cannot substitute logic for an address.
    pub fn call_stored(
        &self,
        base: &MerkleTree,
        call: ContractCall,
        arg: Option<i32>,
    ) -> Result<ContractReceipt, LifecycleError> {
        let view = StateView::new(base.clone(), call.gas_limit);
        let record =
            code_of(&view, &call.contract)?.ok_or(LifecycleError::NotDeployed(call.contract))?;
        let args: &[i32] = match arg {
            Some(value) => &[value],
            None => &[],
        };
        let mut session = self.executor.open(base, call);
        session
            .run(&record.code, RUN_ENTRY, args)
            .map_err(LifecycleError::Engine)?;
        Ok(session.finish())
    }
}

/// Convert an execution receipt into a lifecycle receipt.
fn receipt_of(
    receipt: ContractReceipt,
    lifecycle: LifecycleStatus,
    code_gas: u64,
) -> LifecycleReceipt {
    LifecycleReceipt {
        lifecycle,
        status: receipt.status,
        gas_used: receipt.gas_used,
        host_gas: receipt.host_gas,
        code_gas,
        events: receipt.events,
        writes: receipt.writes,
        diff: receipt.diff,
        result_root: receipt.result_root,
        ret: receipt.return_data,
    }
}

/// Test support: the account balance under `address` in a view.
pub fn balance_of(view: &StateView, address: &Address) -> Amount {
    account_of(view, address).balance
}

/// Test support: build the empty account a fresh address has.
pub fn empty_account() -> Account {
    Account::new(Nonce::new(0), Amount::ZERO, Hash::ZERO, Hash::ZERO)
}
