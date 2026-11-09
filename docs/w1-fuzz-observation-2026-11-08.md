# W1 bounded fuzz observation — 2026-11-08

**WIP local evidence, not a W1 phase receipt, a 24-hour H3 campaign, or a
security audit.** Three sequential libFuzzer/AddressSanitizer runs completed
without a crash in this environment. They cannot establish absence of bugs.
The decrypt target deliberately uses weak test-only KDF settings; the scrypt
differential target uses tiny test-only work factors. Neither tests the cost or
security of production KDF parameters.

Environment: Linux x86_64, `rustc 1.101.0-nightly (1c3a65069 2026-11-02)`,
`cargo-fuzz 0.13.2`, `CARGO_BUILD_JOBS=1`; source base `c749fc0` with the W1
work-in-progress changes described in the current tree. Runs used separate copies
of the repository seed corpora, so they did not mutate committed corpus files.
The table reports libFuzzer's executed duration, not compilation time.

| Target | `max_len` | Seed shown in log | Iterations | Duration | libFuzzer coverage counters / features | Peak reported RSS | Exit |
|---|---:|---:|---:|---:|---:|---:|---:|
| `fuzz_keystore_parse` | 8192 | 3085498976 | 7,236,019 | 181 s | 1549 / 6802 | 520 MiB | 0 |
| `fuzz_keystore_decrypt` | 128 | 3947032530 | 149,207 | 181 s | 2403 / 2718 | 451 MiB | 0 |
| `fuzz_scrypt_differential` | 128 | 2073809815 | 79,010 | 181 s | 313 / 674 | 395 MiB | 0 |

All used `-max_total_time=180 -timeout=30 -rss_limit_mb=1200`; the decrypt
run additionally used `--features insecure-wallet-fuzz`. Each target's artifact
directory was empty. Coverage counters/features are instrumentation-dependent,
not percentage coverage. Logged seeds alone do not guarantee identical runs:
execution speed, corpus ordering and fuzzing evolution can change the results.

Original local output and evolved corpora are under
`/home/user/w1-fuzz-results/<target>/{run.log,corpus,artifacts}`. The three
`run.log` files had these SHA-256 hashes:

```text
54e443d2712c9144f8b8425fb0a9f509b1c0eac6a8a0bb6a111036d7eacda5ec  fuzz_keystore_parse/run.log
11d903b76422551745271b7489be9b3d760e041e41185da96a8c57236a3beba6  fuzz_keystore_decrypt/run.log
90cada1287022593dbbb8f51cd7ec9f6a3d074557e27102b3a0b8d0c448b6be0  fuzz_scrypt_differential/run.log
```

Selected source and lockfile hashes at observation time (not a complete source
manifest):

```text
c9680cc436d806fa7fa2ce8545bd148dc9fcc9ae444973d4968a3f4f4fc069dc  Cargo.lock
efee48a5d312b5856c86bee4ff797e034047156e1ec0328ec2f56a5ac5191a66  fuzz/Cargo.lock
a6fb486e5e815e8f3b367d097bae575c0017158c86cfee4edddf4dc1d1dd671a  crates/aura-wallet/src/keystore.rs
e59b6c719381654394d09ab38be74d88dd2614e03d0ff32e80382d3c5ac35311  crates/aura-wallet/src/derivation.rs
a33e0ae936dccf9168778f446ecee7120d9fa3a51df3ace31775cb183ea1ca27  fuzz/src/lib.rs
4bc2c7a02bc8008a96702cf3a2086e348c4865ce4ac81e2dc9564ccac1fd0369  fuzz/fuzz_targets/fuzz_keystore_parse.rs
724f15093e68bb8f9a187088e388822aeaa423d8867b722e2510d8558cfeb391  fuzz/fuzz_targets/fuzz_keystore_decrypt.rs
b8471b8dd6e4683075773093325b975668f2db2c904d1c11e209a92d9c933462  fuzz/fuzz_targets/fuzz_scrypt_differential.rs
```

To repeat a comparable run, install the pinned nightly and cargo-fuzz, copy each
`fuzz/corpus/<target>` to a new scratch directory, then run from the repository:

```sh
cargo +nightly-2026-11-03 fuzz run fuzz_keystore_parse SCRATCH_PARSE_CORPUS -- -max_total_time=180 -max_len=8192 -timeout=30 -rss_limit_mb=1200 -artifact_prefix=SCRATCH_PARSE_ARTIFACTS/
cargo +nightly-2026-11-03 fuzz run fuzz_keystore_decrypt --features insecure-wallet-fuzz SCRATCH_DECRYPT_CORPUS -- -max_total_time=180 -max_len=128 -timeout=30 -rss_limit_mb=1200 -artifact_prefix=SCRATCH_DECRYPT_ARTIFACTS/
cargo +nightly-2026-11-03 fuzz run fuzz_scrypt_differential SCRATCH_SCRYPT_CORPUS -- -max_total_time=180 -max_len=128 -timeout=30 -rss_limit_mb=1200 -artifact_prefix=SCRATCH_SCRYPT_ARTIFACTS/
```

Replace the scratch paths with existing separate directories. Do not run
negative controls while building or fuzzing: their scripts temporarily mutate
source. Fuzzing seeds and observations are not a replacement for sustained
campaigns, independent review, or end-to-end production testing.
