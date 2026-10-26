# Test rig — C9 specification

**Status:** ✅ implemented in `crates/aura-testkit` + `fuzz/` (2026-10-26).
**Authority:** [`CORE_PLAN.md`](../plans/CORE_PLAN.md) C9 and
[`SECURITY_PLAN.md`](../plans/SECURITY_PLAN.md) H3.

**Scope.** C9 adds the **15th crate**, `aura-testkit`, and makes the C0
placeholders `make test-core` and `make fuzz-smoke` real gates. It adds
`proptest =1.12.0` as a **dev dependency** (13 transitive crates) and four
`libfuzzer-sys` source targets under `fuzz/`.

## 1. What a test rig must prove

A rig that exists but is never run protects nothing. Therefore
`tools/check_rig.py` runs inside `make test` and fails if any required gate
disappears from the Makefile/CI, if `proptest` stops being a dev dependency,
or if a fuzz target is deleted. `tools/count_vectors.py` also runs in
`make test` and fails below `MIN_VECTORS = 200`.

## 2. Golden vectors

`tools/gen_c9_vectors.py` is independent Python (deterministic LCG, fixed
seed) with a C2 minimal-LEB128 transcription. `c9_vectors.txt` has **512
vectors**:

| Prefix | Count | Expected behaviour |
|---|---|---|
| `varint_ok_*` | 128 | encode → exact bytes, decode → exact `u64`, no remainder |
| `varint_bad_*` | 128 | non-minimal or >10-byte overflow → `Err` |
| `trailing_*` | 128 | valid `Vec<u8>` + 1..128 extra bytes → `TrailingBytes` |
| `truncated_*` | 128 | distinct short prefixes of a 256-byte vector → `Err` |

The generator self-checks every valid varint round-trip and every bad varint
rejection before writing. It is offline and byte-stable.

Across all nine fixtures the inventory currently measures **956 vectors**
(9 files, explicit constant/vector classification in `count_vectors.py`).

## 3. Property tests

`crates/aura-testkit/tests/properties.rs` uses `proptest` (256 cases each):

* `varint_is_minimal_and_roundtrips` — any `u64` round-trips, and a padded
  encoding of the same value is rejected;
* `trailing_bytes_are_always_rejected` — extra bytes always `TrailingBytes`;
* `canonical_record_roundtrips` — randomized `CanonicalRecord` fields survive
  `decode_canonical` and re-encode byte-identically;
* `arbitrary_bytes_never_panic_any_decoder` — all six decoders are total on
  arbitrary input;
* `state_key_digest_is_a_pure_function` — C7 key digests are stable.

The C9 NCs inject two **real** bugs — accepting non-minimal varints and
disabling the trailing-byte guard — and require this suite to go red.

## 4. Portable fuzz harness (the measured gate)

`aura_testkit::fuzz_one(target, bytes)` feeds arbitrary bytes to one of six
decoders under `catch_unwind`:

`codec`, `tx`, `state-key`, `genesis-json`, `block`, `merkle-proof`.

`run_portable_fuzz` uses a fixed xorshift64* PRNG, so the campaign is
reproducible with no external randomness. `work` and `iterations` are
counted separately, so a broken iteration counter cannot hang the loop
(NC-5).

```
make fuzz-smoke   # aura-fuzz --seconds 60 --min-iterations 1000 --target all
make test-core    # aura-fuzz --seconds 0  --min-iterations 5000 --target all
```

`--seconds` is the **total** budget split across targets, so `all` at 60 s
really takes 60 s, not 6×60 s. The CLI exits 1 on any panic or zero
iterations. C9's DoD gate is **60 s of fuzzing with zero crashes**.

## 5. libFuzzer targets (H3)

`fuzz/` is a separate cargo-fuzz workspace, excluded from the main workspace
via `exclude = ["fuzz"]`. `libfuzzer-sys` therefore never enters
`Cargo.lock` or the dependency allowlist. Four targets (`fuzz_codec`,
`fuzz_tx`, `fuzz_state_key`, `fuzz_genesis_json`) call the same `fuzz_one`.

They require nightly + `cargo-fuzz` and were **not executed in this sandbox**
(stable toolchain only). The measured C9 gate is the portable harness above;
H3 owes the 24-hour libFuzzer campaign with coverage and crash triage.

## 6. Negative controls

`tools/nc_c9.py` runs 6 NCs: a corrupted golden vector, an impossible
`MIN_VECTORS`, two injected production bugs (varint minimality, trailing-byte
guard), a zeroed fuzz iteration counter and an unwired Makefile. Total across
C0–C9: **50 NC**.

## 7. Non-goals at C9

Miri/ASan/UBSan (H3), 24-hour libFuzzer campaign and crash triage (H3),
state-root determinism across execution (X7), property tests for the VM and
consensus (they do not exist yet), coverage thresholds in CI (O0), and
`cargo deny` (H1).
