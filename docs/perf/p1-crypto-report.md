# P1 — Codec/crypto (phase report)

Date: 2026-11-24. Phase patch: `patches/0094-P1-codec-crypto.patch`.
Report digest
`340a884d17a5ca58111c00fcd55229f7cfce4175fcfb8c97cf69fe2f52c80241`.

## DoD and criteria (PERF_PLAN P1)

| Criterion | Status |
|---|---|
| **+X% verify/s with the same determinism** | ✅ Model: 38 → **22** cost units per signature (batch k=8) = **+72% verify/s**; zero-copy decode 25 → 12 units (−52%); deterministic cost of the repeats byte-for-byte (spread 0%, digest pinned BEFORE the run). Wall-clock layer (evidence): the real ed25519-dalek batch gives **+29% verify/s** at k=8 in this sandbox — an honest "first number" (D-P9) |
| **batch signature verification** | ✅ Real code: `aura_crypto::verify_batch_raw` (one Merlin batch equation over `ed25519_dalek::verify_batch`) + strict single-verification semantics (`verify_raw`: S-canonicity — H2). Gate: batch == sequential verification on every element |
| **BLS aggregation** | ⚠️ **COST MODEL** (D-P4, honestly): `FastAggregateVerify(k) = 2·22 + 2·k` = 60 units for k=8 versus 352 individually (5.9×). The pairing computations themselves are NOT implemented — an external pairing-curve library and a separate review stage are required; the gate pins the model arithmetic and its regression |
| **zero-copy decode** | ✅ Real code: `aura_codec::transfer` — two independent decoders of one canonical 350-byte wire block: `decode_zero_copy` (borrowed view, zero allocations) and `decode_copy` (the baseline, copying one). Gate: field-by-field equality + joint rejection of 4 malformed cases |
| **NC: revert → red** | ✅ 6 revert mutations in `tools/nc_p1.py`: each reverts one optimization aspect → red with a named message. **6/6 RED + GREEN** |

## The model: three optimizations, each before/after (D-P2)

| Optimization | Before | After | Δ |
|---|---|---|---|
| Batch verify (k=8, cost units/signature) | 38 | **22** (`(16+20k)/k`) | **+72% verify/s** |
| Zero-copy decode (350 B, units) | 25 (`4+b/16`) | **12** (`2+b/32`) | −52% |
| BLS FastAggregate (k=8, units) | 352 (8×CoreVerify) | **60** (`44+2k`) | 5.9× (model) |

Per-run pipeline cost (the same D-P1 load: transfer 350 B, 1/1):
12 + 22 + 25 + 40 = **99** units versus the P0 baseline 128 (−22.7%);
5 repeats → total **495**, p50/p95/p99 = 99, spread 0%.

### The gate (`P1Report::validate`, fixed order)

determinism (all repeats equal) → verify speedup (model ≥ +72%,
the decode pair improved too) → BLS aggregate (strictly cheaper than
individual verification). 6 named messages (`P1: …`) — exact
to_string asserts in the tests; each is an NC-revert target.

### Functional gates (not just numbers)

- **Batch == sequential**: a valid batch, a batch with a broken
  signature (both paths reject, the index matches), the S+L twin
  (malleation, H2) — rejected by the batch **and** the sequential
  verifier identically (NonCanonicalSignature).
- **Zero-copy == copy**: field-by-field equality + stable
  encode→decode (one value — one encoding).
- **Malformed ×4** (`short-buffer`, `overlong`, `bad-version`,
  `bad-memo-padding`): both decoders reject; wire-block canonicity
  (length, version, memo_len, zero padding) — is part of the contract.

## Methodology and sources

- **ed25519-dalek `verify_batch`** (github.com/dalek-cryptography):
  batch verification "greatly improved performance" — a batch of 96
  signatures = 18.4 µs/signature versus ~45 µs single (>2×);
  the Merlin transcript protects the batch equation from malleation
  (s is in the transcript). Phase measurement: +29% at k=8, the effect grows with
  batch size (amortizing the setup part `16+20k`).
- **BLS** (draft-irtf-cfrg-bls-signature): an aggregate is verified
  with n+1 pairing operations versus 2n separately;
  `FastAggregateVerify` — key summation + one CoreVerify
  (2 pairings) — hence the model `2·22 + 2·k`.
- **Zero-copy** (rust-patterns-book ch11, epserde, softwarepatterns
  lexicon): borrowing deserialization without allocations — the key to
  fast parsing; "profile first" — that is why the measured layer
  honestly shows both sides (and that the decode win in absolute
  ns is small: 33 → 32 ns in debug, 2 → 1 ns in release — the optimization
  matters on long blocks and under allocator pressure).
- **Internal**: PERF_PLAN (D-P1..D-P3, D-P9/D-P9b, the risk "do not
  optimize before profiling"), X7 determinism, H2 S-canonicity.

## Phase decisions

- **D-P4**: BLS aggregation is declared a **cost model + API contract**,
  not a cryptographic implementation; production pairing —
  a separate phase with an external dependency and a review. This is more honest than
  a toy "implementation".
- **D-P5**: batch verification preserves the strict semantics of the single one
  (S-canonicity per element, first-bad-index diagnostics);
  goldens and the bench plan are fixed before the first run (D-P9).

## Negative controls

`tools/nc_p1.py` — 6 revert mutations: `bad-batch` (the index contract of
the batch), `bad-zc` (substituting the from slot), `bad-malformed` (dropping the
version check), `bad-verify-cost` (reverting the batch coefficient 20→36:
the cost returns to 38), `bad-agg-cost` (the BLS model back to
individual cost), `bad-determinism` (a differing run).
**6/6 RED + GREEN.** Cumulative: **620 NC / 33 phases**.
