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
        // The authenticated normalization-profile marker survives parsing and
        // serialization unchanged (including its absence).
        assert_eq!(
            store.normalization_profile(),
            reparsed.normalization_profile()
        );

        // Strict-parser metamorphic rejects derived from this accepted
        // document: one extra field, a duplicate key spelled with an escape
        // (`\u0066ormat` is `format`), and one byte over the size limit.
        let body = serialized.strip_prefix('{').expect("object document");
        let extra_field = format!("{{\"extra-field\":0,{body}");
        assert!(Keystore::from_json_str(&extra_field).is_err());
        let escaped_duplicate = format!("{{\"\\u0066ormat\":null,{body}");
        assert!(Keystore::from_json_str(&escaped_duplicate).is_err());
        let mut oversize = serialized;
        oversize.push_str(&" ".repeat((MAX_KEYSTORE_BYTES as usize + 1) - oversize.len()));
        assert_eq!(oversize.len(), MAX_KEYSTORE_BYTES as usize + 1);
        assert!(Keystore::from_json_str(&oversize).is_err());
    }
});
