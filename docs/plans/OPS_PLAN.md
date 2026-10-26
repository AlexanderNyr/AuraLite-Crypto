# OPS_PLAN — CI, releases, testnet, mainnet, community

**Status:** 📋 PLAN (O0–O9)  
**Stage:** Stage 1: O0–O4. ⏸ Stage 2: O5–O9 (mainnet, upgrades, community).  
**Deliverable:** `patches/0103-O0-...` … `patches/0112-O9-...`
**Dependencies:** all Stage 1 technical plans; O5/O6 are the final north-star
gates (⏸ Stage 2).

## Problem

"Works on my machine" is not yet a network. We need reproducible builds, a
public testnet with third-party validators, a genesis ceremony, monitoring,
documentation and an incident plan. Without that, mainnet is just a devnet with
a loud name.

## Decisions

- **D-O1.** Everything CI runs can be run locally with one command (`make test`). No "secret" shards.
- **D-O2.** Releases are reproducible and signed; SBOM attached (H1/H7).
- **D-O3.** Mainnet (⏸ Stage 2) starts only after feature freeze +
  audit (H8) + white paper (T8) + ceremony (W7). Stage 1 ends at O4: public
  testnet with no market token.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| **O0** | CI baseline | GitHub Actions, 10 shards, cache, `make test`, coverage report | NC: disable a shard → CI red (target registry check) |
| **O1** | Releases | reproducible build, Docker multi-stage, signatures, SBOM, CHANGELOG, semver | two independent hosts → identical binary sha256 |
| **O2** | M0/M1 gates | 4-validator devnet, "how to run" docs, first receipts | ROADMAP §5 M0 and M1 criteria met |
| **O3** | Closed testnet | invited operators, monitoring, on-call, backup/restore drills | 7 days uptime, 0 state losses, 1 restore drill |
| **O4** | Public testnet (M2) | faucet, explorer, docs, bug bounty, public metrics, >= 10 external validators | 30 days, >= 99% uptime, >= 100 000 txs, 0 unfixed criticals |
| ⏸ **O5** | Mainnet candidate (M3) | feature freeze, H8 audit, T8 white paper, W7 genesis ceremony, H9 playbook | independent check: same genesis hash on all validators; 0 criticals |
| ⏸ **O6** | Mainnet launch (M4) | >= 21 validators, no entity >= 1/3 of stake, 24/7 monitoring, chain-id fixed | north star: network finalizes blocks for 7 days, logs in `docs/evidence/o6/` |
| ⏸ **O7** | First upgrade | on-chain governance (B9/T6) really updates the protocol without a hard fork | Proposal → timelock → activation; NC: upgrade without timelock → red |
| ⏸ **O8** | Docs and support | validator/developer/user guides, localization, FAQ, status page | 10 external validators onboard using docs only |
| ⏸ **O9** | External perimeter (not code) | listings, liquidity, partnerships, legal support, community | honestly labelled **not guaranteed by code**; not part of the north star |

## Risks

- Validators belonging to one operator ("decentralization on paper") — operator uniqueness check in O6.
- Upgrade without timelock/emergency plan — O7 + H9.
- Publishing "mainnet" before the audit — forbidden by D-O3, caught by the O5 review gate.

## Non-goals

Exchange listing as a technical gate, marketing, price promises (O9 is only an
organizational track).

## Closure criteria

Stage 1: public testnet M2 with external validators, reproducible releases and
monitoring (O0–O4).
Stage 2 (⏸): mainnet, audit, incident playbook and the first governance upgrade
(O5–O9). Anything not closed is explicitly listed in `docs/STATUS.md`.
