# cargo-fuzz targets (C9 / H3)

These four targets wrap `aura_testkit::fuzz_one`, the same entry point used by
the portable stable-Rust harness (`make fuzz-smoke`).

```sh
cargo install cargo-fuzz
rustup toolchain install nightly
cargo +nightly fuzz run fuzz_codec -- -max_total_time=60
cargo +nightly fuzz run fuzz_tx -- -max_total_time=60
cargo +nightly fuzz run fuzz_state_key -- -max_total_time=60
cargo +nightly fuzz run fuzz_genesis_json -- -max_total_time=60
```

C9's measured 60-second gate uses `make fuzz-smoke` (stable, deterministic
PRNG, no sanitizers). H3 owes the 24-hour libFuzzer campaign with coverage
and crash triage. This directory is excluded from the main workspace, so
`libfuzzer-sys` never enters `Cargo.lock` or the dependency allowlist.
