//! Reference contract: reads consensus data and emits it (X4).
//!
//! Shows `block_ctx` (height, timestamp, ABI version) and `hash`, and it has no
//! constructor at all — a `run`-only contract, which X3 accepts by design.

#![no_std]

use aura_sdk::prelude::*;

aura_sdk::panic_handler!();

/// Emit the block height as an event and return it.
#[entrypoint]
pub fn run() -> u32 {
    let height = match block_height() {
        Ok(height) => height,
        Err(_) => abort(),
    };
    emit_or_abort(b"height", &height.to_le_bytes());
    // The digest of the height is written to state so the test can prove that
    // `hash` and `storage_write` share one call.
    let digest = match hash(&height.to_le_bytes()) {
        Ok(digest) => digest,
        Err(_) => abort(),
    };
    let slot: Slot<[u8; 32]> = Slot::at(b"digest");
    slot.write(digest);
    height as u32
}
