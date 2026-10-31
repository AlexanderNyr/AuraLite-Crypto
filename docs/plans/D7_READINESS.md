# D7_READINESS — prerequisite path to the four-validator devnet

**Status:** 📋 PLAN / BLOCKED ON LOWER-LAYER NETWORK WORK (D7 is not delivered).  
**Stage:** Stage 1 — prerequisite phases N0, N1, N2 and B5, then NODE D7.  
**Baseline:** GitHub `main` at `517ebb6daecba2367d0a8ac6ef52297dd170bdae`; local workspace contains the verified D0–D6 phase series.  
**Deliverables:** separate phase patches; do not label the aggregate workspace patch as D7.

## Why D7 is not ready to implement as a devnet

The NODE_PLAN D7 acceptance requires four validators to agree on a finalized
head, serve a balance over RPC, and continue finalizing after one validator is
down. The current `aura-p2p` crate is an empty placeholder, N1/N2 have no
transport or gossip implementation, and `aurad` composes a local single-node
pipeline. Starting four independent daemons would create four unrelated chains,
not a devnet; a shared directory or copied head would only fake agreement.

The dependency is explicit in the existing plans: NET_PLAN N1 provides
transport and identity, N2 provides signed gossip, and CONSENSUS_PLAN says
N1–N2 connect at B5 and are mandatory for M1. Do not mark D7 complete or claim
a multi-validator network before that path works.

## Proposed phase sequence

| Step | Owner / phase | Deliverable and exit condition |
|---|---|---|
| 0 | NET N0 | Pin the honest no-P2P baseline and prove two nodes cannot observe one another's transactions; retain the documented gap. |
| 1 | NET N1 | Implement authenticated peer identity and bounded transport/handshake according to NET_PLAN; reject identity substitution; demonstrate ten authenticated connections. |
| 2 | NET N2 | Implement signed, deduplicated transaction/proposal/vote gossip and peer scoring; bound message sizes and rates; exercise flood penalties. |
| 3 | CONSENSUS B5 integration | Connect the B3 voting/finality pipeline to N1/N2 without bypassing QC validation; establish four genesis validators and the f=1 quorum of three. Resolve the validator-set/genesis boundary explicitly before code. |
| 4 | NODE D7 | Add deterministic genesis/faucet configuration and `make devnet` orchestration for four independent processes, per-node data/log directories and RPC endpoints. Run M1 against the actual network. |

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
