# D7_READINESS — prerequisite path to the four-validator devnet

**Status:** ✅ COMPLETED (N0–N2 ✅, B4–B5 ✅, and NODE D7 ✅ landed).
**Stage:** Stage 1 — prerequisite phases N0 (✅), N1 (✅), N2 (✅), B4 (✅), B5 (✅), and NODE D7 (✅).
**Baseline:** GitHub `main` at `2cf0321a83d6b308950a196b476ab086bf50c42c`; N0 delivered in `patches/0040-N0-net-baseline.patch`; N1 delivered in `patches/0041-N1-transport.patch`; N2 delivered in `patches/0042-N2-gossipsub.patch`; B4 delivered in `patches/0043-B4-validator-set.patch`; B5 delivered in `patches/0044-B5-vrf-leader.patch`; D7 delivered in `patches/0045-D7-devnet.patch`.
**Deliverables:** separate phase patches; do not label prerequisite patches as D7.

## Why D7 is not ready to implement as a devnet

The NODE_PLAN D7 acceptance requires four validators to agree on a finalized
head, serve a balance over RPC, and continue finalizing after one validator is
down. While N1 and N2 now provide authenticated peer identity, transport,
gossipsub topics, Ed25519 message signing, deduplication, and peer scoring in
`aura-p2p`, `aurad` still composes a local single-node pipeline until
CONSENSUS B5 connects the voting/finality pipeline to N1/N2. Starting four
independent daemons before B5 would create four unrelated chains, not a devnet;
a shared directory or copied head would only fake agreement.

The dependency is explicit in the existing plans: NET_PLAN N1 provides
transport and identity, N2 provides signed gossip, and CONSENSUS_PLAN says
N1–N2 connect at B5 and are mandatory for M1. Do not mark D7 complete or claim
a multi-validator network before that path works.

## Proposed phase sequence

| Step | Owner / phase | Deliverable and exit condition |
|---|---|---|
| 0 | NET N0 ✅ | Pin the honest no-P2P baseline (`patches/0040-N0-net-baseline.patch`) and prove two nodes cannot observe one another's transactions; retain the documented gap. |
| 1 | NET N1 ✅ | Implement authenticated peer identity and bounded transport/handshake according to NET_PLAN (`patches/0041-N1-transport.patch`); reject identity substitution; demonstrate ten authenticated connections. |
| 2 | NET N2 ✅ | Implement signed, deduplicated transaction/proposal/vote gossip and peer scoring (`patches/0042-N2-gossipsub.patch`); bound message sizes and rates; exercise flood penalties. |
| 3 | CONSENSUS B4–B5 ✅ | Deliver validator set (`patches/0043-B4-validator-set.patch`), stake-weighted VRF leader election with grinding protection, and N1/N2 consensus wiring (`patches/0044-B5-vrf-leader.patch`); establish four genesis validators and the f=1 quorum of three. |
| 4 | NODE D7 ✅ | Add deterministic genesis/faucet configuration and `make devnet` orchestration for four independent processes (`patches/0045-D7-devnet.patch`), per-node data/log directories and RPC endpoints. Run M1 against the actual network. |

N1, N2 and B5 should remain separately reviewable phase patches. The D7
patch should depend on their verified patches rather than reimplementing or
silently folding transport/consensus into the node orchestrator.

## D7 acceptance gate (M1)

- `make devnet` starts four separate validator processes with independent
  storage, distinct validator identities, a common chain ID/genesis, documented
  RPC ports, a development-only faucet key, and per-node logs.
- A network-driven run finalizes at least 100 blocks. All four nodes report the
  same finalized height, block hash and state root; no copied/shared database
  is used to manufacture agreement.
- A client queries the faucet-funded account through a node's RPC and receives
  the expected balance.
- Stop one validator during the run. The remaining three preserve the f=1
  quorum and continue finalizing; the test records the outage and resulting
  common head. Restart/rejoin behavior is reported separately unless N4 sync is
  already implemented.
- D6 `/health`, `/metrics`, JSON logs and optional OTLP configuration remain
  per-node and accurately identify each validator. No P2P metric is fabricated.

## Test and negative-control policy

Use a live multi-process test for the M1 end-to-end property. Mutation-control
scripts must target **unit tests**, not integration tests, as required by the
repository workflow. The proposed controls should pin at least: substituted peer
identity rejection, malformed/unsigned gossip rejection, gossip deduplication,
quorum threshold (three of four), no commit with only two votes, and continued
finalization with one of four validators unavailable. The outage M1 scenario is
also exercised separately in its end-to-end test; it does not replace the unit
mutation controls.

Each phase owns a `make test-<phase>`, six unit-test-based negative controls,
evidence under `docs/evidence/<phase>/`, a clean-apply-validated phase patch,
and truthful dependency/checksum records. The final D7 patch must be applied to
a clean tree reconstructed from the preceding verified patch chain, followed
by the phase gates, `make nc-all`, the full `make test`, and strict close-out
audit before it is called ready.

## Explicit non-claims

Until N1/N2 and B5 integration pass, there is no peer transport, no gossip,
no independently validated four-node finality, no resilient multi-validator
devnet, and no valid D7 M1 result. D6 observability and the local single-node
pipeline do not imply any of these capabilities.
