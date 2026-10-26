# ECONOMICS_PLAN — tokenomics, fees, treasury, white paper

**Status:** 📋 PLAN (T0–T8)  
**Stage:** Stage 1: T0–T4. ⏸ Stage 2: T5–T8 (slashing parameters, governance, treasury, white paper).  
**Deliverable:** `patches/0077-T0-...` … `patches/0085-T8-...`
**Dependencies:** Stage 1 — CONSENSUS B4–B9, EXEC X3–X5, ECOSYSTEM E3, OPS O4.  
T5–T8 (⏸ Stage 2) additionally require E4–E8 and O5–O6.

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
| **T0** | Baseline | no token; desired properties described (scarcity, security, accessibility) | NC: "emission from nowhere" in simulation → gate red |
| **T1** | Issuance model | genesis supply, inflation curve, halving/target staked ratio, max supply (if fixed) | 10-year simulation: supply matches the formula within 1 unit |
| **T2** | Allocation | allocation (validators, treasury, community, team), vesting with cliffs, lockups | script reconciles genesis balances with the table; NC: extra coins → red |
| **T3** | Fees | EIP-1559-like `base_fee` + `tip`, burning the base part, elastic gas target | 10 000 congested blocks: base fee rises/falls per the formula |
| **T4** | Staking | min stake, delegation, commission, unbonding 21 days, slashing period | 1000 delegators: shares sum == 1; NC: withdraw without unbonding → red |
| ⏸ **T5** | Slashing and insurance | penalty parameters, insurance fund, mass-slash protection | 7 scenarios: penalty ≥ damage, fund never negative |
| ⏸ **T6** | Governance | proposal threshold, quorum, voting period, 48 h timelock, veto/emergency with sunset | 20 proposals: valid ones pass, flash-stake attack rejected |
| ⏸ **T7** | Treasury and grants | on-chain treasury, multisig (W4), transparent reporting, grants process | every payout has proposal + tx + report; NC: withdrawal without vote → red |
| ⏸ **T8** | White paper + law | white paper (project, technology, risks, holder rights), marketing materials, legal review in the EU/Latvia | checklist against MiCA Art. 4/6/8/9, notification ≥ 20 working days, publication; **not legal advice** |

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

## Risks

- Inflation eating stakers, or conversely insufficient security at a low staked ratio → T1/T3/T4 simulations.
- Governance attack via flash-borrowed stake → T6 + B4.
- Legal uncertainty → T8 before mainnet, not after.

## Non-goals

Exchange listings, price, marketing promises, KYC/AML of custodial services
(separate O9 track, not code).

## Closure criteria

Stage 1: T0–T4 parameters are fixed in genesis and verified by simulations.
Stage 2 (⏸): white paper, legal opinion, T5–T8 and O6.
