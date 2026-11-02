# CONSENSUS_PLAN — BFT-PoS, staking, finality, governance

**Status:** ✅ COMPLETE — B0–B10 ✅ (`patches/0031-B0-consensus-baseline.patch`..`patches/0034-B3-bft-pipeline.patch`, `patches/0043-B4-validator-set.patch`..`patches/0044-B5-vrf-leader.patch`, `patches/0053-B6-slashing-evidence.patch`..`patches/0057-B10-safety-evidence.patch`).  
**Stage:** Stage 1: B0–B10 (all phases active and complete).  
**Deliverable:** `patches/0031-B0-consensus-baseline.patch`..`patches/0057-B10-safety-evidence.patch` (landed).
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
| ✅ **B4** | Validator set | staking contract, epoch, enter/exit queue, set update | 4→7 validator change at height H without stopping the network |
| ✅ **B5** | VRF leader | stake-weighted VRF, grinding protection, public verification | 10 000 rounds of proposer distribution within confidence interval |
| ✅ **B6** | Slashing / evidence | double-sign, surround-vote, downtime, penalties, distribution | 6 attack types → evidence accepted, penalty applied, NC: slashing off → test red |
| ✅ **B7** | Rewards | inflation/reward, fee burn, delegation, commission, sum invariant | `sum(balances) + burned == issued` within 1 unit over 100 000 blocks |
| ✅ **B8** | Finality | fork choice, finalized checkpoint, light-client proof, deep reorg defence | attempt to reorg a finalized block → reject + evidence |
| ✅ **B9** | On-chain governance | proposal → vote → timelock → runtime upgrade (WASM), emergency pause | first protocol upgrade in testnet without a manual validator restart |
| ✅ **B10** | Safety evidence | TLA+/Apalache model, 100-node simulation, long-range analysis, close-out | model: safety for f < n/3; NC: remove quorum check → model finds a counterexample |

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

### B4 receipt (2026-10-31)

- Deliverable: `patches/0043-B4-validator-set.patch`. The phase lands validator set management: `aura_consensus::validator_set` (`ValidatorStatus`, `Validator`, `StakeEntry`, `EnterRequest`, `ExitRequest`, `EpochInfo`, `SetUpdate`, `StakingContract`, `ValidatorSet`, `SetUpdateError`). No new crate, no new dependency.
- **Staking contract.** `StakingContract` tracks validator self-stake and per-delegator delegations, enforces `min_stake` (`DEFAULT_MIN_STAKE = 1_000_000`), and drives `ValidatorSet::request_enter` / `ValidatorSet::request_exit`.
- **Epoch boundaries & enter/exit queues.** `ValidatorSet::advance_block()` increments block height inside the current epoch and deterministically processes queued `EnterRequest` and `ExitRequest` entries at epoch boundaries (`epoch_length`), updating `total_stake` and validator statuses without halting block production or 3-chain finality.
- **QC-verified set updates.** `ValidatorSet::apply_set_update` and `apply_verified_set_update` validate `update.epoch == self.epoch.number + 1` (`SetUpdateError::WrongEpoch`), minimum stake across all validators (`SetUpdateError::InsufficientStake`), 2f+1 quorum certificate binding to `current_tip_hash` (`SetUpdateError::InvalidQc`), and BLS signature verification over `(chain_id, Domain::Vote)`.
- **The survey is measured, not typed.** `aura-consensus report` now measures **5 implemented, 1 placeholder, 0 missing**: `proposer`, `block-signature`, `qc`, `finality`, and `validator-set` are `implemented`; `vrf-leader` remains `placeholder`.
- The result is pinned in `crates/aura-consensus/tests/fixtures/b4_baseline.txt`, written by `tools/gen_b4_vectors.py` from the binary's own `report --fixture` output.
- DoD gate: 59 `aura-consensus` library unit tests + 2 integration tests in `crates/aura-consensus/tests/b4_validator_set.rs` (including continuous 100-block finality across a 4→7 validator transition at height `H = 50` across 7 nodes, shifting quorum from 3-of-4 to 5-of-7 without stopping the network).
- Evidence: `docs/evidence/b4/` (gates, report, inventory, SHA256SUMS), 6/6 NC (`tools/nc_b4.py`), **249** NC total across C0–C10 + S0–S7 + X0–X8 + D0–D6 + B0–B4 + N0–N2. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B4 manages the validator set, staking, epochs, and QC-authorized set updates, while VRF stake-weighted leader election and N1/N2 consensus networking land in B5.

### B5 receipt (2026-10-31)

- Deliverable: `patches/0044-B5-vrf-leader.patch`. The phase lands stake-weighted VRF leader election, grinding protection, public verification, and N1/N2 consensus networking: `aura_consensus::vrf_leader` (`VrfInput`, `VrfProof`, `VrfError`, `prove_vrf`, `verify_vrf`, `derive_epoch_randomness`, `VrfValidatorEntry`, `VrfLeaderElection`, `LeaderClaim`, `ProposalEnvelope`, `VoteEnvelope`, `NetworkedValidatorNode`, `FourValidatorNetwork`). Zero new external dependencies (`aura-p2p` workspace member wired to `aura-consensus`).
- **Cryptographic VRF & grinding protection.** `VrfInput` binds `(chain_id, epoch, epoch_randomness, view, round)` and rejects zero `epoch_randomness` after epoch 0 (`VrfError::ZeroEpochRandomness`). `derive_epoch_randomness` verifies the boundary `QuorumCertificate` under `(chain_id, Domain::Vote)` (`VrfError::InvalidBoundaryQc`) and canonically sorts signer `(public_key, signature)` pairs so vote arrival order cannot be manipulated to grind future epoch seeds. `prove_vrf` and `verify_vrf` use deterministic BLS12-381 signatures over the domain-separated VRF transcript and verify both the BLS proof signature and the 32-byte uniform output hash.
- **Stake-weighted leader election.** `VrfLeaderElection::select_leader` maps the uniform 64-bit round VRF draw onto `[0, total_stake)` via 128-bit fixed-point arithmetic (`((draw as u128) * (total_stake as u128)) >> 64`), selecting each validator with probability strictly proportional to `stake_i / total_stake`. Over 10,000 rounds across unequal stakes (`10%`, `20%`, `30%`, `40%`), every validator's empirical proposer count falls within its statistical confidence interval (`+/- 200` rounds).
- **N1/N2 consensus wiring (`D7_READINESS` Step 3).** `NetworkedValidatorNode` and `FourValidatorNetwork` connect `aura_p2p::identity::PeerIdentity` and `aura_p2p::gossipsub::GossipEngine` (`GossipTopic::Proposal` and `GossipTopic::Vote`) to `FinalityEngine`, `QcAggregator`, `ValidatorSet`, and `VrfLeaderElection` without bypassing BLS vote or QC verification; a 4-validator (`f = 1`, quorum = 3) network finalizes 100 rounds over N2 gossip, including 50 rounds with 1 validator offline.
- **The survey is measured, not typed.** `aura-consensus report` now measures **6 implemented, 0 placeholders, 0 missing** across the B1–B5 layers (`proposer`, `block-signature`, `qc`, `finality`, `validator-set`, `vrf-leader`).
- The result is pinned in `crates/aura-consensus/tests/fixtures/b5_baseline.txt`, written by `tools/gen_b5_vectors.py` from the binary's own `report --fixture` output.
- DoD gate: 67 `aura-consensus` library unit tests + 3 integration tests in `crates/aura-consensus/tests/b5_vrf_leader.rs` (10,000-round VRF proposer distribution within confidence interval, 4-validator 100-block N1/N2 BFT finality with 1 validator offline, and forged proposal/vote rejection).
- Evidence: `docs/evidence/b5/` (gates, report, inventory, SHA256SUMS), 6/6 NC (`tools/nc_b5.py`), **255** NC total across C0–C10 + S0–S7 + X0–X8 + D0–D6 + B0–B5 + N0–N2. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B5 completes the B1–B5 core consensus layers and N1/N2 BFT consensus wiring required for NODE D7 (`make devnet`, M1); slashing/evidence, rewards, fork-choice, governance, and TLA+ safety evidence remain in B6–B10.

### B6 receipt (2026-11-01)

- Deliverable: `patches/0053-B6-slashing-evidence.patch`. The phase lands slashing, Byzantine evidence verification, stake penalties, validator jailing/tombstoning, and penalty distribution (`D-B4`): `aura_consensus::slashing` (`SlashingAttackKind`, `SlashingEvidence`, `CheckpointAttestation`, `SlashingParams`, `PenaltyDistribution`, `SlashingReceipt`, `ValidatorSlashingRecord`, `SlashingEngine`, `SlashingError`) and `ValidatorSet::slash_validator`. Zero new external dependencies.
- **Six slashable attack types (`D-B4`).** `SlashingEngine::submit_evidence` cryptographically verifies and penalizes all six B6 offences: (1) `DoubleSignProposal` (two conflicting Ed25519-signed block proposals at the same `(view, round)` → 10% slash + tombstone), (2) `DoubleSignVote` (two conflicting BLS votes of the same `VoteType` at the same `(view, round)` → 10% slash + tombstone), (3) `SurroundVote` (outer checkpoint attestation surrounding inner checkpoint attestation `s_outer < s_inner && t_inner < t_outer` → 15% slash + tombstone), (4) `AmnesiaVote` (conflicting precommit at `view_b > view_a` without a justifying QC at `>= view_a` → 5% slash + jail), (5) `InvalidVrfLeaderClaim` (BLS-signed leader claim for `(view, round)` that fails `VrfLeaderElection::verify_leader_claim` → 5% slash + jail), and (6) `ExtendedDowntime` (`missed_blocks >= max_missed_blocks_per_window` certified by a `2f+1` `QuorumCertificate` → 1% slash + jail).
- **Penalty distribution & replay protection.** `PenaltyDistribution::compute` deterministically splits every slashed amount into 10% reporter reward (`1_000` bps), 40% protocol treasury share (`4_000` bps), and 50% permanently burned (`reporter_reward + treasury_share + burned_amount == total_slashed`). Duplicate evidence replay is rejected via canonical `BLAKE3` `evidence_id`.
- **The survey is measured, not typed.** `aura-consensus report --fixture` is pinned in `crates/aura-consensus/tests/fixtures/b6_baseline.txt`, and 64 deterministic slashing vectors are pinned in `crates/aura-consensus/tests/fixtures/b6_slashing.txt` via `tools/gen_b6_vectors.py`.
- DoD gate: 73 `aura-consensus` library unit tests + 2 integration tests in `crates/aura-consensus/tests/b6_slashing.rs`.
- Evidence: `docs/evidence/b6/` (`README.md`, `gate.log`, `nc.log`, `vectors.log`, `SHA256SUMS`), 6/6 NC (`tools/nc_b6.py`, including `NC-1: slashing off -> test red`), **309** NC total across C0–C10 + S0–S7 + X0–X8 + D0–D8 + B0–B6 + N0–N8. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B6 enforces slashing, jailing/tombstoning, and penalty distribution for the 6 consensus attack types; block rewards/inflation, fork-choice, on-chain governance, and TLA+ safety evidence remain in B7–B10.

### B7 receipt (2026-11-01)

- Deliverable: `patches/0054-B7-rewards.patch`. The phase lands consensus rewards, transaction fee burn, stake delegation, validator commission, and the 100,000-block conservation invariant: `aura_consensus::rewards` (`RewardParams`, `ValidatorRewardConfig`, `BlockRewardReceipt`, `RewardsEngine`, `RewardError`). Zero new external dependencies.
- **Inflation, fee burn & proposer bonus.** `RewardsEngine::apply_block_rewards` mints `base_block_reward` per block (`total_minted += base_block_reward`), burns `fee_burn_bps` (`5,000` bps = 50% default) of `total_tx_fees` (`total_burned += burned_fees`), allocates `proposer_bonus_bps` (`1,000` bps = 10%) of the distributable reward to the block proposer's validator pool, and distributes the remaining `committee_pool` across all active validators proportionally to total stake (`self_stake + sum(delegations)`).
- **Delegation & validator commission.** Within each validator pool, `commission_bps` (`0..=10,000`) is deducted first for the validator, and the remaining `delegator_pool` is split proportionally across the validator's self-stake and each delegator's stake. Any integer-division remainder is deterministically credited so zero sub-units are ever lost or printed.
- **100,000-block conservation invariant.** `RewardsEngine::verify_sum_invariant` verifies `|(sum(balances) + burned) - issued| <= 1` (`0` exact drift) after 100,000 blocks of continuous inflation, fee burning, commissions, and multi-delegator payouts.
- **The survey is measured, not typed.** `aura-consensus report --fixture` is pinned in `crates/aura-consensus/tests/fixtures/b7_baseline.txt`, and 64 deterministic reward/fee-burn/commission/delegation vectors are pinned in `crates/aura-consensus/tests/fixtures/b7_rewards.txt` via `tools/gen_b7_vectors.py`.
- DoD gate: 79 `aura-consensus` library unit tests + 2 integration tests in `crates/aura-consensus/tests/b7_rewards.rs`.
- Evidence: `docs/evidence/b7/` (`README.md`, `gates.txt`, `report.md`, `inventory.txt`, `gate.log`, `nc.log`, `vectors.log`, `SHA256SUMS`), 6/6 NC (`tools/nc_b7.py`), **315** NC total across C0–C10 + S0–S7 + X0–X8 + D0–D8 + B0–B7 + N0–N8. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B7 completes inflation, fee burn, delegation, commission, and the 100,000-block sum invariant; fork-choice/deep-reorg defence, on-chain governance, and TLA+ safety evidence remain in B8–B10.

### B8 receipt (2026-11-01)

- Deliverable: `patches/0055-B8-finality-fork-choice.patch`. The phase lands fork choice, finalized checkpoints, consensus light-client 3-chain proofs, and deep-reorg defence (`D-B3`, `D-B4`): `aura_consensus::fork_choice` (`FinalizedCheckpoint`, `ConsensusLightClientProof`, `SignedForkBlock`, `ReorgRejection`, `ForkChoiceTree`, `ForkChoiceError`). Zero new external dependencies.
- **Fork choice & finalized checkpoints (`D-B3`).** `ForkChoiceTree::select_canonical_head` evaluates only branches that descend from the latest `FinalizedCheckpoint`, ranking candidates by `(justified_qc_view, height)` descending with a deterministic lowest-`block_hash` tie-break. `ForkChoiceTree::update_finalized_checkpoint` verifies the `2f+1` BLS `commit_qc` (`quorum_threshold`), prevents checkpoint height regression, and enforces the weak-subjectivity / unbonding window (`DEFAULT_WEAK_SUBJECTIVITY_WINDOW = 10,000` blocks).
- **Consensus light-client proof (`ConsensusLightClientProof`).** Verifies 3-chain parent links (`target -> child -> grandchild`), same-view rule across all three QCs, and `2f+1` BLS signature verification under `(chain_id, Domain::Vote)`.
- **Deep-reorg defence (`attempt to reorg a finalized block -> reject + evidence`, `D-B4`).** When a candidate block attempts to reorganize a finalized block (`height <= finalized_height` with `hash != canonical_hash`, or does not descend from `finalized_checkpoint`), `ForkChoiceTree::insert_block` rejects the reorg with `ForkChoiceError::DeepReorgPastFinalized` and extracts cryptographic `SlashingEvidence` (`DoubleSignProposal` / `DoubleSignVote`) that is accepted directly by `SlashingEngine::submit_evidence` (`B6`).
- **The survey is measured, not typed.** `aura-consensus report --fixture` is pinned in `crates/aura-consensus/tests/fixtures/b8_baseline.txt`, and 64 deterministic fork-choice and deep-reorg vectors are pinned in `crates/aura-consensus/tests/fixtures/b8_fork_choice.txt` via `tools/gen_b8_vectors.py`.
- DoD gate: 85 `aura-consensus` library unit tests + 2 integration tests in `crates/aura-consensus/tests/b8_fork_choice.rs`.
- Evidence: `docs/evidence/b8/` (`README.md`, `gates.txt`, `report.md`, `inventory.txt`, `gate.log`, `nc.log`, `vectors.log`, `SHA256SUMS`), 6/6 NC (`tools/nc_b8.py`), **321** NC total across C0–C10 + S0–S7 + X0–X8 + D0–D8 + B0–B8 + N0–N8. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B8 completes fork-choice, finalized checkpoints, light-client 3-chain finality proofs, and deep-reorg slashing evidence; on-chain governance (`B9`) and TLA+ safety evidence (`B10`) remain.

### B9 receipt (2026-11-01)

- Deliverable: `patches/0056-B9-on-chain-governance.patch`. The phase lands on-chain governance (`proposal -> vote -> timelock -> runtime upgrade (WASM), emergency pause`): `aura_consensus::governance` (`GovernanceParams`, `ProposalPayload`, `GovernanceVoteChoice`, `GovernanceVote`, `TallyResult`, `ProposalStatus`, `GovernanceProposal`, `ActiveRuntimeState`, `EmergencyPauseState`, `GovernanceEngine`, `HotUpgradableValidatorNode`, `HotUpgradableTestnet`, `GovernanceError`). Zero new external dependencies.
- **Proposal, stake-weighted BLS voting, tally & timelock queue.** `GovernanceEngine::submit_proposal` enforces `min_proposer_deposit` (`DEFAULT_MIN_PROPOSER_DEPOSIT = 100_000`), validates WASM runtime bytecode (`\0asm\x01\0\0\0`, `BLAKE3` `wasm_code_hash`, and strictly increasing `to_version > from_version`), and opens the voting window `[submit_height, submit_height + voting_period_blocks]`. `GovernanceEngine::cast_vote` verifies active validator membership and domain-separated BLS12-381 signatures (`Domain::Vote`). `GovernanceEngine::finalize_voting` tallies stake across `Yes`, `No`, `Abstain`, and `NoWithVeto`, enforcing `veto_threshold_bps` (`3_334` bps, burning the deposit on veto), `quorum_bps` (`3_334` bps), and `pass_threshold_bps` (`6_667` bps supermajority), and moves passing proposals into `ProposalStatus::Timelocked { earliest_execution_height }` for at least `timelock_delay_blocks`.
- **Emergency pause.** `GovernanceEngine::trigger_emergency_pause` and `lift_emergency_pause` verify a `2f+1` validator BLS `QuorumCertificate` (`quorum_threshold`), halting proposal submission and timelock execution while paused (`GovernanceError::EmergencyPaused`).
- **Hot WASM runtime upgrade in testnet without a manual validator restart (`B9` test gate).** `HotUpgradableTestnet` runs a 4-validator BFT testnet from genesis protocol `v1` through proposal submission, validator voting, tallying, timelock delay, and automatic hot activation of `v2` at the scheduled height across all 4 validators with `restart_count == 0` on every validator.
- **The survey is measured, not typed.** `aura-consensus report --fixture` is pinned in `crates/aura-consensus/tests/fixtures/b9_baseline.txt`, and 64 deterministic governance vectors are pinned in `crates/aura-consensus/tests/fixtures/b9_governance.txt` via `tools/gen_b9_vectors.py`.
- DoD gate: 91 `aura-consensus` library unit tests + 2 integration tests in `crates/aura-consensus/tests/b9_governance.rs`.
- Evidence: `docs/evidence/b9/` (`README.md`, `gates.txt`, `report.md`, `inventory.txt`, `gate.log`, `nc.log`, `vectors.log`, `SHA256SUMS`), 6/6 NC (`tools/nc_b9.py`), **327** NC total across C0–C10 + S0–S7 + X0–X8 + D0–D8 + B0–B9 + N0–N8. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: B9 completes on-chain governance, timelocked WASM runtime upgrades without validator restarts, and QC-authorized emergency pause; TLA+/Apalache safety evidence and 100-node simulation remain in `B10`.

### B10 receipt (2026-11-01)

- Deliverable: `patches/0057-B10-safety-evidence.patch`. The phase closes `CONSENSUS_PLAN` (`B0`–`B10`) with the TLA+/Apalache formal model (`docs/evidence/b10/HotStuff3ChainSafety.tla`), bounded BFT safety model checker, 100-node Byzantine simulation, and long-range / weak-subjectivity attack analysis (`D-B2`, `D-B3`, `D-B4`): `aura_consensus::safety_model` (`TLA_SPEC_SOURCE`, `ModelBranch`, `BftModelConfig`, `ModelTraceStep`, `CounterexampleTrace`, `ModelCheckResult`, `BftSafetyModelChecker`, `HundredNodeSimulationConfig`, `HundredNodeSimulationReport`, `run_hundred_node_simulation`, `LongRangeAttackScenario`, `LongRangeAnalysisVerdict`, `analyze_long_range_attack`). Zero new external dependencies.
- **TLA+/Apalache model & bounded model checker (`model: safety for f < n/3; NC: remove quorum check -> model finds a counterexample`).** `HotStuff3ChainSafety.tla` and `BftSafetyModelChecker::check` formalize `QuorumThreshold == 2 * F + 1`, `HonestSingleVotePerView`, `QuorumIntersection`, `ThreeChainCommit`, and `AccountableSafetyInvariant`. Across all validator sets with `3 * f < n` (`(4,1)`, `(7,2)`, `(10,3)`, `(13,4)`, `(31,10)`, `(100,33)`), the model checker proves `ModelCheckResult::Safe` with `min_honest_quorum_overlap >= 1`. Removing the `2f + 1` quorum check (`enforce_quorum_check = false`) or setting `3 * f >= n` produces an explicit 6-step `CounterexampleTrace` committing two conflicting block hashes at the same height and extracting the double-signing validator intersection (`BftSafetyModelChecker::extract_conflicting_qc_offenders`).
- **100-node BFT simulation (`run_hundred_node_simulation`).** Simulates `n = 100` validators (`f = 33` Byzantine equivocators voting on an adversarial fork, `67` honest validators, quorum `2f + 1 = 67`) across 50 consensus heights, proving `conflicting_finalized_nodes == 0` across all 67 honest nodes and `rejected_adversarial_forks == 50`.
- **Long-range & weak-subjectivity analysis (`analyze_long_range_attack`, `D-B3`).** Verifies that posterior-corruption forks signed by compromised historical validator keys are rejected either by the weak-subjectivity unbonding window (`RejectedByWeakSubjectivityWindow`) or by the finalized checkpoint anchor (`RejectedByFinalizedCheckpoint`).
- **The survey is measured, not typed.** `aura-consensus report --fixture` is pinned in `crates/aura-consensus/tests/fixtures/b10_baseline.txt`, and 64 deterministic safety-model and long-range vectors are pinned in `crates/aura-consensus/tests/fixtures/b10_safety_model.txt` via `tools/gen_b10_vectors.py`.
- DoD gate: 97 `aura-consensus` library unit tests + 2 integration tests in `crates/aura-consensus/tests/b10_safety_evidence.rs`.
- Evidence: `docs/evidence/b10/` (`HotStuff3ChainSafety.tla`, `README.md`, `gates.txt`, `report.md`, `inventory.txt`, `gate.log`, `nc.log`, `vectors.log`, `SHA256SUMS`), 6/6 NC (`tools/nc_b10.py`), **333** NC total across C0–C10 + S0–S7 + X0–X8 + D0–D8 + B0–B10 + N0–N8. Doc: [`docs/NODE.md`](../NODE.md).
- Honest scope: `CONSENSUS_PLAN` (`B0`–`B10`) is complete. Cross-plan external security audit and formal handoff remain tracked in `SECURITY_PLAN` (`H2`).

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
