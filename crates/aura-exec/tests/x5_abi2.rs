//! X5 gate (ABI v2): calldata, return data, caller and self, at the host boundary.
//!
//! The standards prove the surface is *usable*; this file proves it is *bounded*.
//! Everything here drives `abi_v2::dispatch` directly with a mock linear memory, so
//! the assertions are about the host contract rather than about a contract's
//! behaviour: what is refused, what is truncated, what is charged, and what a
//! contract can and cannot see.
//!
//! A note on why these exist as their own gate: a standard can only be as honest as
//! the host it runs on, and "the return buffer is limited" is a claim that must be
//! checked where the buffer actually lives.

use aura_exec::abi::LinearMemory;
use aura_exec::abi_v2::{self, Abi2Fn, MAX_CALLDATA_LEN, MAX_RETURN_LEN};
use aura_exec::contract::HostState;
use aura_exec::{BlockContext, ContractCall};
use aura_state::{MerkleTree, StateView};
use aura_types::{Address, Amount, Hash, Nonce};

const CONTRACT: [u8; 20] = [0x11; 20];
const CALLER: [u8; 20] = [0x22; 20];

/// A flat 64 KiB of linear memory with the ABI's bounds rules.
struct MockMemory {
    bytes: Vec<u8>,
}

impl MockMemory {
    fn new() -> Self {
        Self {
            bytes: vec![0u8; 64 * 1024],
        }
    }
}

impl LinearMemory for MockMemory {
    fn read(&self, ptr: u32, len: u32) -> Option<Vec<u8>> {
        let start = ptr as usize;
        let end = start.checked_add(len as usize)?;
        self.bytes.get(start..end).map(<[u8]>::to_vec)
    }

    fn write(&mut self, ptr: u32, data: &[u8]) -> bool {
        let start = ptr as usize;
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        match self.bytes.get_mut(start..end) {
            Some(slot) => {
                slot.copy_from_slice(data);
                true
            }
            None => false,
        }
    }
}

fn fresh_host(calldata: Vec<u8>) -> HostState {
    let call = ContractCall {
        contract: Address::from_slice(&CONTRACT).expect("contract"),
        caller: Address::from_slice(&CALLER).expect("caller"),
        block: BlockContext {
            height: 1,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: 1_000_000,
    };
    let entries: Vec<(Hash, Vec<u8>)> = vec![(
        aura_state::StateKey::account(&call.caller).digest(),
        aura_state::Account::new(Nonce::new(0), Amount::from_units(0), Hash::ZERO, Hash::ZERO)
            .to_bytes(),
    )];
    let tree = MerkleTree::from_entries(1, &entries);
    let mut host = HostState::new(StateView::new(tree, call.gas_limit), call);
    host.calldata = calldata;
    host
}

#[test]
fn calldata_len_reports_without_copying() {
    let mut host = fresh_host(vec![1, 2, 3, 4, 5]);
    let mut memory = MockMemory::new();
    let code = abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::CalldataLen, &[]);
    assert_eq!(code, 5);
    assert_eq!(
        host.calldata.len(),
        5,
        "reading a length must not consume it"
    );
    assert!(nothing_written(&memory), "nothing was written");
}

#[test]
fn calldata_read_copies_at_most_what_exists() {
    let mut host = fresh_host(vec![7, 8, 9]);
    let mut memory = MockMemory::new();
    // Ask for 100 bytes when 3 exist: 3 are copied and 3 is the answer.
    let code = abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::CalldataRead, &[0, 100]);
    assert_eq!(code, 3);
    assert_eq!(memory.read(0, 3), Some(vec![7, 8, 9]));
}

#[test]
fn a_bad_pointer_is_a_status_not_a_corrupt_write() {
    let mut host = fresh_host(vec![1, 2, 3]);
    let mut memory = MockMemory::new();
    // A pointer past the end of memory: refused before anything is touched.
    let code = abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::CalldataRead, &[70_000, 16]);
    assert_eq!(code, -1, "out-of-bounds must be OutOfMemory");
    assert!(nothing_written(&memory));
}

#[test]
fn return_data_is_bounded() {
    let mut host = fresh_host(Vec::new());
    let mut memory = MockMemory::new();
    let ok = abi_v2::dispatch(
        &mut host,
        &mut memory,
        Abi2Fn::ReturnWrite,
        &[0, MAX_RETURN_LEN as i32],
    );
    assert_eq!(ok, 0);
    assert_eq!(host.return_data.len(), MAX_RETURN_LEN);

    let too_long = abi_v2::dispatch(
        &mut host,
        &mut memory,
        Abi2Fn::ReturnWrite,
        &[0, (MAX_RETURN_LEN + 1) as i32],
    );
    assert_eq!(too_long, -3, "over the limit is ValueTooLong");
    assert_eq!(
        host.return_data.len(),
        MAX_RETURN_LEN,
        "a refused write must not shrink or grow the buffer"
    );
}

#[test]
fn caller_and_self_are_two_different_addresses() {
    let mut host = fresh_host(Vec::new());
    let mut memory = MockMemory::new();
    assert_eq!(
        abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::Caller, &[0, 20]),
        20
    );
    assert_eq!(memory.read(0, 20), Some(CALLER.to_vec()));

    assert_eq!(
        abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::SelfAddress, &[32, 20]),
        20
    );
    assert_eq!(memory.read(32, 20), Some(CONTRACT.to_vec()));

    // A buffer smaller than an address is refused, not partially written.
    assert_eq!(
        abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::Caller, &[64, 19]),
        -13
    );
    assert_eq!(memory.read(64, 19), Some(vec![0u8; 19]));
}

#[test]
fn the_surface_version_is_published_and_every_function_is_in_the_gas_table() {
    let mut host = fresh_host(Vec::new());
    let mut memory = MockMemory::new();
    let version = abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::AbiVersion, &[]);
    assert_eq!(version, abi_v2::ABI2_VERSION as i32);
    assert_eq!(abi_v2::ABI2_FUNCTIONS.len(), abi_v2::ABI2_GAS_TABLE.len());
    for function in abi_v2::ABI2_FUNCTIONS {
        assert!(
            abi_v2::gas_for(function, 0).is_some(),
            "{function} has no price"
        );
        assert!(
            abi_v2::ABI2_GAS_TABLE
                .iter()
                .any(|row| row.name == function),
            "{function} is missing from the table"
        );
    }
}

#[test]
fn calldata_is_charged_per_byte_and_the_budget_is_shared() {
    // The whole point of one counter: a contract cannot read 1 KiB of calldata for
    // free. The charge is taken before the copy, so a read that cannot be paid for
    // is a read that did not happen.
    let mut host = fresh_host(vec![0u8; 64]);
    let mut memory = MockMemory::new();
    let before = host.host_gas;
    let code = abi_v2::dispatch(&mut host, &mut memory, Abi2Fn::CalldataRead, &[0, 64]);
    assert_eq!(code, 64);
    let price = abi_v2::gas_for("calldata_read", 64).expect("priced");
    assert_eq!(
        host.host_gas - before,
        price,
        "the published price was charged"
    );

    // A budget too small for the read: the status says so and nothing is copied.
    let mut poor = fresh_host(vec![0u8; 64]);
    poor.view = StateView::new(MerkleTree::new(), 5);
    let mut memory = MockMemory::new();
    let code = abi_v2::dispatch(&mut poor, &mut memory, Abi2Fn::CalldataRead, &[0, 64]);
    assert_eq!(code, -8, "out of gas is a status the contract sees");
    assert!(
        poor.out_of_gas,
        "and it is recorded for the engine to trap on"
    );
    assert_eq!(
        memory.read(0, 64),
        Some(vec![0u8; 64]),
        "nothing was copied"
    );
}

#[test]
fn the_session_refuses_oversize_calldata_before_the_engine_runs() {
    // The dispatch above does not police the length: the *session* does, before a
    // single instruction executes. That ordering is the point — a host that ran the
    // contract and then noticed the payload was too large would have already paid
    // for the work.
    let call = ContractCall {
        contract: Address::from_slice(&CONTRACT).expect("contract"),
        caller: Address::from_slice(&CALLER).expect("caller"),
        block: BlockContext {
            height: 1,
            timestamp_ms: 1_700_000_000_000,
        },
        gas_limit: 1_000_000,
    };
    let executor = aura_exec::contract::Executor::new();
    let oversized = vec![0u8; MAX_CALLDATA_LEN + 1];
    let wasm = b"\0asm\x01\0\0\0";
    let error = executor
        .call_data(wasm, MerkleTree::new(), call, &oversized)
        .expect_err("oversize calldata must be refused");
    assert!(
        matches!(error, aura_exec::SandboxError::Limit("calldata")),
        "unexpected error: {error:?}"
    );

    // Exactly at the limit the session accepts it and the (empty) module is the
    // thing that fails — which is the proof that the boundary is the limit, not
    // something smaller.
    let at_limit = vec![0u8; MAX_CALLDATA_LEN];
    let error = executor
        .call_data(wasm, MerkleTree::new(), call, &at_limit)
        .expect_err("an empty module has no run entry point");
    assert!(
        !matches!(error, aura_exec::SandboxError::Limit("calldata")),
        "the limit itself must be allowed"
    );
}

/// True when the memory holds nothing but zeros — used to assert "no write".
fn nothing_written(memory: &MockMemory) -> bool {
    memory.bytes.iter().all(|byte| *byte == 0)
}
