# X7 — the determinism gate (measured)

Generated (UTC): 2026-10-29T10:27:54+00:00. Every number below is either pinned in
`crates/aura-exec/tests/fixtures/x7_determinism.txt` (written by
`tools/gen_x7_vectors.py` from `x7-run --fixture`) or was produced by the
eight-process run recorded in the same command list as `gates.txt`.

## What the gate claims

> The same ordered input, replayed by independent nodes, produces the same
> `state_root`, the same gas bill and the same events.

Everything else here is detail. The input is one constant: [`X7_SEED`]
seeds SplitMix64, which draws the whole workload — no clock, no `/dev/urandom`,
no iterator order anywhere in generation.

## The workload (pinned)

| | |
|---|---|
| steps | 1004 (4 deploys + 1 000 transactions) |
| contract calls | 905 |
| native transfers | 95 |
| accounts | 8 |
| gas limit (ordinary call) | 1,000,000 |
| gas limit (starved call) | 4,000 |
| block height / timestamp | 1000 / 1700000000000 (fixed, never a clock) |

### What the mix exercises, and why each part is there

* **ABI v1 calls (kv, events)** — storage writes, events, and `block_ctx` /
  `hash`, which are the two host calls where a leak would show first.
* **Native transfers** — the path with no VM at all, so the gate also covers
  the ledger arithmetic and the nonce bump a node performs itself.
* **ABI v3 calls (vault)** — 104 of them, each
  opening a nested frame (a withdrawal always notifies its receiver), so the run
  also crosses the X6 call surface that many times.
* **Calls to an address with no code** — a step that must *fail* the same way
  on every node; nodes that disagreed about which transactions were accepted
  would disagree about the root.
* **Starved calls** — out of gas: billed, and committed nothing.

## Pinned artifacts

| contract | bytes | blake3 |
|---|---|---|
| `kv` | 5290 | `27e47b71bc8977e5…` |
| `store` | 5187 | `4daa34dab8574d99…` |
| `events` | 5033 | `f968f280eb9ccec9…` |
| `vault` | 9759 | `11f8fb2cce7aacff…` |

The gate re-verifies these bytes on every run: a determinism claim about
bytecode nobody can name is not a claim.

## The measurement

| | |
|---|---|
| committed | 890 |
| refused (returned non-zero, wrote nothing) | 60 |
| failed (trap / out of gas / no code) | 54 |
| gas total | 46,375,290 |
| version | 890 |
| `state_root` | `efca4222d37347ffbc3416b4bbef06eaa02fb930115c6c611a4351fad77312cb` |
| `tree_root` | `813b06ee47a144ca646fb4368823420cf03965d5a4d8af4ed61463e0d89d13f0` |
| events chain | `8b8e9255ec30e3c51571655514ffa21212fad98f1e4685aba9732ef89888eb60` |
| rolling digest | `91b733922f3ee3c60f603ae41978f15b6be93e3e3c6f25a49851da22afbf9daf` |

Measured in this tree just now: **all eight processes agree**
on the root, the gas bill and the digest.

| node | state root (first 16) | gas | digest (first 16) |
|---|---|---|---|
| 0 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |
| 1 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |
| 2 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |
| 3 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |
| 4 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |
| 5 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |
| 6 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |
| 7 | `efca4222d37347ff…` | 46,375,290 | `91b733922f3ee3c6…` |

## Three build configurations, not three operating systems

The DoD says "3 platforms". Three operating systems cannot be measured from
one sandbox, and claiming them would be exactly the kind of statement this
repository refuses to make. What `make x7-matrix` measures here is three
**build configurations**, each in two separate processes:

| configuration | why it is a different machine |
|---|---|
| `debug` | opt-level 0, integer-overflow checks on, debug assertions on |
| `release` | opt-level 3, overflow checks off — different machine code entirely |
| `release-cgu16` | 16 codegen units instead of 1: different inlining and merging order |

All three agree on the root, the gas total and the digest (see `gates.txt`,
`make x7-matrix`). The **OS** dimension is a CI job —
`.github/workflows/ci.yml` → `determinism`, three runners — and until that job
has actually run, `linux/macOS/windows` agreement is **unproven** and this
report says so rather than implying otherwise.

## What the gate cannot see

* **A nondeterminism that needs two different operating systems.** Covered
  only by CI, as above.
* **A nondeterminism that needs real concurrency.** The executor is
  single-threaded by design (X1: atomics and shared memory are banned, threads
  do not exist), so there is no race to detect — but that also means this gate
  proves nothing about a future multi-threaded engine.
* **A nondeterminism that needs a different CPU.** `release` here targets the
  same host. The X8 verifier's job is to make the *subset* small enough that
  this stops mattering.

## Negative controls

Six tampers, each reddening a *named* test: the version stamp dropped from the
root, an empty diff counted as a commit, the digest ignoring the resulting
root, a refusal applying its diff, a starved call billed nothing, and the
generator ignoring its seed. A green baseline runs first, so a red caused by a
broken tree can never be mistaken for a successful control.

6/6 PASS; total **153** across C0–C10 + S0–S7 + X0–X7.

## Defects the gate found (all fixed in this phase)

1. **The scenario's deployer was not a funded account.** The first run died
   with `InsufficientBalance` on the fourth deploy: the deployer constant had
   the index byte `0xa0` where [`account_address(0)`] has `0x00`. A one-byte
   mistake that only a real run can find, and the reason the gate now asserts
   the deployer *is* the first funded account at the top of [`genesis`].
2. **Passing an argument to `run()` was a lifecycle error.** 33% of the
   workload hit `store`/`events`, whose `run` takes no argument; X3 refuses an
   arity mismatch, so a third of the "calls" never ran. The scenario now
   carries `Option<i32>` and the mix measures 890 commits instead of 559.
3. **The vault's `withdraw` was called with an address in front of its
   amount.** `withdraw(u128)` decodes the amount first, so every withdrawal
   refused with `-21` — a legitimate refusal, but it meant the ABI v3 branch of
   the workload tested nothing. Both selectors now carry exactly their own
   arguments.
4. **The first NC was green.** Dropping the step index from the digest did not
   redden the order-sensitivity test, because the digest also folds the
   resulting root and the roots differ between orders. The control was
   rewritten to remove the *root* from the fold, which is the mistake that
   actually destroys order sensitivity — and it is recorded here rather than
   quietly fixed.

## Honest scope

* **This is a gate, not a node.** No consensus, no mempool, no block structure,
  no P2P. It replays a fixed transaction list through the X3 lifecycle and the
  S3 overlay — the same code an actual node will drive, with the parts a node
  adds still missing.
* **The root includes the version**, so the number of commits matters: the
  gate therefore fixes the input exactly instead of permuting independent
  blocks. A future phase that lets nodes split blocks differently must compare
  roots per block, not per chain.
* **`refused` and `failed` are counted separately on purpose.** Two nodes that
  disagreed about which transactions were accepted would produce different
  roots; counting them makes the report attribute a divergence instead of
  merely reporting one.
* **Wall-clock cost in a debug build is ~70 s** for the eight-process gate
  (measured), which is why the unit-level assertions use a 120–200 step
  scenario and the full 1 004 steps are reserved for the pinned measurement and
  the multi-process comparison.

Tests: **322** in the workspace, 11 of them in the X7 gate.
Validators / blocks / transactions remain **0 / 0 / 0**.
