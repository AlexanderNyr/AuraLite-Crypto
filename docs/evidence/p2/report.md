# P2 Report — Parallel execution

Date: 2026-11-24

The result: **ALL P2 GATES GREEN**

The DoD is met literally: **1000 txs: parallel result == sequential;
conflicts roll back honestly**. The deterministic optimistic engine
(Block-STM form: read sets, validation after execution, abort +
re-execute, a fixed commit order in index order —
Kung-Robinson OCC) produces a final state that is byte-for-byte equal to
sequential execution (state digest `e5759337…75537`), and exactly
32 structural cross-chunk conflicts — each rolls back honestly
(speculative writes are discarded, the retry is in index order,
the balance-preservation invariant holds).

The cost model (D-P6): seq 99 000 → par 15 543 units (+536% of
throughput; a conflict retry costs a full re-execution —
conflicts are never free). 5 repeats with spread 0%, the report digest
`96863c0b…65174` is pinned before the run.

**The honest wall-clock layer (D-P9, evidence only)**: on this machine
(release, 1000 tiny transactions ~0.5 ms per block) the parallel
path is **slower**: −49% with cold thread spawn and −5% in
steady state with a persistent worker pool. The cause is measured honestly:
thread spawn and the sequential commit phase dominate on small
blocks; the modeled gain is the algorithm's upper bound, which
opens up on large blocks/cheap state operations (the work of P3).
The negative result is published as is (the P6 convention).

12 tests: golden numbers, 6 named messages (`P2: …`), functional
gates (parallel==sequential, honest rollback with leak diagnostics,
commit order, exactly 32 structural conflicts, engine determinism
under real threads, scaling of 2000 independent txs → zero
conflicts, balance preservation, canonicality of the state form).
Example ↔ `tools/p2_parallel_check.py` byte-for-byte. NC `tools/nc_p2.py`
— 6 revert mutations (6/6 RED+GREEN).

Sources: Block-STM (arXiv 2203.06871 — multi-version memory,
read-set validation, abort/re-execute (incarnation), a fixed
in-block order → deterministic state for consensus);
Kung-Robinson OCC 1981 (read/validate/write, serializability);
Stable v1.4.0 OPE (an optimistic parallel executor on top of
Block-STM, determinism across repeated executions). Decisions D-P6
(the engine is a profiling artifact of the bench crate; integration into
aura-exec — follow-up production work) and D-P9.

## Negative controls

`tools/nc_p2.py` — 6 revert mutations: `bad-parallel` (removing
conflict detection), `bad-rollback` (leaking rolled-back writes),
`bad-retry-order` (reversed commit order), `bad-speedup`
(1 worker instead of 8), `bad-conflicts` (doubling the conflict counter),
`bad-determinism`. **6/6 RED + GREEN.** Cumulative **626 NC / 34
phases**.
