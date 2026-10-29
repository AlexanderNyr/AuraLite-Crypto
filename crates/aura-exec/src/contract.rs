//! X2 — contract execution: host state, gas, and the commit/rollback rule.
//!
//! [`Executor`] is the object a node will call in X3/D-phase work. It owns the
//! only three things that matter for correctness:
//!
//! 1. **the budget** — one number, shared by instruction metering and host
//!    calls (1 fuel = 1 gas), so a contract cannot spend twice;
//! 2. **the view** — an S3 [`StateView`] over an immutable base tree; every
//!    write goes into the overlay, never into the tree;
//! 3. **the rule** — a successful call commits a canonical [`StateDiff`], and
//!    *anything else* (out of gas, any trap) rolls the view back so the state
//!    root is byte-identical to what it was before the call.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use aura_exec::contract::{BlockContext, ContractCall, Executor};
//! use aura_state::MerkleTree;
//! use aura_types::Address;
//!
//! let wasm = std::fs::read("contract.wasm")?;
//! let call = ContractCall {
//!     contract: Address::from_slice(&[1u8; 20])?,
//!     caller: Address::from_slice(&[2u8; 20])?,
//!     block: BlockContext { height: 7, timestamp_ms: 1_700_000_000_000 },
//!     gas_limit: 1_000_000,
//! };
//! let receipt = Executor::new().call(&wasm, MerkleTree::new(), call, None)?;
//! // The bill, the storage diff and the events of one call:
//! println!("{:?} gas={} diff={:?}", receipt.status, receipt.gas_used, receipt.diff.is_some());
//! # Ok(())
//! # }
//! ```
//!
//! Gas accounting is reported twice on purpose: `gas_used` is the whole bill
//! (instructions + host calls) and `host_gas` is the part the ABI table
//! explains. `StateDiff::gas_used` is set to the whole bill, so the diff a node
//! stores and the receipt a user sees cannot disagree.

use aura_state::{Account, MerkleTree, StateDiff, StateKey, StateView};
use aura_types::{Address, Amount, Hash};

use crate::abi::{self, AbiError};
use crate::error::{SandboxError, TrapKind};
use crate::limits::SandboxLimits;
use crate::sandbox::{Engine, Execution, Outcome};
use crate::wasmi_engine::WasmiEngine;

/// The entry point a plain call executes.
pub const RUN_ENTRY: &str = "run";

/// Consensus context a contract is allowed to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockContext {
    /// Block height the transaction is included at.
    pub height: u64,
    /// Block timestamp in milliseconds, as agreed by consensus (never a clock).
    pub timestamp_ms: u64,
}

/// Who is calling whom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractCall {
    /// Address whose storage and balance the call may touch.
    pub contract: Address,
    /// Address that signed the transaction.
    pub caller: Address,
    /// Consensus context.
    pub block: BlockContext,
    /// Gas budget for this call.
    pub gas_limit: u64,
}

/// Mutable state behind one invocation: the overlay, the gas meter and the
/// per-call counters.
#[derive(Debug)]
pub struct HostState {
    /// The S3 overlay. Never the base tree.
    pub view: StateView,
    /// The call being executed.
    pub call: ContractCall,
    /// Gas charged by ABI calls so far.
    pub host_gas: u64,
    /// Set when the budget ran out; the adapter turns it into a trap.
    pub out_of_gas: bool,
    /// ABI v2 calldata: fixed before execution, immutable during it.
    pub calldata: Vec<u8>,
    /// ABI v2 return buffer, written by `env2.return_write`.
    pub return_data: Vec<u8>,
    /// X6: the frame stack, innermost last.
    ///
    /// The top-level frame is the contract being called; a nested call pushes
    /// the callee. Two guards read this: the depth limit and the reentrancy
    /// rule (`abi_v3::admit`).
    pub frames: Vec<Address>,
    /// X6: the answer of the last nested call, empty when there was none.
    pub returndata: Vec<u8>,
    /// X6: how many nested calls this frame made.
    pub calls: u32,
    /// Storage writes the call performed.
    ///
    /// Counted even when the call is later rolled back: the receipt is an audit
    /// record, and "tried to write twice, committed nothing" is a different fact
    /// from "never wrote". `diff: None` is what says nothing was committed.
    pub writes: usize,
}

impl HostState {
    /// Wrap a view and a call.
    pub fn new(view: StateView, call: ContractCall) -> Self {
        Self {
            view,
            call,
            host_gas: 0,
            out_of_gas: false,
            calldata: Vec::new(),
            return_data: Vec::new(),
            frames: vec![call.contract],
            returndata: Vec::new(),
            calls: 0,
            writes: 0,
        }
    }

    /// Charge `gas`; `OutOfGas` is returned (and recorded) when the budget is
    /// not enough. The caller must not perform the effect afterwards.
    pub fn charge(&mut self, gas: u64) -> Result<(), AbiError> {
        match self.view.charge_gas(gas) {
            Ok(()) => {
                self.host_gas = self.host_gas.saturating_add(gas);
                Ok(())
            }
            Err(_) => {
                self.out_of_gas = true;
                Err(AbiError::OutOfGas)
            }
        }
    }

    /// Gas still available for this call.
    pub fn gas_left(&self) -> u64 {
        self.view.gas_left()
    }

    /// Bill `spent` units of instruction gas to the view.
    ///
    /// Returns `false` (and raises `out_of_gas`) when the segment does not fit,
    /// which is how a contract that burns its last gas in pure computation is
    /// still reported as `OutOfGas` rather than as a silent revert.
    pub fn settle_instructions(&mut self, spent: u64) -> bool {
        match self.view.charge_gas(spent) {
            Ok(()) => true,
            Err(_) => {
                self.out_of_gas = true;
                false
            }
        }
    }

    /// Discard every uncommitted change, event and gas unit.
    ///
    /// This is the X2 rule in one line: a call that did not *succeed* leaves no
    /// trace. NC-4 in `tools/nc_x2.py` empties this body and requires the gate
    /// to go red.
    pub fn rollback(&mut self) {
        self.view.rollback();
    }

    /// Balance of an address, as stored (missing account = zero).
    pub fn balance_of(&self, address: &Address) -> Amount {
        abi::account_of(self, address).balance
    }

    fn account(&self, address: &Address) -> Account {
        abi::account_of(self, address)
    }

    /// Move `amount` from the calling contract to `target`.
    ///
    /// Both writes are staged only after both accounts have been computed, so a
    /// failure changes nothing. Self-transfers are a no-op that still costs gas.
    pub fn transfer_to(&mut self, target: &Address, amount: Amount) -> Result<(), AbiError> {
        if amount > self.account(&self.call.contract).balance {
            return Err(AbiError::InsufficientBalance);
        }
        if target == &self.call.contract || amount == Amount::ZERO {
            return Ok(());
        }
        let mut from = self.account(&self.call.contract);
        let mut to = self.account(target);
        from.debit(amount).map_err(|_| AbiError::Overflow)?;
        to.credit(amount).map_err(|_| AbiError::Overflow)?;
        self.view
            .set(&StateKey::account(&self.call.contract), from.to_bytes());
        self.view.set(&StateKey::account(target), to.to_bytes());
        self.writes += 2;
        Ok(())
    }

    /// Stage the value of a nested call, returning what is needed to undo it.
    ///
    /// The callee must see a funded account while it runs, so the transfer is
    /// written to the *caller's* view before the callee is instantiated. The two
    /// previous account encodings are returned so the caller can put them back if
    /// the callee fails: a frame commits wholly or not at all, and value is part
    /// of the frame.
    pub fn stage_value(
        &mut self,
        target: &Address,
        amount: Amount,
    ) -> Result<Option<ValueUndo>, AbiError> {
        if amount == Amount::ZERO || target == &self.call.contract {
            return Ok(None);
        }
        let undo = ValueUndo {
            from: self.call.contract,
            to: *target,
            from_before: self
                .view
                .get(&StateKey::account(&self.call.contract))
                .map(<[u8]>::to_vec),
            to_before: self
                .view
                .get(&StateKey::account(target))
                .map(<[u8]>::to_vec),
        };
        self.transfer_to(target, amount)?;
        Ok(Some(undo))
    }

    /// Put a staged value transfer back exactly as it was.
    pub fn undo_value(&mut self, undo: ValueUndo) {
        match undo.from_before {
            Some(bytes) => self.view.set(&StateKey::account(&undo.from), bytes),
            None => self.view.delete(&StateKey::account(&undo.from)),
        }
        match undo.to_before {
            Some(bytes) => self.view.set(&StateKey::account(&undo.to), bytes),
            None => self.view.delete(&StateKey::account(&undo.to)),
        }
    }

    /// Fold a committed child diff into this frame's overlay.
    ///
    /// Paths are hashes of keys, so they identify the same slot in the child's
    /// overlay and in ours; only the *version* differs, and this frame's version
    /// is the one that commits. That is what makes a nested call atomic: if the
    /// child fails there is no diff to merge, and nothing it wrote ever existed.
    pub fn merge_child(&mut self, diff: &aura_state::StateDiff) {
        for (path, value) in &diff.changes {
            match value {
                Some(bytes) => self.view.set_path(*path, bytes.clone()),
                None => self.view.delete_path(*path),
            }
        }
        self.writes += diff.changes.len();
    }

    /// Append the events a committed child emitted, in order.
    pub fn merge_events(&mut self, events: &[Vec<u8>]) {
        for event in events {
            self.view.emit(event.clone());
        }
    }

    /// Append one event in canonical form: `u32le(topic_len) || topic || data`.
    pub fn push_event(&mut self, topic: &[u8], data: &[u8]) -> Result<(), AbiError> {
        if self.view.events().len() >= aura_state::MAX_EVENTS {
            return Err(AbiError::TooManyEvents);
        }
        let mut event = Vec::with_capacity(4 + topic.len() + data.len());
        event.extend_from_slice(&(topic.len() as u32).to_le_bytes());
        event.extend_from_slice(topic);
        event.extend_from_slice(data);
        self.view.emit(event);
        Ok(())
    }
}

/// The state a value transfer overwrote, so a failed frame can restore it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueUndo {
    /// Address the value was taken from (the calling contract).
    pub from: Address,
    /// Address the value was sent to (the callee).
    pub to: Address,
    /// Encoding of `from`'s account before the transfer, if it existed.
    pub from_before: Option<Vec<u8>>,
    /// Encoding of `to`'s account before the transfer, if it existed.
    pub to_before: Option<Vec<u8>>,
}

/// How a call ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecStatus {
    /// `run` returned normally with this value.
    Returned(i64),
    /// The budget ran out; the state was rolled back.
    OutOfGas,
    /// The contract trapped; the state was rolled back.
    Trap(TrapKind),
    /// The module never ran: census refusal, size cap, missing `run`, …
    Rejected,
}

impl ExecStatus {
    /// Stable label for receipts and evidence.
    pub fn label(self) -> String {
        match self {
            Self::Returned(value) => format!("returned:{value}"),
            Self::OutOfGas => "out_of_gas".to_string(),
            Self::Trap(kind) => format!("trap:{}", kind.code()),
            Self::Rejected => "rejected".to_string(),
        }
    }

    /// True when the call may be committed.
    pub fn is_success(self) -> bool {
        matches!(self, Self::Returned(_))
    }
}

/// The result of one contract call.
#[derive(Clone, Debug)]
pub struct ContractReceipt {
    /// Outcome.
    pub status: ExecStatus,
    /// Whole bill: instructions + host calls.
    pub gas_used: u64,
    /// The part of `gas_used` explained by the ABI table.
    pub host_gas: u64,
    /// Events the call emitted (empty when it was rolled back).
    pub events: Vec<Vec<u8>>,
    /// Number of storage writes the call performed.
    pub writes: usize,
    /// Canonical diff to commit, or `None` when the call failed.
    pub diff: Option<StateDiff>,
    /// Root the state has after this call (unchanged on failure).
    pub result_root: Hash,
    /// ABI v2 return data, empty unless the call succeeded.
    ///
    /// A reverted call returns nothing: the contract's answer is part of what
    /// was rolled back, and a caller that reads a return value must also know
    /// the call committed.
    pub return_data: Vec<u8>,
}

/// A node-side executor: one sandbox configuration, used for many calls.
#[derive(Debug, Clone)]
pub struct Executor<E: Engine = WasmiEngine> {
    limits: SandboxLimits,
    engine: E,
}

impl Executor<WasmiEngine> {
    /// Stage 1 executor on the default backend.
    pub fn new() -> Self {
        Self {
            limits: SandboxLimits::restrictive(),
            engine: WasmiEngine,
        }
    }

    /// The same executor with a tighter limit set (tests, future phases).
    pub fn with_limits(limits: SandboxLimits) -> Result<Self, SandboxError> {
        limits.validate().map_err(SandboxError::Limit)?;
        Ok(Self {
            limits,
            engine: WasmiEngine,
        })
    }
}

impl Default for Executor<WasmiEngine> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Engine> Executor<E> {
    /// Build an executor over an explicit backend.
    pub fn with_engine(limits: SandboxLimits, engine: E) -> Result<Self, SandboxError> {
        limits.validate().map_err(SandboxError::Limit)?;
        Ok(Self { limits, engine })
    }

    /// Limits in force.
    pub fn limits(&self) -> &SandboxLimits {
        &self.limits
    }

    /// The backend in force.
    pub fn engine(&self) -> &E {
        &self.engine
    }

    /// Open a session against `base`: a state view, a host state and the call.
    ///
    /// # Budget
    ///
    /// The view is created with `min(call.gas_limit, limits.fuel)` so the
    /// transaction's budget and the node's ceiling cannot disagree, and the
    /// instruction meter is derived from that one number by the backend.
    pub fn open(&self, base: &MerkleTree, call: ContractCall) -> Session<'_, E> {
        let budget = call.gas_limit.min(self.limits.fuel);
        let call = ContractCall {
            gas_limit: budget,
            ..call
        };
        let view = StateView::new(base.clone(), budget);
        let base_root = view.base_root();
        let version = view.base_version() + 1;
        Session {
            executor: self,
            base_root,
            version,
            host: HostState::new(view, call),
            execution: None,
            pre_gas: 0,
            extra_writes: 0,
        }
    }

    /// Run one call of the `run` entry point against `base`.
    ///
    /// The returned receipt carries the diff to commit on success. On failure
    /// nothing is committed **and** the view is rolled back inside the host
    /// state, so the base tree is untouched either way (a `StateView` never
    /// mutates its base).
    pub fn call(
        &self,
        wasm: &[u8],
        base: MerkleTree,
        call: ContractCall,
        arg: Option<i32>,
    ) -> Result<ContractReceipt, SandboxError> {
        let args: &[i32] = match arg {
            Some(value) => &[value],
            None => &[],
        };
        let mut session = self.open(&base, call);
        session.run(wasm, RUN_ENTRY, args)?;
        Ok(session.finish())
    }

    /// Call `run` with ABI v2 calldata.
    ///
    /// The calldata is part of the invocation, not of the state: it is fixed
    /// before execution and reaches the contract only through
    /// `env2.calldata_read`, priced per byte.
    pub fn call_data(
        &self,
        wasm: &[u8],
        base: MerkleTree,
        call: ContractCall,
        calldata: &[u8],
    ) -> Result<ContractReceipt, SandboxError> {
        let mut session = self.open(&base, call);
        session
            .set_calldata(calldata.to_vec())
            .map_err(|_| SandboxError::Limit("calldata"))?;
        session.run(wasm, RUN_ENTRY, &[])?;
        Ok(session.finish())
    }

    /// Convenience wrapper for a call that is expected to be rejected before
    /// execution (census, size, imports): returns `ExecStatus::Rejected`.
    pub fn rejected(&self, base_root: Hash) -> ContractReceipt {
        ContractReceipt {
            status: ExecStatus::Rejected,
            gas_used: 0,
            host_gas: 0,
            events: Vec::new(),
            writes: 0,
            diff: None,
            result_root: base_root,
            return_data: Vec::new(),
        }
    }
}

/// One in-flight invocation: the overlay, the budget and the outcome so far.
///
/// A session is what X2's `Executor::call` and X3's lifecycle operations share.
/// Lifecycle work that is *not* a host call — storing bytecode, bumping a code
/// version, moving an initial balance — is performed directly on
/// [`Session::view`] and billed through [`Session::charge`], so the receipt
/// still has exactly one bill.
pub struct Session<'a, E: Engine> {
    executor: &'a Executor<E>,
    host: HostState,
    base_root: Hash,
    version: u64,
    execution: Option<Execution>,
    /// Gas charged by lifecycle operations rather than by the ABI.
    pre_gas: u64,
    extra_writes: usize,
}

impl<E: Engine> Session<'_, E> {
    /// The overlay. Lifecycle code writes here; a contract cannot reach it
    /// except through the ABI.
    pub fn view(&mut self) -> &mut StateView {
        &mut self.host.view
    }

    /// The host state (read-only view of the call, the budget and the counters).
    pub fn host(&self) -> &HostState {
        &self.host
    }

    /// Version this session commits.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Charge `gas` for a lifecycle operation.
    pub fn charge(&mut self, gas: u64) -> Result<(), AbiError> {
        match self.host.charge(gas) {
            Ok(()) => {
                self.pre_gas = self.pre_gas.saturating_add(gas);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Account for a state write performed outside the ABI.
    pub fn note_write(&mut self, count: usize) {
        self.extra_writes += count;
    }

    /// Declare a failed execution without running anything (a refusal that
    /// happened before the engine was reached).
    pub fn refuse(&mut self, status: ExecStatus) {
        self.execution = Some(Execution {
            outcome: match status {
                ExecStatus::Trap(kind) => Outcome::Trap(kind),
                _ => Outcome::Trap(TrapKind::Other),
            },
            fuel_used: 0,
            trace: Vec::new(),
            exports: Vec::new(),
        });
        self.host.out_of_gas = matches!(status, ExecStatus::OutOfGas);
    }

    /// Hand the invocation its ABI v3 frame stack.
    ///
    /// A nested frame inherits its parent's stack and appends itself, so the
    /// reentrancy rule and the depth limit see exactly what the executor sees.
    pub fn set_frames(&mut self, frames: Vec<Address>) {
        self.host.frames = frames;
    }

    /// Hand the invocation its ABI v2 calldata.
    ///
    /// Refused (not truncated) above [`crate::abi_v2::MAX_CALLDATA_LEN`]: a host
    /// that quietly dropped the tail of a `transfer` would be a fund-loss bug.
    pub fn set_calldata(&mut self, calldata: Vec<u8>) -> Result<(), AbiError> {
        if calldata.len() > crate::abi_v2::MAX_CALLDATA_LEN {
            return Err(AbiError::ValueTooLong);
        }
        self.host.calldata = calldata;
        Ok(())
    }

    /// Run `entry` of `wasm` inside this session.
    pub fn run(&mut self, wasm: &[u8], entry: &str, args: &[i32]) -> Result<(), SandboxError> {
        let call = self.host.call;
        let host = core::mem::replace(
            &mut self.host,
            // Placeholder that is overwritten immediately below; the engine
            // hands the real host state back.
            HostState::new(StateView::new(MerkleTree::new(), 0), call),
        );
        let (execution, host) =
            self.executor
                .engine
                .execute_hosted(wasm, &self.executor.limits, entry, args, host)?;
        self.host = host;
        self.execution = Some(execution);
        Ok(())
    }

    /// Bill the whole session and produce the receipt.
    pub fn finish(self) -> ContractReceipt {
        let Session {
            base_root,
            version,
            mut host,
            execution,
            pre_gas,
            extra_writes,
            ..
        } = self;
        let host_gas = host.host_gas;
        let instruction_gas = execution.as_ref().map_or(0, |e| e.fuel_used);
        let gas_used = instruction_gas.max(host_gas).max(pre_gas);
        let writes = host.writes + extra_writes;
        let (status, diff, events, kept_return) = match execution.map(|execution| execution.outcome)
        {
            // No entry point ran (a deploy of a module without `init`): the
            // lifecycle writes are the whole operation, and there is nothing to
            // fail.
            None => (
                ExecStatus::Returned(0),
                Some(host.view.diff(version)),
                host.view.events().to_vec(),
                host.return_data.clone(),
            ),
            Some(Outcome::Returned(value)) if !host.out_of_gas => (
                ExecStatus::Returned(value),
                Some(host.view.diff(version)),
                host.view.events().to_vec(),
                host.return_data.clone(),
            ),
            Some(Outcome::Returned(_)) => {
                host.rollback();
                (
                    ExecStatus::OutOfGas,
                    None,
                    host.view.events().to_vec(),
                    Vec::new(),
                )
            }
            Some(Outcome::Trap(kind)) => {
                let status = if host.out_of_gas
                    || matches!(kind, TrapKind::OutOfGas | TrapKind::OutOfFuel)
                {
                    ExecStatus::OutOfGas
                } else {
                    ExecStatus::Trap(kind)
                };
                host.rollback();
                (status, None, host.view.events().to_vec(), Vec::new())
            }
        };
        let result_root = diff
            .as_ref()
            .map(|diff| diff.result_root)
            .unwrap_or(base_root);
        ContractReceipt {
            status,
            gas_used,
            host_gas,
            events,
            writes,
            diff,
            result_root,
            return_data: kept_return,
        }
    }
}
