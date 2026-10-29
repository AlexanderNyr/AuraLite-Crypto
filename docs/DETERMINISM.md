# Determinism — what AuraLite-Crypto promises, and how it is measured

**Phase:** X7 · **Gate:** `crates/aura-exec/tests/x7_determinism.rs` · **Status:** measured

> The same ordered input, replayed by independent nodes, produces the same
> `state_root`, the same gas bill and the same events.

This document says what that sentence means, how it is tested, and where it stops.

## 1. Why this is the decisive gate

Every other phase can be right and the chain can still be worthless: if two nodes
execute the same block and get different state roots, there is no chain, only two
databases with the same name. Determinism is therefore not a quality of the code,
it is a **property of the pair (code, input)** — and the only way to know it holds
is to run the input more than once, in places that differ, and compare.

`docs/ABI_V1.md`, `docs/ABI_V2.md`, `docs/ABI_V3.md` and `docs/CALLS.md` describe
the surfaces; this document describes the *machine* underneath them.

## 2. Where nondeterminism could come from

| source | how this chain removes it | where the removal is proven |
|---|---|---|
| floats (`NaN` payloads, rounding) | banned by the census before deploy | X0 (`x0_census`), X8 will re-check |
| host imports (clock, RNG, OS) | a closed list of ABI functions; no WASI, no `random_get` | X1, X2 |
| shared memory / atomics / threads | banned by the census; the engine is single-threaded | X0, X1 |
| map iteration order | every collection that reaches the root is a `BTreeMap` or a sorted `Vec` | S1, S3, X7 |
| process-level randomness (hasher seed, ASLR) | nothing keys on it — and the gate runs eight separate processes to prove it | X7 |
| integer overflow (`debug` wraps/panics, `release` does not) | checked arithmetic in the ledger, and the gate runs a `debug` build against a `release` one | C1, C7, X7 |
| optimizer / codegen order | the gate runs `release` with 1 and with 16 codegen units | X7 |
| engine implementation | one interpreter is the protocol engine; the second backend is a CI job | X1, X7 |

## 3. The measured claim

`make x7-matrix` and the X7 gate replay a single, fully pinned workload:

* **1 004 ordered steps**: 4 deploys, then 1 000 transactions — 60% ABI v1 calls,
  12% ABI v3 calls with a nested frame, 10% native transfers, 3% calls to an
  address with no code, 2% calls that run out of gas, plus the remainder of the
  mix described in `docs/evidence/x7/report.md`;
* the input is a **pure function of one constant** (`detcheck::X7_SEED`): no clock,
  no entropy, no iteration order in generation;
* the artifacts it executes are the **shipped** `kv`, `store`, `events` and
  `vault` bytecode, pinned by BLAKE3 in
  `crates/aura-exec/tests/fixtures/x7_determinism.txt` and re-verified on every run.

The report's numbers — root, gas, events chain, rolling digest and the commit /
refusal / failure split — are recorded in `docs/evidence/x7/`.

## 4. Why eight *processes*

Two runs in one process share Rust's per-process hasher seed, the allocator's
history and the address layout. They can agree while a second node disagrees. So
the gate spawns `x7-run` eight times and compares the eight reports; each process
is a different seed, a different allocator history and a different address space.

The gate also runs in-process, because that is cheap and catches the obvious
regressions; but the *claim* rests on the processes.

## 5. Why three *build configurations*, and not three platforms

The DoD asks for three platforms. Three operating systems cannot be measured from
one sandbox, so this repository measures the three dimensions that are actually
different on one machine and that an OS matrix would vary anyway:

| configuration | what differs |
|---|---|
| `debug` | opt-level 0, integer-overflow checks on, debug assertions on |
| `release` | opt-level 3, overflow checks off — different machine code |
| `release-cgu16` | 16 codegen units instead of 1: different inlining and merging order |

All three agree on the root, the gas total and the digest. **The OS dimension is a
CI job** (`.github/workflows/ci.yml` → `determinism`, three runners). Until that job
has run on all three, "Linux, macOS and Windows agree" is **unproven**, and this
document says so instead of implying it.

## 6. What a divergence would look like

`x7-run` prints a rolling digest that folds every step — its class, status, gas and
the resulting root — so a divergence is localised to the first differing step
rather than reported as "the final roots differ". A node that committed a different
number of steps disagreed about *execution*; a node with the same counters and a
different root disagreed about *hashing*. Those are different bugs, and the report
keeps the counters separate so the difference is visible.

## 7. Rules for anyone changing the machine

1. **Nothing may read a clock, an entropy source or a PID.** Not in the executor,
   not in the state layer, not in a contract's gas path.
2. **A collection that reaches the root must be ordered.** `BTreeMap` or a sorted
   `Vec`; a `HashMap` may be used only where the order cannot be observed.
3. **Arithmetic that touches a balance, a nonce, a version or gas is checked.**
   A `wrapping_*` in a consensus path is a bug, not an optimization.
4. **A new limit is a protocol parameter.** If a bound changes which transactions
   are accepted, it changes the root, so it needs a protocol version and a
   migration (X3's rule, restated for the machine).
5. **A new nondeterminism source needs a new negative control** — a tamper that
   reddens a named test. `tools/nc_x7.py` is where those live.

## 8. Honest limits

* A gate is not a node: no consensus, no mempool, no block structure, no P2P.
* Concurrency is untested because it does not exist (atomics and threads are
  banned); this gate therefore says nothing about a future multi-threaded engine.
* The root includes the version, so the gate fixes the input exactly rather than
  permuting independent blocks; a node that splits blocks differently must compare
  per block.
* Different CPUs are not exercised; making the accepted subset small enough that
  this stops mattering is the static verifier's job (X8).
