//! C9 gate: proptest properties over codec, tx, state keys and the rig.
//!
//! These properties are deliberately able to catch real regressions: the C9
//! NCs inject a non-minimal-varint bug and a disabled trailing-byte guard
//! and require this suite to go red.

use aura_codec::{
    CODEC_VERSION, CanonicalRecord, CodecError, Reader, Writer, decode_canonical, encode_to_bytes,
    read_varint, write_varint,
};
use aura_genesis::Genesis;
use aura_state::StateKey;
use aura_testkit::{FUZZ_TARGETS, FuzzOutcome, fuzz_one};
use aura_tx::Tx;
use aura_types::{Address, Amount, CanonicalMap, ChainId, Domain, Hash, Height, Nonce};
use proptest::prelude::*;

proptest! {
    // C9 NCs deliberately inject bugs; proptest must NOT persist those
    // artificial failure seeds into the repository.
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Any u64 must survive a minimal varint round-trip, and a deliberately
    /// padded encoding of the same value must be rejected.
    #[test]
    fn varint_is_minimal_and_roundtrips(value in any::<u64>()) {
        let mut writer = Writer::new();
        write_varint(&mut writer, value);
        let minimal = writer.into_bytes();

        let mut reader = Reader::new(&minimal);
        prop_assert_eq!(read_varint(&mut reader).unwrap(), value);
        prop_assert_eq!(reader.remaining_len(), 0);

        if let Some((last, prefix)) = minimal.split_last() {
            let mut padded = prefix.to_vec();
            padded.push(last | 0x80);
            padded.push(0x00);
            let mut padded_reader = Reader::new(&padded);
            prop_assert!(
                read_varint(&mut padded_reader).is_err(),
                "non-minimal varint for {value} must be rejected"
            );
        }
    }

    /// Extra bytes after a complete value must always be reported.
    #[test]
    fn trailing_bytes_are_always_rejected(extra in prop::collection::vec(any::<u8>(), 1..64)) {
        let mut valid = encode_to_bytes(&vec![1u8, 2, 3]);
        valid.extend_from_slice(&extra);
        prop_assert_eq!(
            decode_canonical::<Vec<u8>>(&valid),
            Err(CodecError::TrailingBytes(extra.len()))
        );
    }

    #[test]
    fn canonical_record_roundtrips(
        nonce in any::<u64>(),
        height in any::<u64>(),
        flag in any::<bool>(),
        amount in any::<u128>(),
        payload in prop::collection::vec(any::<u8>(), 0..48),
        memo in prop::option::of("[a-z0-9]{0,16}"),
        tag_value in any::<u64>(),
    ) {
        let mut tags = CanonicalMap::new();
        tags.insert("a".to_owned(), tag_value);
        tags.insert("z".to_owned(), tag_value ^ 0xFF);
        let record = CanonicalRecord {
            version: CODEC_VERSION,
            domain: Domain::Tx,
            flag,
            chain: ChainId::stage1(),
            height: Height::new(height),
            nonce: Nonce::new(nonce),
            amount: Amount::from_units(amount),
            hash: Hash::ZERO,
            address: Address::ZERO,
            memo,
            tags,
        };
        let bytes = encode_to_bytes(&record);
        let decoded: CanonicalRecord = decode_canonical(&bytes).unwrap();
        prop_assert_eq!(decoded.clone(), record);
        prop_assert_eq!(encode_to_bytes(&decoded), bytes);
        prop_assert!(payload.len() <= 48);
    }

    #[test]
    fn arbitrary_bytes_never_panic_any_decoder(data in prop::collection::vec(any::<u8>(), 0..192)) {
        for target in FUZZ_TARGETS {
            prop_assert_ne!(fuzz_one(target, &data), FuzzOutcome::Panic);
        }
        let _ = Tx::from_canonical_bytes(&data);
        let _ = StateKey::from_bytes(&data);
        let _ = Genesis::from_canonical_bytes(&data);
    }

    #[test]
    fn state_key_digest_is_a_pure_function(index in any::<u64>()) {
        let mut bytes = [0u8; 20];
        bytes[..8].copy_from_slice(&index.to_le_bytes());
        let key = StateKey::account(&Address::from_bytes(bytes));
        prop_assert_eq!(key.digest(), key.digest());
        prop_assert_eq!(key.as_bytes().len(), 21);
        prop_assert_eq!(StateKey::from_bytes(&key.to_bytes()).unwrap(), key);
    }
}
