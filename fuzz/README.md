# cargo-fuzz targets (C9 / H3)

Four legacy targets wrap `aura_testkit::fuzz_one`, the same entry point used by the
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
wallet-keystore hardening beyond the short W1 campaigns. This directory is excluded from the main workspace,
so `libfuzzer-sys` never enters the production `Cargo.lock` or dependency
allowlist.

## W1 wallet keystore targets (test-only)

`fuzz_keystore_parse` exercises UTF-8, strict JSON parsing, duplicate keys,
field validation, and parse/serialize/parse invariants. It never executes a
fuzz-supplied KDF. Seed its corpus from the public test fixtures in
`fuzz/corpus/fuzz_keystore_parse/`.

`fuzz_keystore_decrypt` constructs a valid document from a fixed **public test
seed/password**, then exercises 12 structured modes: valid roundtrip; ciphertext
body or tag changes; ID, salt, nonce, public key or address changes; wrong
password; and bounded changes to Argon2 memory, iterations or lanes. Each
invalid mode must return `AuthenticationFailed`, rather than merely any error.
The shared driver in `fuzz/src/lib.rs` has unit tests covering every mode and
its input/KDF boundaries; `tools/nc_w1_fuzz.py` checks that the driver fails
when a wrong-password bypass or AAD-ID omission is injected. These controls test
the oracle, not the cryptosystem's security. The CI-integrated `make test-w1`
and `make nc-w1` run them; never run mutation controls alongside builds.

This target only tests the explicitly weak `insecure-wallet-fuzz` feature and
calls the unsafe test-only encrypt/decrypt APIs. Its baseline Argon2id uses
16 KiB, one iteration and one lane. Although fuzz inputs select some KDF
parameters, each selection is hard bounded: altered memory 17–64 KiB, time
2–3, or lanes 2 with the 16 KiB baseline. It does not accept arbitrary or
production-cost KDF parameters. The ordinary wallet methods retain their KDF
minima with the feature enabled, but the unsafe methods are available even in
release builds and MUST NOT be enabled or used for production. This is not a
production benchmark; it does not independently exercise high-cost KDFs.

`fuzz_scrypt_differential` compares the local wiping fork with the pinned
unpatched upstream scrypt over bounded **test-only** parameters and input
bytes. These parameters are intentionally below wallet production policy;
this target never changes the wallet's accepted KDF parameters.

```sh
cargo +nightly fuzz run fuzz_scrypt_differential -- -max_total_time=60 -max_len=128
cargo +nightly fuzz run fuzz_keystore_parse -- -max_total_time=60 -max_len=8192
cargo +nightly fuzz run fuzz_keystore_decrypt --features insecure-wallet-fuzz -- -max_total_time=60 -max_len=128
```

A short run is a smoke test, not a 24-hour campaign or security audit. Keep
crash artifacts and separately triage reproducible findings. To avoid writing
mutated corpus into the checkout, pass an external corpus directory before
`--`, and configure `-artifact_prefix` to an external writable directory.
