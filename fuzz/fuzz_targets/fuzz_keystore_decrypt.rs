#![no_main]

use libfuzzer_sys::fuzz_target;

// The shared driver uses only a fixed public seed/password and bounded,
// intentionally weak test-only KDF parameters. It is also unit/mutation tested.
fuzz_target!(|data: &[u8]| {
    aura_fuzz_targets::keystore_decrypt::check(data);
});
