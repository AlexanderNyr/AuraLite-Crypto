# ECOSYSTEM_PLAN — applications, explorer, DeFi, bridge

**Status:** 📋 PLAN (E0–E9)  
**Stage:** Stage 1: E0–E3. ⏸ Stage 2: E4–E9 (DeFi, oracles, NFT, bridge).  
**Deliverable:** `patches/0067-E0-...` … `patches/0076-E9-...`
**Dependencies:** NODE D4–D7, WALLET W1–W4, EXEC X3–X6.

## Problem

A working chain with no applications is a lab bench. An ecosystem is an
indexer + explorer + SDK + a set of reference contracts + a bridge, all passing
the same gates as the core (otherwise "the dApp only works on the author's
machine").

## Decisions

- **D-E1.** The indexer is idempotent and can roll back on reorg; the explorer gets data only from it.
- **D-E2.** All reference contracts use our own SDK, with verifiable source and reproducible WASM builds.
- **D-E3.** DeFi protocols (AMM/CDP/lending) come **after** the oracle and the audit; the order cannot change.
- **D-E4.** The bridge is the most dangerous component: light-client verification, limits, pause, separate audit.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| **E0** | Baseline | no applications; report "what a user must be able to do" | checklist of 10 user scenarios (0/10 so far) |
| **E1** | Indexer | read canonical chain → Postgres, idempotency, reorg rollback | 10 000 blocks + artificial reorg → 0 duplicates/losses, NC: without rollback → red |
| **E2** | Explorer | Rust backend + web UI: blocks/tx/accounts/validators/contracts, verified source | 10 user scenarios green; reproducible contract build = same code_hash |
| **E3** | Faucet + SDK | rate-limited faucet (testnet only), Rust/TS/Python SDK, docs site | 1000 requests: 1 payout/address/day; SDK examples pass in CI |
| ⏸ **E4** | AMM DEX | constant-product pools, LP tokens, swap, slippage, routing | property: invariant `k` never decreases; NC: slippage guard off → red |
| ⏸ **E5** | Oracle | feed network, median, quorum, staleness, provider signatures | 5 providers, 2 malicious → price unchanged; NC: one provider → red |
| ⏸ **E6** | Stablecoin | over-collateralized CDP, liquidations, peg stress | 200% collateral at -40% price → liquidation with no bad debt |
| ⏸ **E7** | Lending | markets, interest rate model, collateral factor, liquidation engine | 30-day simulation: `collateral >= debt` invariant; NC: no liquidation → red |
| ⏸ **E8** | NFT + names + DAO | ARC-721/1155, `.aura` name service, on-chain identity, governance UI | 50 standard vectors + first DAO vote through the UI |
| ⏸ **E9** | Bridge + AuraLite OS | IBC-like light-client bridge, limits/pause; light client in AuraLite OS; CI run of `aurad` in AuraLite-Emulator | MITM forged header → reject; OS client checks finalized checkpoint; emulator lane green |

## Risks

- **Bridge = bank**: withdrawal limits, timelock, pause, separate audit (H8).
- **Oracle = single point of failure**: E5 before E6/E7, no "temporary" prices.
- **Indexer lies**: E1 + reconciliation with on-chain data in CI.

## Non-goals

Centralized exchange, custodial services, market making, mobile apps,
"guaranteed returns".

## Closure criteria

Stage 1: a user creates a wallet → receives testnet-`ALCRY` → sees everything in
the explorer and SDK (E0–E3, receipts in `docs/evidence/e0-e3/`).
Stage 2 (⏸): AMM, stablecoin, lending, NFT/DAO and the bridge (E4–E9).
