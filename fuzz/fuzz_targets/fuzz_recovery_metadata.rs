#![no_main]

use libfuzzer_sys::fuzz_target;

// Public, parser-only inputs; never include a mnemonic or password. Exercise
// both bounded and oversized JSON, and require canonical round-trip stability.
// The shared driver is unit/mutation tested.
fuzz_target!(|data: &[u8]| {
    aura_fuzz_targets::recovery_metadata::check(data);
});
