#![no_main]

use libfuzzer_sys::fuzz_target;

// Public test-only inputs: tiny work factors keep the differential campaign
// bounded. This does not test or benchmark wallet production KDF policy. The
// shared driver is unit tested and the fork itself is provenance-guarded.
fuzz_target!(|data: &[u8]| {
    aura_fuzz_targets::scrypt_differential::check(data);
});
