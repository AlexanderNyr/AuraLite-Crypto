# B5 — Stake-weighted VRF leader election, grinding protection, and N1/N2 consensus wiring evidence receipt

Generated (UTC): 2026-10-31T15:25:39+00:00.

## Delivered behavior

`aura-consensus` completes the **B5** VRF leader and N1/N2 BFT networking phase
(`aura_consensus::vrf_leader`) and updates `aura_consensus::baseline` to report
all 6 core consensus layers (`proposer`, `block-signature`, `qc`, `finality`,
`validator-set`, `vrf-leader`) as `implemented`:

- **Cryptographic VRF & grinding protection (`VrfInput`, `VrfProof`, `prove_vrf`, `verify_vrf`, `derive_epoch_randomness`)**:
  - `VrfInput` binds `(chain_id, epoch, epoch_randomness, view, round)` and rejects
    zero `epoch_randomness` after epoch 0 (`VrfError::ZeroEpochRandomness`).
  - `derive_epoch_randomness` verifies the boundary `QuorumCertificate` under
    `(chain_id, Domain::Vote)` (`VrfError::InvalidBoundaryQc`) and sorts signer
    `(public_key, signature)` pairs canonically so vote arrival order cannot be
    manipulated to grind future epoch seeds.
  - `prove_vrf` and `verify_vrf` use deterministic BLS12-381 signatures over the
    domain-separated VRF transcript and verify both the BLS proof signature and
    the 32-byte uniform output hash.
- **Stake-weighted VRF leader election (`VrfLeaderElection`, `LeaderClaim`)**:
  - Maps the uniform 64-bit round VRF draw onto `[0, total_stake)` via 128-bit
    fixed-point arithmetic (`((draw as u128) * (total_stake as u128)) >> 64`),
    selecting each validator with probability strictly proportional to
    `stake_i / total_stake`.
  - Over 10,000 rounds across unequal stakes (`10%`, `20%`, `30%`, `40%`), every
    validator's empirical proposer count falls within its statistical confidence
    interval (`+/- 200` rounds).
- **Connecting B3/B4 BFT finality to N1/N2 (`ProposalEnvelope`, `VoteEnvelope`, `NetworkedValidatorNode`, `FourValidatorNetwork`)**:
  - Wires `aura_p2p::identity::PeerIdentity` and `aura_p2p::gossipsub::GossipEngine`
    (`GossipTopic::Proposal` and `GossipTopic::Vote`) to `FinalityEngine`,
    `QcAggregator`, `ValidatorSet`, and `VrfLeaderElection` without bypassing
    BLS vote verification or QC verification.
  - A 4-validator (`f = 1`, quorum = 3) network finalizes 100 rounds over N2
    gossip, including 50 rounds with 1 validator offline (`set_online(3, false)`).

## Gate results

- `make test-b5`: 67 `aura-consensus` library unit tests and 3 `b5_vrf_leader`
  integration tests passed.
- `make nc-b5`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact vrf_leader::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **255/255** cumulative negative controls through B5; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean B4-baseline (`2cf0321a83d6b308950a196b476ab086bf50c42c` + `0040-N0` +
  `0041-N1` + `0042-N2` + `0043-B4`) apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
