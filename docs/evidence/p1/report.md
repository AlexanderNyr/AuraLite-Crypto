# P1 Report — Codec/crypto

Date: 2026-11-24

The result: **ALL P1 GATES GREEN**

The DoD "+X% verify/s with the same determinism" is met: the model gives
**+72% verify/s** (38 -> 22 cost units per signature at batch k=8, the same
D-P1 workload, the same determinism — 5 repeats with spread 0%,
the digest `340a884d…80241` pinned before the run); zero-copy decode
25 -> 12 units; the BLS model FastAggregateVerify 60 versus 352
(5.9x, MODEL ONLY — D-P4). The wall-clock layer (evidence only): the real
ed25519-dalek batch **+29% verify/s** at k=8 on this machine (D-P9).

Three optimizations, each with before/after (D-P2): **batch signature
verification** — the real `aura_crypto::verify_batch_raw` code (a single
Merlin batch equation) with strict single-verification semantics
(S canonicality — H2; first-bad-index diagnostics);
**zero-copy decode** — `aura_codec::transfer`: two independent
decoders of one canonical 350-byte wire block (a borrowed view without
allocations versus a copying baseline) with an equality gate and rejection of
4 malformed cases; **BLS aggregation** — an honest cost model per
draft-irtf-cfrg-bls-signature (pairing computations are not implemented — D-P4).

12 tests: golden numbers (the literal DoD +72%), 6 named messages
(`P1: …`), functional gates (batch==sequential, including
S+L malleation; zero-copy==copy; malformed ×4), "revert → red" for
each optimization, metamorphic properties (model purity, scaling of
runs, wire-form canonicality). Example ↔ `tools/p1_crypto_check.py`
byte-for-byte (`P1-RUNS-OK 5` / `P1-CRYPTO-OK {…}`). Negative controls
`tools/nc_p1.py` — 6 revert mutations (`nc.log`, RESULT: 6/6 negative
controls PASS). Sources: ed25519-dalek (batch >2x on large
batches), draft-irtf-cfrg-bls-signature (n+1 versus 2n pairing),
zero-copy literature (the serde/rkyv/zerocopy class) — see
`docs/perf/p1-crypto-report.md`. Decisions D-P4/D-P5.
