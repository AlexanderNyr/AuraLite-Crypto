#![no_main]

use libfuzzer_sys::fuzz_target;

// H3: p2p frame decoders must be total — no panic on arbitrary bytes.
// Rejection is expected and is NOT a crash. Same contract as the portable
// C9 harness (docs/h3-fuzz-plan.md §2).
fuzz_target!(|data: &[u8]| {
    let _ = aura_p2p::transport::HandshakeHello::decode(data);
    let _ = aura_p2p::gossipsub::GossipMessage::decode(data);
});
