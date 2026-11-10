#![no_main]

use libfuzzer_sys::fuzz_target;

// Parser-only: this target never runs attacker-selected, expensive KDFs. Seed
// the corpus with public valid scrypt/Argon2id interop fixtures so libFuzzer
// explores valid and malformed documents as well as arbitrary UTF-8. The
// shared driver is unit/mutation tested.
fuzz_target!(|data: &[u8]| {
    aura_fuzz_targets::keystore_parse::check(data);
});
