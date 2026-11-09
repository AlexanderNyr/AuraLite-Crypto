#![no_main]

use aura_wallet::keystore::RecoveryMetadata;
use libfuzzer_sys::fuzz_target;

// Public, parser-only inputs; never include a mnemonic or password. Exercise
// both bounded and oversized JSON, and require canonical round-trip stability.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(marker) = RecoveryMetadata::from_json_str(text) {
        assert!(data.len() <= 4096, "oversized marker accepted");
        let canonical = marker.to_json_string().expect("marker serializes");
        let reparsed =
            RecoveryMetadata::from_json_str(&canonical).expect("canonical marker parses");
        assert_eq!(marker, reparsed);
        assert_eq!(
            canonical,
            reparsed.to_json_string().expect("stable serialization")
        );
        // Metamorphic invalid inputs constructed from a known-good marker:
        // a duplicate key, an unknown member, and a document over the byte cap.
        let prefix = canonical.strip_suffix('}').expect("object serialization");
        let duplicate = format!("{prefix},\"normalization_profile\":null}}");
        let extra = format!("{prefix},\"unexpected\":null}}");
        let oversized = format!("{canonical}{}", " ".repeat(4097 - canonical.len()));
        for invalid in [&duplicate, &extra, &oversized] {
            assert!(
                RecoveryMetadata::from_json_str(invalid).is_err(),
                "invalid marker accepted"
            );
        }
    }
});
