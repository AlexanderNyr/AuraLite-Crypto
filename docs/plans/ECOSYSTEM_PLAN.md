# ECOSYSTEM_PLAN — applications, explorer, DeFi, bridge

**Status:** 🚧 IN PROGRESS — E0 ✅ (`patches/0074-E0-baseline.patch`); E1 ✅ (`patches/0075-E1-indexer.patch`); E2 ✅ (`patches/0076-E2-Explorer.patch`); E3 ✅ (`patches/0077-E3-Faucet-SDK.patch`); ⏸ E4–E9 Stage 2  
**Stage:** Stage 1: E0–E3. ⏸ Stage 2: E4–E9 (DeFi, oracles, NFT, bridge).  
**Deliverable:** `patches/0067-E0-...` … `patches/0076-E9-...`
**Dependencies:** NODE D4–D7, WALLET W1–W4, EXEC X3–X6.

## Problem

A working chain with no applications is a lab bench. An ecosystem is an
indexer + explorer + SDK + a set of reference contracts + a bridge, all passing
the same gates as the core (otherwise "the dApp only works on the author's
machine").

### E0 receipt (2026-11-18) — baseline user scenarios

- Deliverable: `patches/0074-E0-baseline.patch`; evidence in
  `docs/evidence/e0/` (`campaign/` self-contained: baseline logs,
  `result.json` with commands and source digests).
- Gate met literally: the checklist holds **exactly 10 user scenarios**
  with an honest **0/10 readiness** count (no applications exist yet);
  NC-1 drops a scenario and NC-6 falsifies the counter — the oracle turns
  red with the named message both times.
- Report `docs/ecosystem/e0-baseline-report.md` ("what a user must be
  able to do", traceability US-01..US-10 → W/H/X/E phases), registry
  `tools/fixtures/e0_user_scenarios.json`, oracle
  `tools/e0_baseline_check.py` wired into `make test`.
- Negative controls: `tools/nc_e0.py`, 6/6 in `docs/evidence/e0/nc.log`;
  cumulative negative control total 494.

### E1 receipt (2026-11-18) — chain indexer

- Deliverable: `patches/0075-E1-indexer.patch`; evidence in
  `docs/evidence/e1/` (`campaign/` self-contained: gate run log,
  `result.json` with commands and source digests).
- **Deviation (recorded as this plan prescribes):** the Postgres
  component was not created in this phase — the indexer runs over a
  local row store (`crates/aura-storage/src/indexer.rs`, in-process;
  `Cargo.lock` is pinned at H1, no new dependencies). Gates, negative
  controls and metrics are identical to the stated ones.
- Gate met literally: `crates/aura-storage/tests/e1_indexer.rs` builds
  a 10 000-block chain plus an artificial 205-block reorg at height
  9 900 → **0 duplicates / 0 losses** (every fixture block still known,
  a second full ingest returns exactly 10 000 `Duplicate`, tip 10 105,
  the dropped tip stays orphaned); NC-1 removes rollback and the named
  gate `E1: indexer must roll back on reorg` goes red.
- Negative controls: `tools/nc_e1.py`, 6/6 in `docs/evidence/e1/nc.log`;
  cumulative negative control total 500.

### E2 receipt (2026-11-19) — explorer

- Deliverable: `patches/0076-E2-Explorer-phase-closed-10-user-scenarios-green.patch`;
  evidence in `docs/evidence/e2/` (`campaign/` self-contained: gate run
  log, `result.json` with commands and source digests).
- Gate met literally: the checklist `tools/fixtures/e2_explorer_scenarios.json`
  holds **exactly 10 user scenarios, 10/10 ready**, each backed by a
  named assertion in `crates/aura-storage/tests/e2_explorer.rs` served
  over real HTTP; the **reproducible contract build** property holds:
  formatting-equivalent sources build to the **same code_hash**
  (independent Python builder in `tools/e2_explorer_check.py`
  recomputes `tools/fixtures/e2_contracts.json`).
- Explained deviations (recorded here as this plan prescribes):
  (1) the web UI is served by a stdlib HTTP handler
  (`explorer::serve`) instead of a framework — `Cargo.lock` is pinned
  at H1, no new dependencies; (2) "contract build" compiles a
  deterministic ALVM-1 bytecode (the project's own IR) rather than an
  external toolchain — the gate metric "same code_hash" is unchanged;
  (3) validator participation is tallied by the explorer feed at load
  time; consensus health heights come from the indexer.
- Decision D-E1 enforced: block views answer only from `ChainIndexer`;
  NC-3 detaches them and the named gate `E2: explorer must read blocks
  from the indexer` goes red.
- Negative controls: `tools/nc_e2.py`, 6/6 in `docs/evidence/e2/nc.log`;
  cumulative negative control total 506.

### E3 receipt (2026-11-19) — faucet + sdk

- Deliverable: `patches/0077-E3-Faucet-SDK-phase-closed-1000-requests-1-payout.patch`;
  evidence in `docs/evidence/e3/` (`campaign/` self-contained: gate run
  log, `result.json` with commands and source digests).
- Gate met literally: the 1000-request script (250 addresses × 3 days
  + 250 same-day repeats) pays **exactly 750 payouts, 250 rate-limit
  denials, 1 payout per address per day** — re-simulated by an
  independent Python model (`tools/e3_faucet_check.py`,
  `tools/fixtures/e3_faucet_script.json`); the three SDK examples
  (Rust/TS/Python) pass in CI (`tools/e3_examples_check.py`, 3/3,
  identical `E3-EXAMPLE-OK` summaries against a live reference faucet).
- Explained deviations (recorded here as this plan prescribes):
  (1) payouts are faucet-ledger entries with funds deducted — no
  on-chain transfer while W1-FORMAT-v1 testnet freeze is in force;
  (2) the demo HTTP protocol carries an explicit `now` timestamp for
  CI determinism; (3) the TS example runs via a deterministic
  type-strip + `node` (no TypeScript toolchain pinned in CI);
  (4) the docs site is static markdown in `docs/sdk/` (no publishing
  pipeline).
- Negative controls: `tools/nc_e3.py`, 6/6 in `docs/evidence/e3/nc.log`;
  cumulative negative control total 512.

## Decisions

- **D-E1.** The indexer is idempotent and can roll back on reorg; the explorer gets data only from it.
- **D-E2.** All reference contracts use our own SDK, with verifiable source and reproducible WASM builds.
- **D-E3.** DeFi protocols (AMM/CDP/lending) come **after** the oracle and the audit; the order cannot change.
- **D-E4.** The bridge is the most dangerous component: light-client verification, limits, pause, separate audit.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **E0** | Baseline | no applications; report "what a user must be able to do" | checklist of 10 user scenarios (0/10 so far) |
| ✅ **E1** | Indexer | read canonical chain → local row store (Postgres replaced by deviation), idempotency, reorg rollback | 10 000 blocks + artificial reorg → 0 duplicates/losses, NC: without rollback → red — met, 6 mutation controls |
| ✅ **E2** | Explorer | Rust backend + web UI: blocks/tx/accounts/validators/contracts, verified source | 10 user scenarios green; reproducible contract build = same code_hash — met, 10/10 ready + 6 mutation controls |
| ✅ **E3** | Faucet + SDK | rate-limited faucet (testnet only), Rust/TS/Python SDK, docs site | 1000 requests: 1 payout/address/day; SDK examples pass in CI — met, 3/3 examples + 6 mutation controls |
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
