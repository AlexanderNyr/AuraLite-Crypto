# cargo-fuzz targets (C9 / H3)

Four targets wrap `aura_testkit::fuzz_one`, the same entry point used by the
portable stable-Rust harness (`make fuzz-smoke`). The `fuzz_rpc` target drives
both HTTP and WebSocket JSON-RPC message parsers against a state-free backend.

```sh
cargo install cargo-fuzz
rustup toolchain install nightly
cargo +nightly fuzz run fuzz_codec -- -max_total_time=60
cargo +nightly fuzz run fuzz_tx -- -max_total_time=60
cargo +nightly fuzz run fuzz_state_key -- -max_total_time=60
cargo +nightly fuzz run fuzz_genesis_json -- -max_total_time=60
cargo +nightly fuzz run fuzz_rpc -- -max_total_time=60
```

C9's measured 60-second gate uses `make fuzz-smoke` (stable, deterministic
PRNG, no sanitizers). H3 still owes the 24-hour libFuzzer campaign with
coverage and crash triage, plus the planned p2p-frame, WASM-verifier, and
wallet-keystore targets. This directory is excluded from the main workspace,
so `libfuzzer-sys` never enters the production `Cargo.lock` or dependency
allowlist.
