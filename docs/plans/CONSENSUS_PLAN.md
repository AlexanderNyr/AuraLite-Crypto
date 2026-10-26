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
| **B0** | Baseline | single node, ordered tx log, measure "no consensus", report | NC: two different logs pass as "the same" → documented defect |
| **B1** | PoA round-robin | N validators, deterministic proposer, simple block signature | 4 validators, 100 blocks, identical head on all |
| **B2** | QC / BLS | prevote/precommit, 2f+1 aggregation, QC verification, fake QC rejected | f=1 (4 nodes): 3 signatures → commit, 2 → no; NC: substituted pubkey → red |
| **B3** | BFT pipeline | 3-chain commit, view change, timeouts, nil blocks, consensus timestamp | 1 node partitioned → network continues; 100 rounds without desync |
| **B4** | Validator set | staking contract, epoch, enter/exit queue, set update | 4→7 validator change at height H without stopping the network |
| **B5** | VRF leader | stake-weighted VRF, grinding protection, public verification | 10 000 rounds of proposer distribution within confidence interval |
| **B6** | Slashing / evidence | double-sign, surround-vote, downtime, penalties, distribution | 6 attack types → evidence accepted, penalty applied, NC: slashing off → test red |
| **B7** | Rewards | inflation/reward, fee burn, delegation, commission, sum invariant | `sum(balances) + burned == issued` within 1 unit over 100 000 blocks |
| **B8** | Finality | fork choice, finalized checkpoint, light-client proof, deep reorg defence | attempt to reorg a finalized block → reject + evidence |
| **B9** | On-chain governance | proposal → vote → timelock → runtime upgrade (WASM), emergency pause | first protocol upgrade in testnet without a manual validator restart |
| **B10** | Safety evidence | TLA+/Apalache model, 100-node simulation, long-range analysis, close-out | model: safety for f < n/3; NC: remove quorum check → model finds a counterexample |

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
