#![no_main]

use libfuzzer_sys::fuzz_target;

// H3: the WASM verifier (`Sandbox::gate`: size -> census -> compile) must be
// total — arbitrary bytes are rejected, never panic. Verification only; no
// contract is executed (docs/h3-fuzz-plan.md §2).
fuzz_target!(|data: &[u8]| {
    let sandbox = aura_exec::sandbox::Sandbox::new();
    let _ = sandbox.gate(data);
});
