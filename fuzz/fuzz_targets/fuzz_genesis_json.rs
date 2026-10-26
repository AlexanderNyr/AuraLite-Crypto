#![no_main]

use libfuzzer_sys::fuzz_target;

// Same entry point as the portable C9 harness: no decoder may panic on
// arbitrary input. Rejection is expected and is NOT a crash.
fuzz_target!(|data: &[u8]| {
    let _ = aura_testkit::fuzz_one("genesis-json", data);
});
