#![no_main]

use aura_wallet::keystore::{Keystore, MAX_KEYSTORE_BYTES};
use libfuzzer_sys::fuzz_target;

// Parser-only: this target never runs attacker-selected, expensive KDFs. Seed
// the corpus with public valid scrypt/Argon2id interop fixtures so libFuzzer
// explores valid and malformed documents as well as arbitrary UTF-8.
fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_KEYSTORE_BYTES as usize {
        return;
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(store) = Keystore::from_json_str(text) {
        let serialized = store.to_json_string().expect("valid document serializes");
        let reparsed = Keystore::from_json_str(&serialized).expect("serialized document parses");
        assert_eq!(
            serialized,
            reparsed.to_json_string().expect("stable serialization")
        );
    }
});
