# P2 — Parallel execution (phase report)

Date: 2026-11-24. Phase patch: `patches/0095-P2-parallel-execution.patch`.
Report digest
`96863c0b96671e12424ec6befc818b1a5028fda01789ad585179424aa3265174`.

## DoD and criteria (PERF_PLAN P2)

| Criterion | Status |
|---|---|
| **1000 txs: parallel result == sequential** | ✅ Deterministic optimistic engine: the final state of the parallel path is byte-for-byte equal to the sequential one (state digest `e5759337…75537`); commit order is strictly index order (serializable) |
| **conflicts roll back honestly** | ✅ Exactly **32** structural cross-chunk conflicts; each: abort → discarding speculative writes (leak diagnostics `P2: conflict not rolled back`) → retry in index order; the balance-preservation invariant; the conflict counter is pinned (counter mutation is red) |
| **optimistic MVCC / Block-STM-like (stretch)** | ✅ The scheme: chunks speculate over a write-overlay (a form of multi-version memory), read-set validation at commit in index order, abort/re-execute (Kung-Robinson OCC; Block-STM: "a fixed in-block order keeps the final state deterministic") |
| **NC: revert → red** | ✅ 6 revert mutations (`tools/nc_p2.py`): conflict detection, rollback, commit order, parallel win (8→1 worker), conflict counter, determinism — **6/6 RED + GREEN** |

## The model and numbers (D-P6)

| | Before (sequential) | After (parallel, 8 workers) |
|---|---|---|
| Block cost 1000 txs | 99 000 units (1000 × 99) | **15 543** = 99 000/8 + 32 × 99 |
| Throughput | baseline | **+536%** (6.37×) |

The retry cost is a full re-execution of the transaction (99 units) —
conflicts are never free (an honest model). 5 repeats, spread
0% (the model is a pure function, X7), the digest is pinned before the run.

### Gates (`P2Error`, 6 named messages)

`P2: parallel result mismatch ({expected}/{actual})` (the DoD gate
of equality, with leak diagnostics), `P2: conflict not rolled back
(tx {n})` (rollback honesty), `P2: retry order violated (tx {n})`
(serializable order), `P2: speedup regression ({before}/{after})`
(the model), `P2: conflict count mismatch ({expected}/{actual})`
(schedule determinism), `P2: determinism violated (run {run})` (X7).

## Honest wall-clock layer (D-P9) — a published negative result

A release measurement on this machine (a block of 1000 tiny transactions ≈ 0.5 ms):

| Path | Block time | Δ vs sequential |
|---|---|---|
| Sequential (baseline) | ~483 µs | — |
| Parallel, cold thread spawn | ~931 µs | **−49%** |
| Parallel, persistent worker pool | ~505 µs | **−5%** |

The modeled +536% is the algorithmic ceiling at zero overhead;
reality on small blocks is different: thread spawn and the sequential
commit phase (deterministic order — a consensus requirement)
dominate. The break-even point is set by block size and the cost of
state operations — the work of P3 (storage). The negative result
is published as-is (D-P3: numbers with methodology; the P6 convention:
"did not work" is a result too).

## The fixture (honest design before the run)

1000 transactions / 2096 accounts (balances 1 000 000):
- **936 independent** transfers `2i → 2i+1` (unique cells, zero
  conflicts) — the scheme's target case ("for independent txs");
- **32 structural cross-chunk conflicts**: transaction pairs at
  indices `4+27k` and `129+27k` (guaranteed adjacent chunks) pull
  from one "hot" account `2000+k` to unique targets. Exactly one
  a transaction from each pair (the later chunk) must roll back —
  32 conflicts structurally, not by chance.

## Sources

- **Block-STM** (arXiv 2203.06871, Aphinyanaphongs et al./Aptos):
  multi-version memory, read-set validation after execution,
  abort/re-execute ("incarnation"), a fixed in-block order →
  a deterministic state compatible with consensus; the scheme
  inherits STM/OCC (Herlihy-Moss, Shavit-Touitou).
- **Kung-Robinson, «On Optimistic Methods for Concurrency Control»
  (1981)**: three phases read/validate/write; validation guarantees
  serializability: for `TN(Ti) < TN(Tj)` it suffices that `WS(Ti) ∩
  RS(Tj) = ∅` or Ti finished its write phase before Tj's reads — exactly what
  our commit cycle checks.
- **Stable v1.4.0 OPE** (docs.stable.xyz): an optimistic parallel
  executor over Block-STM in production: "transactions execute
  concurrently across CPU cores, while a fixed in-block order keeps the
  final state deterministic" — matching our equality gate.
- **Internal**: PERF_PLAN (D-P1..D-P3, D-P6, D-P9), X7 determinism,
  P1-cost (99 units/tx), the "first numbers, even bad ones" convention.

## Phase decisions

- **D-P6**: the P2 engine is a self-contained profiling artifact
  `aura-bench::p2` (native ports along the MVCC map); integrating the
  scheme into `aura-exec` (the WASM engine) — follow-up production work with
  its own review.
- Speculation is a write-overlay over shared base state
  (a form of multi-version memory), without snapshot cloning; thread
  synchronization does not affect the result (a chunk is a pure function of the base):
  real threads in tests are deterministic (X7).

## Negative controls

`tools/nc_p2.py` — 6 revert mutations (each a one-liner):
`bad-parallel` (conflict detection → `if true`), `bad-rollback`
(leaking aborted writes before the retry), `bad-retry-order`
(reversed chunk order), `bad-speedup` (1 worker), `bad-conflicts`
(doubling the counter), `bad-determinism`. **6/6 RED + GREEN.**
Cumulative: **626 NC / 34 phases**.
