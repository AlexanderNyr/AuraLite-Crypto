# B2 baseline — QC / BLS aggregation, measured

Generated (UTC): 2026-10-29T19:39:36+00:00. Every number below came out of `aura-consensus` in this
tree; `gates.txt` holds the raw command output, and
`crates/aura-consensus/tests/fixtures/b2_baseline.txt` pins the same survey so it
cannot drift silently.

## What this phase claims

> B2 lands QC / BLS aggregation: the [`bls`] module (BLS12-381 threshold
> signatures) and the [`qc`] module (quorum certificate aggregation from
> 2f+1 votes). Three consensus capabilities remain missing and named:
> finality (B3), validator-set (B4), vrf-leader (B5).

## The binary's contract

| invocation | exit | what it does |
|---|---|---|
| `aura-consensus` (no arguments) | 2 | prints the missing capabilities, one phase each, and refuses |
| `aura-consensus report [--workspace PATH]` | 0 | measures the tree and prints what exists |
| `aura-consensus report --fixture` | 0 | the same measurement as `key=value` lines (the pin) |
| `aura-consensus report --workspace <not a workspace>` | 3 | refuses: there is nothing to measure |
| `aura-consensus <anything else>` | 64 | usage error, with the usage text |
| `aura-consensus --help` / `--version` | 0 | documented commands, version names the phase |

## The survey (measured now)

| layer | owner | presence |
|---|---|---|
| `proposer` | B1 | implemented |
| `block-signature` | B1 | implemented |
| `qc` | B2 | implemented |
| `finality` | B3 | implemented |
| `validator-set` | B4 | placeholder |
| `vrf-leader` | B5 | placeholder |

**4 implemented, 2 placeholders, 0 missing.**
proposer, block-signature, and qc measure as implemented; finality,
validator-set, and vrf-leader measure as placeholders.

## The three capabilities B3-B5 will add; B2's qc is no longer missing

| capability | phase | what it will do |
|---|---|---|
| `validator-set` | B4 | staking contract, epoch, enter/exit queue, set update |
| `vrf-leader` | B5 | stake-weighted VRF leader election, grinding protection |

The `what` column lives in `MISSING_CAPABILITIES` and is printed by the binary;
the gate asserts that the refusal text contains every capability, its phase and
its description, so the message cannot rot into a generic \"not implemented\".

## What M0 still needs after B2

* **B3.** BFT pipeline: 3-chain commit, view change, timeouts, nil blocks.
* **B4.** Validator set: staking contract, epoch, enter/exit queue, set update.
* **B5.** VRF leader election.
* **N1–N2.** Transport and gossipsub (NET_PLAN) — required before consensus
  can talk to other nodes.
* **D2–D7.** Node pipeline, mempool, RPC, metrics, devnet, CLI (NODE_PLAN).

## What B2 deliberately does not do

* **It does not implement finality.** No 3-chain commit, no view change,
  no timeouts — those land at B3.
* **It does not manage validators.** No staking contract, no epoch, no
  enter/exit queue — those land at B4.
* **It does not elect leaders by VRF.** That lands at B5.
* **It does not order transactions.** At B2, two different tx logs are
  not equivalent — there is no full consensus to order them.
* **It does not claim correctness.** The survey counts files and public
  items; whether QC aggregation is correct is what B2's gates measure.

## Honest limits

* **The survey is a static count, not an audit.** A crate with one public
  item measures the same as a crate with two hundred and one.
* **`Cargo.toml` membership is trusted.** The survey looks for
  `crates/aura-consensus/src`.
* **The baseline will go stale on purpose.** When B3 lands finality,
  this gate fails until the plan and the fixture are updated in the same
  patch — that is the mechanism, not a defect.

## Negative controls

Six tampers, each reddening a *named* test: the baseline exits 0, every
layer is classified `implemented`, `report` measures a non-workspace, an
unknown command falls through to exit 0, the refusal stops naming the
capabilities, and two different logs are treated as equivalent. A green
baseline runs first, so a red caused by a broken tree can never be
mistaken for a successful control.

6/6 PASS; total **189** across C0–C10 + S0–S7 + X0–X8 + D0 + D1 + B0–B2.

Tests: **453** in the workspace, 56 of them in the B2 gate.
Validators / blocks / transactions remain **0 / 0 / 0**.
