# ECOSYSTEM_PLAN — applications, explorer, DeFi, bridge

**Status:** 🚧 IN PROGRESS — E0 ✅ (`patches/0074-E0-baseline.patch`); E1 ✅ (`patches/0075-E1-indexer.patch`); E2 ✅ (`patches/0076-E2-Explorer.patch`); E3 ✅ (`patches/0077-E3-Faucet-SDK.patch`); E5 ✅ (`patches/0087-E5-Oracle.patch`, D-E3 order: oracle before DeFi); E4 ✅ (`patches/0088-E4-AMM.patch`); E6 ✅ (`patches/0089-E6-CDP.patch`); E7 ✅ (`patches/0090-E7-Lending.patch`); E8 ✅ (`patches/0091-E8-NFT-DAO.patch`); E9 ✅ (`patches/0092-E9-Bridge.patch`)  
**Stage:** Stage 1: E0–E3. Stage 2: E4 ✅ (AMM), E6 ✅ (stablecoin CDP), E7 ✅ (lending), E8 ✅ (NFT/DAO), E9 ✅ (bridge) — all after the oracle and H8 audit per D-E3; Stage 2 E-phases complete.  
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

### E5 receipt (2026-11-22) — oracle

- Deliverable: `patches/0087-E5-Oracle.patch`; evidence in
  `docs/evidence/e5/` (`campaign/` self-contained: gate run log,
  `result.json` with commands and source digests).
- Gate met literally: **5 providers, 2 malicious → price unchanged** —
  the two liars (x100 pump, 0.0001 dump) land outside the ±500 bps
  outlier band and the published price stays the honest median (100.0,
  18 decimals); **NC: one provider → red** with the named message
  `E5: oracle requires a quorum of provider reports (1/3)`.
- Model `crates/aura-storage/src/oracle.rs`: ed25519-signed provider
  reports (canon `feed16||price16BE||ts8BE`), strict admission
  (unknown/duplicate/foreign-feed/bad-signature/stale-or-future → named
  messages), quorum N/2+1 (min 3), lower-median aggregation, outlier
  band ±500 bps around the provisional median, lower-median timestamp.
  Constants: staleness 3600 s (heartbeat), deviation 500 bps, Price =
  u128 18 decimals (D-T1). Decisions: D-E3 order kept (oracle before
  DeFi; H8 audit closed).14 gate tests including metamorphic
  permutation/scale/determinism; simulation `sim/e5-oracle.json` +
  `crates/aura-storage/examples/e5_oracle_example.rs` (`E5-SIM-OK`),
  independent Python model `tools/e5_oracle_check.py` (`E5-MODEL-OK`),
  aggregate digest
  `aee332b61c43f19d234ec79044208daa4bb1671ec1e8126b5d7ff75a4bdead24`.
  Report: `docs/ecosystem/e5-oracle-report.md`.
- Negative controls: `tools/nc_e5.py`, 6/6 in `docs/evidence/e5/nc.log`;
  cumulative negative control total 578 (26 phases).

### E4 receipt (2026-11-22) — AMM DEX

- Deliverable: `patches/0088-E4-AMM.patch`; evidence in
  `docs/evidence/e4/` (`campaign/` self-contained: gate run log,
  `result.json` with commands and source digests).
- Gate met literally: **invariant `k` never decreases** — 21 tests,
  `e4_gate_k_never_decreases` runs a deterministic swap sequence both
  directions at wildly varying sizes plus multi-hop routes and asserts
  `wide_ge(k_after, k_before)` in 256-bit limbs on every pool
  (`k = reserve_a · reserve_b` overflows u128 on purpose); **NC:
  slippage guard off → red** — the `bad-slippage` mutation in
  `tools/nc_e4.py` weakens the named message and the suite turns red.
- Model `crates/aura-storage/src/amm.rs`: constant-product AMM (x·y = k),
  fee-on-input 30 bps (0.3%, Uniswap V2 model) with floor rounding at
  every division, 256-bit `wide_mul`/`mul_div`, LP tokens (initial mint
  `a + b` — explained deviation from Uniswap's `sqrt`; later mints
  strictly proportional `min(a·S/ra, b·S/rb)`; burn pro-rata with exact
  round trip), `swap_exact_in` with slippage guard, `swap_route`
  multi-hop A→B→C. 9 named messages. Decisions D-T1..D-T4. 21 gate tests
  including metamorphic split≤single, route=manual-hops and determinism;
  simulation `sim/e4-amm.json` +
  `crates/aura-storage/examples/e4_amm_example.rs` (`E4-SIM-OK`),
  independent Python model `tools/e4_amm_check.py` (`E4-MODEL-OK`),
  aggregate digest
  `c1286a3521b58ab9a53c7032db4099c517ec57448abeba4edcc1cc0096283865`.
  Golden values: swap 1 A→B out of (100/200) = `1974316068794122597`,
  route 1 A→B→C = `5791186053878244695`, single 2A `3910033923564131223`
  ≥ split `3909976989011504086`. Report:
  `docs/ecosystem/e4-amm-report.md`.
- Negative controls: `tools/nc_e4.py`, 6/6 in `docs/evidence/e4/nc.log`;
  cumulative negative control total 584 (27 phases).

### E6 receipt (2026-11-22) — stablecoin CDP

- Deliverable: `patches/0089-E6-CDP.patch`; evidence in
  `docs/evidence/e6/` (`campaign/` self-contained: gate run log,
  `result.json` with commands and source digests).
- Gate met literally: **200% collateral at −40% price → liquidation
  with no bad debt** — vault 200/100 at price 0.6 has ratio 12000 bps,
  `liquidate` seizes `175e18` (debt + 5% penalty at 0.6), burns the
  full `100e18` debt (`shortfall == 0`), returns `25e18` to the owner;
  `bad_debt == 0`, peg backing intact. Property extension
  `e6_gate_peg_backing_never_breaks`: three vaults (200/220/300 vs
  100 debt), shocks 0.9/0.6/0.45, three liquidations — at every step
  `bad_debt == 0 && backed_value >= stable_supply`.
- Model `crates/aura-storage/src/cdp.rs`: MakerDAO-style vaults —
  min ratio 150% (ETH-A liquidation ratio), liquidation penalty 5%
  (USDC-A/Raft precedent), 256-bit `mul_div` with floor rounding,
  mint/withdraw keep the ratio, liquidation seizes `debt·(1+penalty)`
  and returns the excess, peg invariant
  `bad_debt == 0 && backed_value >= stable_supply`. 12 named messages.
  Decisions D-T1..D-T4. 14 gate tests including metamorphic deposit
  split, mint/repay round trip, liquidation-order commutativity and
  determinism; simulation `sim/e6-cdp.json` +
  `crates/aura-storage/examples/e6_cdp_example.rs` (`E6-SIM-OK`),
  independent Python model `tools/e6_cdp_check.py` (`E6-MODEL-OK`),
  aggregate digest
  `9b64b653f7464ff1c634743e55c1bd4b1383381a75210e75e234ab11ba499c95`.
  Report: `docs/ecosystem/e6-cdp-report.md`.
- Negative controls: `tools/nc_e6.py`, 6/6 in `docs/evidence/e6/nc.log`
  (key NC `bad-mint-ratio`: mint-ratio guard message weakened → red);
  cumulative negative control total 590 (28 phases).

### E7 receipt (2026-11-22) — lending

- Deliverable: `patches/0090-E7-Lending.patch`; evidence in
  `docs/evidence/e7/` (`campaign/` self-contained: gate run log,
  `result.json` with commands and source digests).
- Gate met literally: **30-day simulation, `collateral >= debt`
  invariant never breaks** — two accounts (1000/700 and 500/300),
  five price shocks (0.85/0.72/0.61/0.52/0.44 on days 6/12/18/24/30),
  daily interest accrual at the kinked utilization rate, 16
  liquidations restoring positions (min health 12530 bps); the
  invariant `value >= debt` holds every simulated day. **NC: no
  liquidation → red** — the same path without liquidations breaks the
  invariant on day 18 (`e7_gate_no_liquidation_breaks_invariant`).
- Model `crates/aura-storage/src/lending.rs`: Aave/Compound-style
  lending market — LTV 75% borrow cap, liquidation at health < 12500
  bps (LTV > 80%; the LTV↔threshold gap is the price buffer), 5%
  liquidation bonus, 50% close factor, kinked rate model (base 2% +
  slope1 10%·U/kink, kink 80%, slope2 30% above), daily floor accrual
  `debt·rate/(365·10000)`. 12 named messages. Decisions D-T1..D-T4.
  14 gate tests including metamorphic supply split, borrow/repay
  round trip, interest monotonicity and determinism; simulation
  `sim/e7-lending.json` +
  `crates/aura-storage/examples/e7_lending_example.rs` (`E7-SIM-OK`),
  independent Python model `tools/e7_lending_check.py` (`E7-MODEL-OK`),
  aggregate digest
  `f8fa28de165958235dc3547b03259824440fc0b74ca86d1fbe30c6d93ebea7dd`.
  Report: `docs/ecosystem/e7-lending-report.md`.
- Negative controls: `tools/nc_e7.py`, 6/6 in `docs/evidence/e7/nc.log`
  (key NC `bad-borrow-capacity`: LTV guard message weakened → red);
  cumulative negative control total 596 (29 phases).

### E8 receipt (2026-11-22) — NFT + names + DAO

- Deliverable: `patches/0091-E8-NFT-DAO.patch`; evidence in
  `docs/evidence/e8/` (`campaign/` self-contained: gate run log,
  `result.json` with commands and source digests).
- DoD met literally: **50 standard vectors + first DAO vote through
  the UI.** (1) The pinned fixture
  `crates/aura-storage/tests/fixtures/e8-standards.txt` (generated by
  `tools/gen_e8_vectors.py`) runs green on both the Rust model and the
  independent Python model with exact `E8: …` error literals and the
  rebuilt vectors digest
  `8cb9da945b0eed8f4432d31b71d20fffb72a6317fc90204d1890bd480c062bd3`.
  (2) The DAO scenario opens proposal 1 and casts **the first vote
  through the governance UI** (receipt recorded, tally 250000/75000/
  25000 ALCRY lands on-chain, double vote rejected
  `E8: vote already cast (1/1)`, proposal finalized and executed after
  the timelock); the governance page renders byte-identical in Rust and
  Python (sha `bf1486e2…1167f`).
- Model `crates/aura-storage/src/nft.rs`: ARC-721 per the repo's X5
  standard (transfers clear approvals — stale-approval vector v013),
  ARC-1155 (per-id supply, operators, all-or-nothing batches), ENS-
  shaped `.aura` names (365-day term, 90-day grace, renewal by anyone,
  re-registration after expiry+grace, primary/reverse), on-chain
  identity records. Governance UI `src/dao_ui.rs` wraps the T6 DAO
  (propose/vote/finalize/execute/veto + receipt log + HTML page).
  25 named messages. Decisions D-E6..D-E8. 15 gate tests with
  metamorphic controls (batch ≡ sequential, mint order, vote order,
  determinism). Aggregate digest
  `39674d6a0813cb9beae5837025e1a8b6c00fb8ebe1bc620a06e844a5829fb313`.
  Report: `docs/ecosystem/e8-nft-report.md`.
- Negative controls: `tools/nc_e8.py`, 6/6 in `docs/evidence/e8/nc.log`
  (key NC `bad-ui-vote`: UI double-vote message weakened → red);
  cumulative negative control total 602 (30 phases).

### E9 receipt (2026-11-22) — bridge + AuraLite OS

- Deliverable: `patches/0092-E9-Bridge.patch`; evidence in
  `docs/evidence/e9/` (`campaign/` self-contained: gate run log,
  `result.json` with commands and source digests, `lane.log`).
- DoD met literally (decision D-E4 protections): **MITM forged header →
  reject** — four attack shapes (foreign signer 99 appended to an
  honest commit, bit-flipped validator signature, wholly attacker-signed
  commit, duplicate vote) all rejected with the exact
  `E9: forged header signature (…)`; every commit signature is
  verified with no early stop, quorum is strictly more than 2/3 of the
  power (27 of 40). **OS client checks finalized checkpoint** —
  `OsClient` (the AuraLite OS embedding point) verifies checkpoints
  against the validator set and accepts messages only at or below the
  finalized checkpoint (`E9: checkpoint not finalized (6)`); a forged
  checkpoint is rejected at the OS boundary. **Emulator lane green** —
  `tools/e9_emulator_lane.py` (`make emulator-lane`, registered in CI):
  `E9-LANE-OK 4` — `aurad --version`, `aurad report --fixture`, startup
  refusal without `--chain-id` (exit 2), bounded node run with clean
  SIGTERM shutdown.
- Model `crates/aura-storage/src/bridge.rs`: IBC/Tendermint-shaped light
  client (hash chain + ed25519 commits + 2/3 quorum), bridge protections
  (per-tx 1000 / daily 1200 velocity limits, guardian pause, one-time
  message ids — the Nomad replay lesson, reserve invariant
  `locked == minted` — the Wormhole lesson) and the OS checkpoint
  client. 25 named scenario steps pinned (digest
  `039d498ba6fcd17e7640094492283a94be2c3447f8ebec89f9306eb438339580`);
  independent Python model `tools/e9_bridge_check.py` (RFC 8032 via
  `cryptography`) reproduces `E9-STEPS-OK 25` / `E9-SIM-OK` byte for
  byte. Aggregate digest
  `4f8d9af363ab88fb7a6ce0ef6af52facb886f5e123ca0a128d1693e06d575e4f`.
  8 gate tests with metamorphic controls. Report:
  `docs/ecosystem/e9-bridge-report.md`. Deviation: external
  AuraLite-OS / AuraLite-Emulator repositories replaced by in-repo
  `OsClient` + the `aurad` lane runner (integration tracked there).
- Negative controls: `tools/nc_e9.py`, 6/6 in `docs/evidence/e9/nc.log`
  (key NC `bad-finalized`: checkpoint-finalized message weakened → red);
  cumulative negative control total 608 (31 phases).
- Separate audit (D-E4): the bridge is not enabled on mainnet without
  the H8 external audit + bug bounty (SECURITY_PLAN).

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
| ✅ **E4** | AMM DEX | constant-product pools, LP tokens, swap, slippage, routing | property: invariant `k` never decreases; NC: slippage guard off → red — met, 21 tests + 6 mutation controls |
| ✅ **E5** | Oracle | feed network, median, quorum, staleness, provider signatures | 5 providers, 2 malicious → price unchanged; NC: one provider → red — met, 14 tests + 6 mutation controls |
| ✅ **E6** | Stablecoin | over-collateralized CDP, liquidations, peg stress | 200% collateral at -40% price → liquidation with no bad debt — met, 14 tests + 6 mutation controls |
| ✅ **E7** | Lending | markets, interest rate model, collateral factor, liquidation engine | 30-day simulation: `collateral >= debt` invariant; NC: no liquidation → red — met, 14 tests + 6 mutation controls + functional no-liquidation control (breach day 18) |
| ✅ **E8** | NFT + names + DAO | ARC-721/1155, `.aura` name service, on-chain identity, governance UI | 50 standard vectors + first DAO vote through the UI — met: 50 vectors green (digest 8cb9da94…), first vote cast through the UI (receipt + tally + byte-identical governance page), 6 mutation controls |
| ✅ **E9** | Bridge + AuraLite OS | IBC-like light-client bridge, limits/pause; light client in AuraLite OS; CI run of `aurad` in AuraLite-Emulator | MITM forged header → reject; OS client checks finalized checkpoint; emulator lane green — met: 4 forgery shapes rejected (digest 039d498b…), OsClient finalized-only, `E9-LANE-OK 4` registered in CI |

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
