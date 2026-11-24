# ECONOMICS_PLAN — tokenomics, fees, treasury, white paper

**Status:** 🚧 IN PROGRESS — T0 ✅ (`patches/0078-T0-baseline.patch`); T1 ✅ (`patches/0079-T1-issuance.patch`); T2 ✅ (`patches/0080-T2-allocation.patch`); T3 ✅ (`patches/0081-T3-fees.patch`); T4 ✅ (`patches/0082-T4-staking.patch`) — **Stage 1 (T0–T4) CLOSED**; T5 ✅ (`patches/0083-T5-slashing.patch`); T6 ✅ (`patches/0084-T6-governance.patch`); T7 ✅ (`patches/0085-T7-treasury.patch`); T8 ✅ (`patches/0086-T8-whitepaper.patch`) — **ECONOMICS complete**  
**Stage:** Stage 1: T0–T4. Stage 2: T5 ✅, T6 ✅, T7 ✅, T8 ✅.  
**Deliverable:** `patches/0077-T0-...` … `patches/0085-T8-...`
**Dependencies:** Stage 1 — CONSENSUS B4–B9, EXEC X3–X5, ECOSYSTEM E3, OPS O4.  
T7–T8 (⏸ Stage 2) additionally require E4–E8 and O5–O6 (T5 and T6 were
delivered as self-contained pinned models; see their receipts).

## Problem

A technically correct network with undescribed economics is either a hidden
premine, random inflation, or a token that legally cannot be offered to the
public. Economics must be **measurable**: parameters in genesis, invariants in
tests, simulations in `sim/`, text in the white paper.

## Decisions

- **D-T1.** Native asset `ALCRY`, 18 decimals, `u128`. Emission cannot be changed "by a simple commit": only governance (T6/B9) with timelock.
- **D-T2.** No promises of returns. Validator rewards are payment for securing the network and are described as variable parameters.
- **D-T3.** Every parameter change is accompanied by an agent-based simulation with invariants (`supply`, `staked_ratio`, `real_apr`).
- **D-T4.** The legal perimeter is a separate phase T8 (⏸ Stage 2), not "see a lawyer later". Stage 1 issues only testnet-`ALCRY` with no market value.

## Phases

| Phase | Title | DoD | Test gate |
|---|---|---|---|
| ✅ **T0** | Baseline | no token; desired properties described (scarcity, security, accessibility) | NC: "emission from nowhere" in simulation → gate red — met, 1000-step simulation, golden digest pinned, 6 mutation controls |
| ✅ **T1** | Issuance model | genesis supply, inflation curve, halving/target staked ratio, max supply (if fixed) | 10-year simulation: supply matches the formula within 1 unit — met, 0-unit drift every year, golden path digest pinned, 6 mutation controls |
| ✅ **T2** | Allocation | allocation (validators, treasury, community, team), vesting with cliffs, lockups | script reconciles genesis balances with the table; NC: extra coins → red — met, pots sum to exactly 3 000 000 `ALCRY`, golden table digest pinned, 6 mutation controls |
| ✅ **T3** | Fees | EIP-1559-like `base_fee` + `tip`, burning the base part, elastic gas target | 10 000 congested blocks: base fee rises/falls per the formula — met, formula holds at every step of the pinned run, golden path digest pinned, 6 mutation controls |
| ✅ **T4** | Staking | min stake, delegation, commission, unbonding 21 days, slashing period | 1000 delegators: shares sum == 1; NC: withdraw without unbonding → red — met, shares sum to exactly one pool at every step of the pinned run, golden digest pinned, 7 mutation controls |
| ✅ **T5** | Slashing and insurance | penalty parameters, insurance fund, mass-slash protection | 7 scenarios: penalty ≥ damage, fund never negative — met, both invariants hold at every step of the pinned run (87 steps, 48 incidents), golden digest pinned, 7 mutation controls |
| ✅ **T6** | Governance | proposal threshold, quorum, voting period, 48 h timelock, veto/emergency with sunset | 20 proposals: valid ones pass, flash-stake attack rejected — met, 15/20 executed as designed (4 rejected: majority/quorum/tie/empty, 1 vetoed before the sunset), the flash-stake vote is rejected with `NoVotingPower`, golden digest pinned, 8 mutation controls |
| ✅ **T7** | Treasury and grants | on-chain treasury, multisig (W4), transparent reporting, grants process | every payout has proposal + tx + report; NC: withdrawal without vote → red — met: 6 payouts (tx 1..6) each with proposal + report; `NoVote` on withdrawal without a passed vote; multisig 2-of-3 enforced; overdraw/double-exec/rejected-proposal red; 7 mutation controls |
| ✅ **T8** | White paper + law | white paper (project, technology, risks, holder rights), marketing materials, legal review in the EU/Latvia | checklist against MiCA Art. 4/6/8/9, notification ≥ 20 working days, publication; **not legal advice** — met: 8-document checklist run (1 fully valid; one gate failure each for the rest), notification window pinned at 20 working days (boundary pass / 19 red), publication before offer enforced, not-legal-advice disclaimer enforced, 7 mutation controls |

### T0 receipt (2026-11-20) — economics baseline

- Deliverable: `patches/0078-T0-baseline-phase-closed-emission-from-nowhere.patch`;
  evidence in `docs/evidence/t0/` (`campaign/` self-contained: gate run
  log, `result.json` with commands, the golden digest and source digests).
- Gate met literally: the "emission from nowhere" mutation (recipient
  credited one unit extra per transfer) turns the gate red with the named
  message `T0: transfers must conserve the total supply` (NC-1). The
  positive gate runs the deterministic 1000-step baseline schedule
  (`sim/t0-baseline.json`, seed 20261180) and checks the supply identity
  after **every** step; the final state digest
  `aec12b64eada1b850e7c73ac1e2afce62fb398df0d27365c7af300c295c7a51a`
  is pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t0_economics_check.py`), which also cross-runs the Rust
  example `t0_sim`.
- No token: the simulation uses abstract testnet accounting units (no
  market value, decision D-T4); `ALCRY` arrives with the T1 issuance
  model. Desired properties (scarcity, security, accessibility) are
  described and enforced: `docs/economics/t0-baseline.md`,
  `aura_storage::econ::DESIRED_PROPERTIES`.
- Explained deviations (recorded here as this plan prescribes):
  (1) the simulation engine lives in `crates/aura-storage/src/econ.rs`
  (the workspace convention for phase code) while `sim/` holds the
  scenario it runs — `sim/t0-baseline.json`; (2) reproducibility is
  pinned by a cross-language golden digest (sha256 of the sorted
  `name:balance` state) instead of a numeric tolerance.
- Negative controls: `tools/nc_t0.py`, 6/6 in `docs/evidence/t0/nc.log`;
  cumulative negative control total 518.

### T1 receipt (2026-11-20) — issuance model

- Deliverable: `patches/0079-T1-issuance-phase-closed-10-year-supply-mat.patch`;
  evidence in `docs/evidence/t1/` (`campaign/` self-contained: gate run
  log, `result.json` with commands, the golden path digest and source
  digests).
- Gate met literally: the 10-year simulation matches the closed-form
  formula **within 1 base unit at every year** (integer truncation gives
  0-unit drift); the "extra unit per year" mutation turns the gate red
  with the named message `T1: the supply must match the issuance formula`
  (NC-1). Parameters (genesis-fixed, D-T1/D-T3): genesis 3 000 000
  `ALCRY`, annual emission 300 000 `ALCRY` halving every 4 years, hard
  cap 6 000 000 `ALCRY` (fixed), target staked ratio 6000 bps (pinned for
  T4). Ten-year path 3.0M → 4.95M `ALCRY`; golden path digest
  `c2a4669192ddde5d9cc4e7cb32a76ecd06356022bce7c7230826f068854c1a35`
  pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t1_issuance_check.py`), which also cross-runs the Rust
  example `t1_issuance_example`.
- Cap enforcement: the binding-cap scenario (3.5M cap, 100 years) halts
  issuance at exactly the cap (NC-3); genesis drift, halving removal,
  staked-ratio validation removal and horizon truncation each turn red
  with their named message (`tools/nc_t1.py`, 6/6).
- Explained deviations (recorded here as this plan prescribes):
  (1) the model lives in `crates/aura-storage/src/issuance.rs` (workspace
  convention) with the scenario mirrored in `sim/t1-issuance.json`;
  (2) the horizon granularity is whole years (the gate says "10-year
  simulation"; block-level emission belongs to T3/T4 mechanics).

### T2 receipt (2026-11-20) — allocation: validators, treasury, community, team

- Deliverable: `patches/0080-T2-allocation.patch`; evidence in
  `docs/evidence/t2/` (`campaign/` self-contained: gate run log,
  `result.json` with commands, the golden table digest and source
  digests).
- Gate met literally: `tools/t2_allocation_check.py` reconciles the
  genesis balances with the allocation table — the pots sum to exactly
  3 000 000 `ALCRY` (= the T1 genesis supply), and the phase control is
  exactly the named one: **one extra coin anywhere turns the
  reconciliation red** with `T2: the allocation must sum to the genesis
  supply` (NC-1). Table (genesis-fixed, D-T1): validators 1 200 000
  `ALCRY`, treasury 600 000 (vesting 24 months, 6-month cliff), community
  750 000, team 450 000 (vesting 36 months, 12-month cliff). Golden table
  digest
  `5fe7417a134011e7028f255cf7cf180335cc23c773ee9657a68de260fcc9ea41`
  pinned in the Rust tests and re-derived by the independent Python
  oracle, which also cross-runs the Rust example `t2_allocation_example`.
- Vesting with cliffs and lockups enforced, not decorative: the cliff
  locks everything before it, vesting completes exactly at the end
  (truncated linear), and the locked part cannot be spent
  (`check_spend`); vesting overflow, cliff removal, non-completing
  vesting, ignored lockup and table drift each turn the gate red with
  their named message (`tools/nc_t2.py`, 6/6). Two metamorphisms: split
  pots preserve the total and the lock profile; monthly deltas
  telescope to the vested amount.
- Explained deviations (recorded here as this plan prescribes):
  (1) the table lives in `crates/aura-storage/src/allocation.rs`
  (workspace convention) with the scenario mirrored in
  `sim/t2-allocation.json`;
  (2) the reconciliation is against the T1 genesis supply (the phase
  says "genesis balances") — validator rewards from the validators pot
  are T4 mechanics and carry no promises of returns (D-T2).

### T3 receipt (2026-11-21) — fees: base fee + tip, burned base, elastic target

- Deliverable: `patches/0081-T3-fees.patch`; evidence in
  `docs/evidence/t3/` (`campaign/` self-contained: gate run log,
  `result.json` with commands, the golden path digest and source
  digests).
- Gate met literally: the pinned run is **10 000 congested blocks**
  (250-block demand segments cycling +25 %/−25 %/+50 %/−50 %/target —
  bursts, lulls, quiet blocks) and the base fee rises and falls **per
  the formula** at every step: `base ± max(base·|gas−target|/target/8,
  1)` in integer arithmetic (EIP-1559, change of at least one base unit
  off target, unchanged at target). An independent in-test
  re-derivation checks all 10 000 steps; the golden path digest
  `412f1f9e120ec829fb5d2ae65e4590da69771cd7b49bef83f29440d0d93bc124`
  is pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t3_fees_check.py`), which also cross-runs the Rust
  example `t3_fees_example`. Checkpoints: base after block 249 = 2 192
  756 374 619, after block 499 = 783 284 026, final = 56 473; peak
  2 993 325 499 252 235; burned 2 039 212 976 242 596 352 500 000; tips
  150 000 000 000 000 000 000.
- Fees are split, not promised: the base part is **burned**
  (`burned = gas × base_fee`, `paid == burned + tips`), only the tip
  (`gas × tip_per_gas`) goes to the validator — payment for inclusion,
  no returns promised (D-T2). Elastic gas target 15 000 000 with the
  block limit 30 000 000 (elasticity 2) enforced (`check_gas`). Wrong
  formula denominator, unburned base, uncredited tip, swapped
  directions, unenforced limit and drifted constants each turn the gate
  red with their named message (`tools/nc_t3.py`, 6/6). Metamorphisms:
  at-target blocks are flat (minimum one-unit delta off target),
  splitting a transaction preserves burn/tips/total, higher constant
  demand yields a strictly higher base fee.
- Explained deviations (recorded here as this plan prescribes):
  (1) the model lives in `crates/aura-storage/src/fees.rs` (workspace
  convention) with the scenario mirrored in `sim/t3-fees.json`;
  (2) the formula is the EIP-1559 update rule with the EIP's minimum
  delta of one base unit; the scenario mixes congested and quiet blocks
  ("congested blocks" as a congestion regime) so the gate literally
  covers rises **and** falls; parameter changes stay governance-locked
  (D-T1 timelock, T6).

### T4 receipt (2026-11-21) — staking: min stake, delegation, commission, unbonding, slashing

- Deliverable: `patches/0082-T4-staking.patch`; evidence in
  `docs/evidence/t4/` (`campaign/` self-contained: gate run log,
  `result.json` with commands, the golden digest and source digests).
- Gate met literally: the pinned run drives **1000 delegators**
  (delegations 1000–1999 `ALCRY`), 600 reward steps, two slashes
  (100/500 bps), partial undelegations and matured withdrawals — and
  the delegator **shares sum to exactly one** pool
  (`Σ shares == total_shares`) at **every** one of the 2004 steps; the
  golden digest
  `e80662c82c74e1d1a0a5b0624beb4cf08af88c24f8091751c277cc7ea6887f07`
  is pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t4_staking_check.py`, full model re-implementation),
  which also cross-runs the Rust example `t4_staking_example`.
  Checkpoints: bonded after the 100 bps slash = 1 484 861 400 000 000
  000 000 000, after the 500 bps slash = 1 325 529 559 769 089 696 565
  650, final bonded 1 325 889 559 769 089 696 565 650; final shares =
  Σ shares = 1 408 708 333 333 333 333 333 367; withdrawn 85 430 770
  230 910 303 434 449; validator commission 120 000 000 000 000 000
  000.
- The phase NC is literal: **a withdrawal without unbonding is
  rejected** (`NoUnbonding`; early claim → `StillUnbonding`) — one block
  before maturity the attempt is recorded as a rejection in the digest;
  exactly after 21 days (362 880 blocks at 5 s) the claim succeeds.
  DoD enforced: min stake 1 000 `ALCRY`; delegation via shares
  (rewards compound into the pool, share counts never move);
  commission 1000 bps **from rewards only** (D-T2: no promised
  income); slashing period = the unbonding window — bonded and
  not-yet-withdrawn stake is cut proportionally. Seven mutation
  controls (`tools/nc_t4.py`, 7/7): share-accounting drift, allowed
  withdrawal without unbonding, shortened window, double commission,
  dropped min-stake, non-proportional slash and parameter drift each
  turn the gate red with their named message. Metamorphisms: splitting
  a delegation preserves the shares; consecutive slashes compose
  proportionally.
- Explained deviations (recorded here as this plan prescribes):
  (1) the model lives in `crates/aura-storage/src/staking.rs` (workspace
  convention) with the scenario mirrored in `sim/t4-staking.json`;
  (2) "shares sum == 1" is realized as the exact integer identity
  `Σ delegator shares == total_shares` (the pool fractions sum to one)
  checked at every step;
  (3) the mutation budget is **7** controls (the standard's 6 plus the
  slash proportionality control — the H6–H9 precedent); one validator
  pool is the T4 model (multi-validator sets belong to B-series/T6).
- Negative controls: `tools/nc_t1.py`, 6/6 in `docs/evidence/t1/nc.log`;
  cumulative negative control total 524.

## Legal perimeter (what exactly T8 checks)

A public offer of a crypto-asset in the EU requires the offeror to be a legal
person and to draw up, notify and publish a crypto-asset white paper; the
notification is at least 20 working days before publication (Regulation (EU)
2023/1114, MiCA)
[2](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32023R1114),
[3](https://www.amf-france.org/en/news-publications/depth/mica),
[5](https://cms.law/en/int/publication/legal-experts-on-markets-in-crypto-assets-mica-regulation/eu-regulation-mandates-crypto-asset-white-paper).
There are exemptions (e.g. free distribution as a validation reward, offers to
fewer than 150 persons per Member State, volume < EUR 1 000 000 over 12 months)
[4](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32023R1114) —
in the plan they are reviewed by a lawyer, not chosen "by eye". If the token is
classified as an ART/EMT or a financial instrument, the regime is different and
the plan is revised before O6.

### T5 receipt (2026-11-21) — slashing and insurance: penalty parameters, insurance fund, mass-slash protection

- Deliverable: `patches/0083-T5-slashing.patch`; evidence in
  `docs/evidence/t5/` (`campaign/` self-contained: gate run log,
  `result.json` with commands, the golden digest and source digests).
- Gate met literally: **7 scenarios** (single double-sign, repeat offender,
  correlated mass slash, fund exhaustion, penalty margin floor, mass-slash
  cap over windows, exhaustion and refill chain) with both invariants —
  `penalty >= damage` and the insurance fund **never negative** — checked
  at every step of the pinned run (87 steps, 48 incidents). The golden
  digest `4cc9ea0319374c3c2c74bc88db6d7747b23f0e21ea637ae2270bc4c5707c9cef`
  is pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t5_slashing_check.py`), which also cross-runs the Rust
  example `t5_slashing_example`.
- Penalty parameters: `T5_PENALTY_BPS = 15_000` (the offender pays 150 %
  of the measured damage, so `penalty >= damage` by construction);
  `T5_FUND_SHARE_BPS = 5_000` of every applied penalty replenishes the
  insurance fund, which covers victims as `min(damage, balance)` and
  records the uncovered part as `shortfall`. Mass-slash protection:
  `T5_MASS_SLASH_CAP_BPS = 500` of the registered bonded base
  (`T5_REGISTERED_BONDED` = 1 000 000 `ALCRY`) may be destroyed per
  window; the excess penalty is **deferred, never dropped**, and drains
  over windows (`total_applied == total_penalty`, `pending == 0` at the
  end of the run).
- NC: 7/7 named messages (`tools/nc_t5.py`; standard 6 + the mass-slash
  cap control, H6–H9/T4 precedent). 10 gate tests + 2 metamorphisms
  (splitting a report preserves the totals; reordering independent
  reports preserves the balance).
- Explained deviations (recorded here as this plan prescribes):
  (1) T5 ships as a self-contained pinned model in
  `crates/aura-storage/src/slashing.rs` — the E4–E8/O5–O6 items of
  Stage 2 remain external-review dependencies (T-series precedent:
  parameters are pinned and simulated, not deployed);
  (2) the fund balance is a signed `i128` on purpose, so an overdraw
  would be visible to the "never negative" invariant instead of wrapping.

### T6 receipt (2026-11-21) — governance: threshold, quorum, voting period, 48 h timelock, veto with sunset

- Deliverable: `patches/0084-T6-governance.patch`; evidence in
  `docs/evidence/t6/` (`campaign/` self-contained: gate run log,
  `result.json` with commands, the golden digest and source digests).
- Gate met literally: **20 proposals** are driven by the pinned run
  (113 steps) — **valid ones pass** (15 executed; 4 rejected by design:
  majority against, quorum missed, tie `yes == no`, no votes; 1 vetoed
  one block before the sunset) and the **flash-stake attack is
  rejected**: a 1 000 000 `ALCRY` stake taken after the snapshot has no
  voting power and the vote fails with `NoVotingPower` (message
  `T6: a flash-stake attack must be rejected`). The golden digest
  `828b13cf7ca3abcaea65386f7c62e1e155f4d207cb000aceccd86397b5c729aa`
  is pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t6_governance_check.py`), which also cross-runs the
  Rust example `t6_governance_example`.
- Parameters, each with a boundary-checked edge in the run: proposal
  threshold 10 000 `ALCRY` (a proposer with exactly the threshold is
  accepted, below is `BelowThreshold`); quorum 2000 bps of the snapshot
  total (voted == quorum exactly passes; a tie is rejected — strict
  `yes > no`); voting period 120 960 blocks (7 days at 5 s) as the
  half-open window `[created, created + period)` (a vote at the closing
  block is `VotingClosed`); **48 h timelock** = 34 560 blocks
  (one block early is `Timelock`); the emergency veto **sunsets** at
  block 1 521 000 (one block before — works, at the sunset and after —
  `VetoSunset`).
- NC: 8/8 named messages (`tools/nc_t6.py`; standard 6 + the flash-stake
  control and the veto sunset, H6–H9/T4/T5 precedent). 10 gate tests +
  2 metamorphisms (reordering independent votes preserves the tally; a
  late stake does not change a snapshot).
- Explained deviations (recorded here as this plan prescribes):
  (1) T6 ships as a self-contained pinned model in
  `crates/aura-storage/src/governance.rs` — the E4–E8/O5–O6 items of
  Stage 2 remain external-review dependencies (T-series precedent);
  (2) the 20 proposals are driven grouped per proposal with per-proposal
  timelines, so block heights are monotone within a proposal's block of
  actions rather than globally.

## Risks

- Inflation eating stakers, or conversely insufficient security at a low staked ratio → T1/T3/T4 simulations.
- Governance attack via flash-borrowed stake → T6 + B4.
- Legal uncertainty → T8 before mainnet, not after.

## Non-goals

Exchange listings, price, marketing promises, KYC/AML of custodial services
(separate O9 track, not code).

### T7 receipt (2026-11-21) — treasury and grants

- Deliverable: `patches/0085-T7-treasury.patch`; evidence in
  `docs/evidence/t7/` (`campaign/` self-contained).
- Gate met literally: "every payout has proposal + tx + report; NC:
  withdrawal without vote -> red" — the pinned run drives 10 proposals /
  82 steps / 6 payouts (tx 1..6), each payout linked to an executed
  proposal, a numbered tx and a transparent `Report {tx_id, proposal_id,
  amount, recipient, purpose, height}`; a withdrawal without a passed
  vote returns `NoVote` (run step 19; NC-1 turns the gate red with the
  named message `T7: a withdrawal without a vote must be rejected`).
  Council 5 (strict majority) + multisig 2-of-3 (W4); signed `i128`
  balance: conservation `final + paid = initial + inflows` (225 000 +
  475 000 == 500 000 + 200 000 ALCRY).
- Golden digest `719c93b244ff42ebf94538a5e9fb5b9e8f52eac07035abaf3c526e4c0a3231f7`
  pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t7_treasury_check.py`), which cross-runs
  `t7_treasury_example`.
- Negative controls: `tools/nc_t7.py`, 7/7 in `docs/evidence/t7/nc.log`;
  cumulative 565 over 24 phases.

### T8 receipt (2026-11-22) — white paper + law

- Deliverable: `patches/0086-T8-whitepaper.patch`; the white paper
  `docs/whitepaper.md`, marketing materials `docs/marketing.md` and the
  EU/Latvia review perimeter `docs/legal-review.md`; evidence in
  `docs/evidence/t8/`. **Not legal advice** (D-T4); no income promises
  (D-T2).
- Gate met literally: "checklist against MiCA Art. 4/6/8/9, notification
  >= 20 working days, publication; not legal advice" — the pinned run
  drives 8 documents / 77 steps (1 fully valid; one gate failure each
  for the rest; outcomes `[0, 2, 3, 4, 7, 6, 1, 9]`). Art. 4 legal
  person + lifecycle, Art. 6 sections + traceable claims (pins:
  `MAX_SUPPLY`, `GENESIS_SUPPLY`, `T3_INITIAL_BASE_FEE`,
  `T6_PROPOSAL_THRESHOLD`, `T7_SIGNER_THRESHOLD`), Art. 8
  `T8_NOTICE_WORKING_DAYS = 20` (boundary: 20 passes, 19 red), Art. 9
  publication before the offer + published == notified, Art. 7 marketing
  after publication with a reference, mandatory `T8_NOT_LEGAL_ADVICE`.
- Golden digest `03a859433b58f14d6ad538faf2f3b504631c8b05707856a6ef1580694a89396f`
  pinned in the Rust tests and re-derived by the independent Python
  oracle (`tools/t8_whitepaper_check.py`), which cross-runs
  `t8_whitepaper_example`.
- Negative controls: `tools/nc_t8.py`, 7/7 in `docs/evidence/t8/nc.log`;
  cumulative 572 over 25 phases.

## Closure criteria

Stage 1: T0–T4 parameters are fixed in genesis and verified by simulations.
Stage 2: T5 ✅ (slashing and insurance), T6 ✅ (governance, pinned
models), T7 ✅ (treasury and grants), T8 ✅ (white paper + law
checklist); ⏸ legal opinion (qualified EU/Latvia review) and O6.
