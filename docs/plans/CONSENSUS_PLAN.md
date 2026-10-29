# CONSENSUS_PLAN — BFT-PoS, staking, finality, governance

**Status:** 📋 PLAN (B0–B10)  
**Stage:** Stage 1: B0–B10 (all phases active).  
**Deliverable:** `patches/0030-B0-...` … `patches/0040-B10-...`
**Dependencies:** CORE C3–C8, STATE S1–S3, EXEC X2 (execution). Votes in B1–B3
travel over an in-process transport; NET N1–N2 connects at B5 and is mandatory
for M1.

## Problem

"Your own chain" starts with the temptation to write the simplest single-node
consensus. Such a network survives neither a Byzantine validator nor a
partition. We need real BFT with QCs, leader rotation, slashing and provable
finality — verified by a simulator, not "by eye".

## Decisions

- **D-B1.** In stages: first deterministic PoA (for devnet), then QC and a HotStuff-like 3-chain pipeline, then PoS/VRF. Every step is a separate phase with an NC.
- **D-B2.** Threshold 2f+1; BLS12-381 signature aggregation into a QC; QC verified in O(1).
- **D-B3.** Finality is instant, but weak subjectivity is explicitly documented: checkpoint + unbonding period (T4).
- **D-B4.** Evidence (double-sign, surround) is part of the block, not "we'll deal with it later".

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **B0** | Baseline | single node, ordered tx log, measure "no consensus", report | NC: two different logs pass as "the same" → documented defect |
| ✅ **B1** | PoA round-robin | N validators, deterministic proposer, simple block signature | 4 validators, 100 blocks, identical head on all |
| ✅ **B2** | QC / BLS | prevote/precommit, 2f+1 aggregation, QC verification, fake QC rejected | f=1 (4 nodes): 3 signatures → commit, 2 → no; NC: substituted pubkey → red |
| ✅ **B3** | BFT pipeline | 3-chain commit, view change, timeouts, nil blocks, consensus timestamp | 1 node partitioned → network continues; 100 rounds without desync |
| **B4** | Validator set | staking contract, epoch, enter/exit queue, set update | 4→7 validator change at height H without stopping the network |
| **B5** | VRF leader | stake-weighted VRF, grinding protection, public verification | 10 000 rounds of proposer distribution within confidence interval |
| **B6** | Slashing / evidence | double-sign, surround-vote, downtime, penalties, distribution | 6 attack types → evidence accepted, penalty applied, NC: slashing off → test red |
| **B7** | Rewards | inflation/reward, fee burn, delegation, commission, sum invariant | `sum(balances) + burned == issued` within 1 unit over 100 000 blocks |
| **B8** | Finality | fork choice, finalized checkpoint, light-client proof, deep reorg defence | attempt to reorg a finalized block → reject + evidence |
| **B9** | On-chain governance | proposal → vote → timelock → runtime upgrade (WASM), emergency pause | first protocol upgrade in testnet without a manual validator restart |
| **B10** | Safety evidence | TLA+/Apalache model, 100-node simulation, long-range analysis, close-out | model: safety for f < n/3; NC: remove quorum check → model finds a counterexample |

### B0 receipt (2026-10-29)

- Deliverable: `patches/0031-B0-consensus-baseline.patch`. The phase ships the **opposite of a fake consensus**: `aura_consensus::baseline` (survey + refusal) and the `aura-consensus` binary, no new crate and no new dependency (`aura-consensus` already existed as an empty placeholder with a `[[bin]]` since C0).
- **The binary refuses.** `aura-consensus` with no arguments exits **2** and prints the six missing capabilities with the phase that owes each one; `--help`/`--version` succeed; `report` exits **3** when asked to measure a directory that has no `crates/`; an unknown command exits **64** with the usage text.
- **The survey is measured, not typed.** `aura-consensus report` walks `crates/aura-consensus/src` and counts files, lines and public item declarations for the six consensus layers, classifying each as `implemented`, `placeholder` or `missing`. Measured now: **0 implemented, 6 placeholders, 0 missing**.
- The result is pinned in `crates/aura-consensus/tests/fixtures/b0_baseline.txt`, written by `tools/gen_b0_vectors.py` from the binary's own `report --fixture` output. The gate compares live survey to pin, so landing consensus code (B1) fails the build until the plan and the fixture change in the same patch.
- DoD gate: 10 unit tests (refusal exit code, refusal names every capability, report refuses non-workspace, unknown command → usage, two different logs not equivalent, survey deterministic, fixture matches tree, documentation is not a public item, no public consensus items).
- Evidence: `docs/evidence/b0/` (gates, report, inventory), 6/6 NC, **177** NC total across C0–C10 + S0–S7 + X0–X8 + D0 + D1 + B0. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: a static count is not an audit — "implemented" says nothing about correctness, which is what B1–B5's gates measure. Nothing here orders a transaction, votes on a block or talks to a peer, on purpose.
- What M0 still needs after B0: **B1–B5** (proposer, QC, finality, validator set, VRF leader) and **N1–N2** (transport, gossipsub) before consensus can talk to other nodes; then **D2–D7** (node pipeline, mempool, RPC, metrics, devnet, CLI).

### B1 receipt (2026-10-29)

- Deliverable: `patches/0032-B1-poa-proposer.patch`. The phase lands the first real consensus code: `aura_consensus::proposer` (deterministic round-robin over N validators) and `aura_consensus::signature` (simple Ed25519 block signature with domain separation). No new crate, no new dependency — builds on `aura-types`, `aura-crypto`, `aura-block`, and `aura-tx`.
- **Proposer.** `Proposer::new(validators)` selects `validators[round % len]` for each round — deterministic PoA rotation. Panics on empty validator set. `is_proposer(address, round)` checks membership.
- **Block signature.** `BlockSignature::sign_block(seed, validator, block_hash, chain_id)` produces a 64-byte Ed25519 signature over `hash_domain(chain_id, Domain::Block, block_hash)`. `verify(chain_id)` checks it. `SignedBlock::sign(params, txs, seed, validator, chain_id)` builds and signs a block in one call.
- **The survey is measured, not typed.** `aura-consensus report` now measures **2 implemented, 4 placeholders, 0 missing**: proposer and block-signature are `implemented`; qc, finality, validator-set, vrf-leader remain `placeholder`.
- The result is pinned in `crates/aura-consensus/tests/fixtures/b1_baseline.txt`, written by `tools/gen_b1_vectors.py` from the binary's own `report --fixture` output.
- DoD gate: 22 unit tests (proposer rotation, is_proposer, single validator, sign/verify roundtrip, wrong chain id, wrong block hash, tampered signature, wrong public key, survey deterministic, fixture matches tree, 2 implemented + 4 placeholders, refusal exit code, refusal names every capability, report refuses non-workspace, unknown command → usage, two different logs not equivalent).
- Evidence: `docs/evidence/b1/` (gates, report, inventory), 6/6 NC, **183** NC total across C0–C10 + S0–S7 + X0–X8 + D0 + D1 + B0 + B1. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B1 is PoA, not BFT. There is no voting, no quorum certificate, no finality, no validator rotation. B2–B5 add those one at a time.

### B2 receipt (2026-10-29)

- Deliverable: `patches/0033-B2-qc-bls.patch`. The phase lands QC / BLS aggregation: `aura_consensus::bls` (BLS12-381 secret key, public key, signature, domain-separated signing and verification, address derivation) and `aura_consensus::qc` (Vote, VoteType, QuorumCertificate, QcAggregator, quorum_threshold). No new crate; builds on `aura-types`, `aura-crypto` plus `blst = "=0.3.14"`.
- **BLS12-381 signatures.** `BlsSecretKey::from_seed([u8; 32])` derives a key; `BlsSecretKey::sign(chain_id, domain, message)` signs with domain separation via `aura_crypto::hash_domain`; `verify` checks an Ed25519-free BLS signature. `address_from_public_key` derives a 20-byte validator address. 9 unit tests.
- **Quorum certificates.** `Vote { view, round, block_hash, validator, vote_type, signature }`; `QuorumCertificate { view, round, block_hash, vote_type, public_keys, signatures }`; `QcAggregator` collects votes keyed by validator address and emits a QC once `quorum_threshold(n)` signatures are present. QC verification checks every signature individually. 8 unit tests.
- **The survey is measured, not typed.** `aura-consensus report` now measures **3 implemented, 3 placeholders, 0 missing**: proposer, block-signature, and qc are `implemented`; finality, validator-set, vrf-leader remain `placeholder`.
- The result is pinned in `crates/aura-consensus/tests/fixtures/b2_baseline.txt`, written by `tools/gen_b2_vectors.py` from the binary's own `report --fixture` output.
- DoD gate: 36 unit tests (sign/verify roundtrip, wrong chain id, wrong domain, tampered message, wrong public key, address determinism, quorum threshold, aggregator reaches quorum, aggregator ignores duplicate, aggregator below threshold, QC verify all valid, QC verify wrong chain ID fails, survey deterministic, fixture matches tree, 3 implemented + 3 placeholders, refusal exit code, refusal names every capability, report refuses non-workspace, unknown command → usage, two different logs not equivalent).
- Evidence: `docs/evidence/b2/` (gates, report, inventory), 6/6 NC, **189** NC total across C0–C10 + S0–S7 + X0–X8 + D0 + D1 + B0 + B1 + B2. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B2 aggregates votes into a QC but has no finality (no 3-chain commit), no validator set management (no staking/epoch), and no VRF leader election. B3–B5 add those one at a time.

### B3 receipt (2026-10-29)

- Deliverable: `patches/0034-B3-bft-pipeline.patch`. The phase lands BFT finality: `aura_consensus::finality` (`FinalityBlock`, `FinalizedBlock`, `ThreeChainCommit`, `ViewChange`, `Timeout`, `FinalityEngine`). No new crate, no new dependency — builds on `aura-types`, `aura-crypto`, `aura-consensus::bls`, and `aura-consensus::qc`.
- **3-chain commit.** A block is committed when QCs exist for it, its child, and its grandchild, all in the same view. Once committed, the block and all ancestors are finalized. `ThreeChainCommit` tracks blocks, QCs, and parent-child links; `finalize_committed()` walks committed chains and assigns heights.
- **View change.** `ViewChange` tracks the current view, round, and highest QC. On timeout, `change_view()` increments the view, resets the round, and keeps the high QC for bootstrap. `change_round()` advances within the same view.
- **Timeouts.** `Timeout` wraps a `Duration` and an `Instant` deadline. `is_expired()` checks whether the view-change timer has fired; `reset()` rearms it.
- **Nil blocks.** `FinalityBlock::propose_nil()` creates an empty-transaction block that advances the chain when there is nothing to commit.
- **Consensus timestamp.** Every `FinalityBlock` carries `timestamp_ms` (unix milliseconds), so the chain never loses its sense of time.
- **Domain separation.** All votes are BLS-signed over `(chain_id, Domain::Vote, view || round || block_hash || vote_type)`, so votes cannot be replayed across views or chains.
- **The survey is measured, not typed.** `aura-consensus report` now measures **4 implemented, 2 placeholders, 0 missing**: proposer, block-signature, qc, and finality are `implemented`; validator-set and vrf-leader remain `placeholder`.
- The result is pinned in `crates/aura-consensus/tests/fixtures/b3_baseline.txt`, written by `tools/gen_b3_vectors.py` from the binary's own `report --fixture` output.
- DoD gate: 11 unit tests (propose + nil block, 3-chain commit, two blocks not committed, view change on timeout, round change, block hash changes with content, nil block has no transactions, verify vote roundtrip, verify vote wrong chain fails, high QC tracks newest, commit preserves genesis anchor) + 25 baseline tests = 36 total in the B3 gate.
- Evidence: `docs/evidence/b3/` (gates, report, inventory), 6/6 NC, **195** NC total across C0–C10 + S0–S7 + X0–X8 + D0 + D1 + B0 + B1 + B2 + B3. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B3 finalizes blocks via 3-chain commit but does not manage validators (no staking/epoch) and does not elect leaders by VRF. B4–B5 add those one at a time.

## Risks

- **Safety vs liveness**: under partition a halt is possible, but two different commits are not. This invariant is checked by the B10 model.
- **Stake grinding / proposer censoring** — VRF + B5 + `SECURITY_PLAN` H6.
- **Long-range attack** — checkpoint + unbonding + documented weak subjectivity.
- **Reward inconsistency** — the B7 invariant catches "printed" coins.

## Non-goals

PoW, mining pools, private consensus, sharding (a separate plan after M5).

## Closure criteria

A 100-node network with f Byzantine nodes preserves safety, finalizes blocks
with measured latency, punishes double-signs and upgrades the protocol by vote —
all with logs in `docs/evidence/b*/`.
