//! X4 gate (host side): the SDK against the node's published ABI.
//!
//! Two halves, and both matter:
//!
//! * **Agreement** — the SDK's idea of ABI v1 (names, arities, limits, status
//!   codes, module name) must equal `aura_exec::abi`'s, checked against the real
//!   crate rather than against a comment. A divergence here would compile fine
//!   and fail at deploy.
//! * **Behaviour** — the wrappers, the codec and the storage layer run against
//!   the mock backend, which speaks the same `extern "C"` contract as the node:
//!   same status codes, same limits, same absent-key status.
//!
//! The mock does **not** meter gas; that is the engine's job and it is asserted
//! in `crates/aura-exec/tests/x4_contracts.rs`, where the same contracts are
//! deployed and called through the real `Lifecycle`.

use aura_sdk::prelude::*;
use aura_sdk::{NO_ARG, codec::Codec};

const CONTRACT: [u8; 20] = [0x11; 20];
const ALICE: [u8; 20] = [0xa1; 20];

fn fresh() {
    abi::mock::reset(7, 1_700_000_000_000);
    abi::mock::set_this(CONTRACT);
    abi::mock::set_balance(CONTRACT, 1_000);
}

#[test]
fn sdk_and_node_publish_the_same_abi() {
    // The node is the source of truth; the SDK is a client of it.
    assert_eq!(abi::ABI_MODULE, aura_exec::abi::ABI_MODULE);
    assert_eq!(abi::ABI_VERSION, aura_exec::abi::ABI_VERSION);
    assert_eq!(abi::MAX_KEY_LEN, aura_exec::abi::MAX_KEY_LEN);
    assert_eq!(abi::MAX_VALUE_LEN, aura_exec::abi::MAX_VALUE_LEN);
    assert_eq!(abi::MAX_TOPIC_LEN, aura_exec::abi::MAX_TOPIC_LEN);
    assert_eq!(abi::ADDRESS_LEN, aura_exec::abi::ADDRESS_LEN);
    assert_eq!(abi::AMOUNT_LEN, aura_exec::abi::AMOUNT_LEN);

    assert_eq!(abi::FUNCTIONS.len(), aura_exec::abi::ABI_FUNCTIONS.len());
    for (index, name) in abi::FUNCTIONS.iter().enumerate() {
        assert_eq!(
            *name,
            aura_exec::abi::ABI_FUNCTIONS[index],
            "function {index} differs"
        );
        assert_eq!(
            Some(abi::ARITIES[index]),
            aura_exec::abi::arity(name),
            "arity of {name} differs"
        );
    }

    // Every status code the SDK knows is a code the node can return, and the
    // mapping is the same in both directions.
    for code in [-1, -2, -3, -4, -5, -6, -7, -8, -11] {
        let error = Error::from_code(code);
        assert_eq!(error.code(), code);
        let node = aura_exec::abi::AbiError::from_code(code);
        assert_eq!(
            error.name(),
            node.map(|error| error.name()).unwrap_or("?"),
            "name of {code} differs"
        );
    }
    assert_eq!(Error::from_code(-12), Error::Unknown(-12));
}

#[test]
fn storage_round_trips_every_supported_type() {
    fresh();
    let counter: Slot<u64> = Slot::at(b"counter");
    assert_eq!(counter.get(), Ok(None), "a fresh slot is absent");

    counter.set(41).unwrap();
    assert_eq!(counter.get(), Ok(Some(41)));
    counter.write(counter.get_or(0) + 1);
    assert_eq!(counter.get_or(0), 42);

    let big: Slot<u128> = Slot::at(b"big");
    big.set(u128::MAX).unwrap();
    assert_eq!(big.get(), Ok(Some(u128::MAX)));

    let flag: Slot<bool> = Slot::at(b"flag");
    flag.set(true).unwrap();
    assert_eq!(flag.get(), Ok(Some(true)));

    let digest: Slot<[u8; 32]> = Slot::at(b"digest");
    digest.set([9u8; 32]).unwrap();
    assert_eq!(digest.get(), Ok(Some([9u8; 32])));

    counter.clear().unwrap();
    assert_eq!(counter.get(), Ok(None), "clear deletes the slot");
}

#[test]
fn a_slot_refuses_a_value_written_by_another_layout() {
    fresh();
    let wide: Slot<u64> = Slot::at(b"x");
    wide.set(7).unwrap();
    let narrow: Slot<u32> = Slot::at(b"x");
    assert_eq!(
        narrow.get(),
        Err(Error::ValueTooLong),
        "8 bytes read as 4 must fail, not truncate"
    );
}

#[test]
fn composed_keys_are_distinct_and_bounded() {
    fresh();
    let first: Slot<u128> = Slot::with_key(Key::new(&[b"bal:", &ALICE]).unwrap());
    let second: Slot<u128> = Slot::with_key(Key::new(&[b"bal:", &CONTRACT]).unwrap());
    first.set(1).unwrap();
    second.set(2).unwrap();
    assert_eq!(first.get(), Ok(Some(1)));
    assert_eq!(second.get(), Ok(Some(2)));

    // 33 bytes is over the published key limit; 32 is exactly on it.
    assert_eq!(Key::new(&[&[7u8; 33][..]]), Err(Error::KeyTooLong));
    assert!(Key::new(&[&[7u8; 32][..]]).is_ok());
    assert_eq!(Key::new(&[b"a", b""]), Err(Error::KeyTooLong));
    assert_eq!(Key::new(&[]), Err(Error::KeyTooLong));
}

#[test]
fn the_abi_value_ceiling_is_enforced_before_the_call() {
    fresh();
    let oversized = [0u8; aura_sdk::abi::MAX_VALUE_LEN + 1];
    assert_eq!(
        abi::storage_write(b"too-big", &oversized),
        Err(Error::ValueTooLong)
    );
    let exact = [0u8; aura_sdk::abi::MAX_VALUE_LEN];
    assert_eq!(abi::storage_write(b"exactly", &exact), Ok(()));
}

#[test]
fn events_are_ordered_and_capped() {
    fresh();
    emit(b"first", b"1").unwrap();
    emit_u64(b"second", 2).unwrap();
    let events = abi::mock::events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], (b"first".to_vec(), b"1".to_vec()));
    assert_eq!(events[1], (b"second".to_vec(), 2u64.to_le_bytes().to_vec()));

    // An empty topic and an oversized one are both refused at the call site,
    // before the host is asked.
    assert_eq!(emit(b"", b"x"), Err(Error::TopicTooLong));
    let long = [b'x'; aura_sdk::abi::MAX_TOPIC_LEN + 1];
    assert_eq!(emit(&long, b"x"), Err(Error::TopicTooLong));
    assert_eq!(abi::mock::events().len(), 2, "refusals emit nothing");
}

#[test]
fn value_moves_and_a_shortfall_is_reported() {
    fresh();
    abi::mock::set_balance(ALICE, 0);
    assert_eq!(abi::balance(&ALICE), Ok(0));
    assert_eq!(abi::transfer(&ALICE, 400), Ok(()));
    assert_eq!(abi::balance(&ALICE), Ok(400));
    assert_eq!(abi::mock::balance_of(CONTRACT), 600);

    assert_eq!(
        abi::transfer(&ALICE, 601),
        Err(Error::InsufficientBalance),
        "the ABI refuses rather than going negative"
    );
    assert_eq!(
        abi::balance(&ALICE),
        Ok(400),
        "a refused transfer moved nothing"
    );
}

#[test]
fn block_context_comes_from_the_host() {
    fresh();
    assert_eq!(block_height(), Ok(7));
    assert_eq!(block_timestamp_ms(), Ok(1_700_000_000_000));
    assert_eq!(abi_version(), Ok(u64::from(abi::ABI_VERSION)));
}

#[test]
fn hashing_is_deterministic_in_the_sdk_path() {
    fresh();
    let a = hash(b"auralite").unwrap();
    let b = hash(b"auralite").unwrap();
    let c = hash(b"auralite!").unwrap();
    assert_eq!(a, b, "same input, same digest");
    assert_ne!(a, c, "different input, different digest");
}

#[test]
fn absent_and_unknown_are_statuses_not_errors() {
    fresh();
    let mut buf = [0u8; 64];
    assert_eq!(abi::storage_read(b"missing", &mut buf), Ok(None));
    assert_eq!(
        abi::abi_version(),
        Ok(1),
        "a known field is a value, not a status"
    );
}

#[test]
fn the_entry_point_attribute_produces_the_export_the_chain_calls() {
    // The wrapper is what WASM exports; on the host it is an ordinary function,
    // so calling it is the same as the engine calling `run`.
    #[aura_sdk::entrypoint]
    fn run() -> u32 {
        7
    }

    #[aura_sdk::constructor(arg)]
    fn init(initial: i32) -> u32 {
        initial as u32
    }

    #[aura_sdk::migrate]
    fn migrate(from: i32, to: i32) -> u32 {
        (from + to) as u32
    }

    assert_eq!(run(), 7);
    assert_eq!(init(5), 5);
    assert_eq!(init(NO_ARG), 0);
    assert_eq!(migrate(1, 2), 3);
}

#[test]
fn the_codec_is_little_endian_and_length_checking() {
    let mut buf = [0u8; 16];
    assert_eq!(0x0102u16.encode(&mut buf).unwrap(), &[0x02, 0x01]);
    assert_eq!(u16::decode_from(&[0x02, 0x01]), Ok(0x0102));
    assert_eq!(u32::decode_from(&[0x02, 0x01]), Err(Error::ValueTooLong));
    assert_eq!(bool::decode_from(&[2]), Err(Error::Unknown(-12)));
}

#[test]
fn the_mock_counts_its_calls_so_a_test_can_assert_silence() {
    fresh();
    let before = abi::mock::calls();
    let slot: Slot<u64> = Slot::at(b"quiet");
    slot.set(1).unwrap();
    slot.get().unwrap();
    assert_eq!(abi::mock::calls(), before + 2);
}
