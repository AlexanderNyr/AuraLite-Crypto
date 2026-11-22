# T8 White paper and law

**Scope**: ECONOMICS_PLAN T8 (white paper + law) — the crypto-asset
white paper (project, technology, risks, holder rights), marketing
materials and the legal review perimeter in the EU/Latvia. **This
document is an engineering checklist and not legal advice** (decision
D-T4); the plain wording is an instrument, not a legal opinion.

## Gate (literal)

> checklist against MiCA Art. 4/6/8/9, notification ≥ 20 working days,
> publication; **not legal advice**

## Model

`crates/aura-storage/src/whitepaper.rs` — the MiCA white paper machine
(Regulation (EU) 2023/1114):

- **Art. 4** (offers to the public): the offeror is a legal person
  (`NoLegalPerson`), and the paper is drawn up, notified and published
  with compliant marketing.
- **Art. 6** (content and form): the four sections of the deliverable
  (project, technology, risks, holder rights; `SectionMissing`) and
  every claim traceable to a pinned model constant (`ClaimUnverified`;
  pins: `MAX_SUPPLY`, `GENESIS_SUPPLY`, `T3_INITIAL_BASE_FEE`,
  `T6_PROPOSAL_THRESHOLD`, `T7_SIGNER_THRESHOLD`).
- **Art. 8** (notification): at least `T8_NOTICE_WORKING_DAYS = 20`
  working days before publication — boundary: exactly 20 passes, 19 is
  red (`NoticeTooShort`, the literal gate control).
- **Art. 9** (publication): on the website before the starting date of
  the offer (`PublishedAfterOffer`), published version identical to the
  notified one (`VersionMismatch`).
- **Art. 7** (marketing): communications reference the published white
  paper and come after publication (`MarketingNotCompliant`).
- **Disclaimer**: `T8_NOT_LEGAL_ADVICE` is mandatory (`NoDisclaimer`,
  the literal "not legal advice" control); D-T2 — no income promises.

## Deliverables

- `docs/whitepaper.md` — the white paper (project, technology, risks,
  holder rights + claims table + disclaimer).
- `docs/marketing.md` — marketing materials with mandatory disclaimer
  and dissemination rules (after publication only).
- `docs/legal-review.md` — the EU/Latvia review perimeter: the 12-point
  MiCA checklist (Art. 4/6/8/9) with per-item status and the named
  model check for each enforceable item.

## Determinism and oracle

The pinned scenario [`run_t8_scenario`] drives 8 documents through the
lifecycle: WP1 fully valid (notify day 100, publish day 120 — exactly
20 working days); WP2 unverified claim; WP3 no disclaimer; WP4
19-day window; WP5 published after offer start; WP6 marketing without
reference and before publication; WP7 missing risks section; WP8 not a
legal person. Final step: `total = 8, valid = 1`; outcomes
`[0, 2, 3, 4, 7, 6, 1, 9]`. Digest:
`03a859433b58f14d6ad538faf2f3b504631c8b05707856a6ef1580694a89396f`.

Oracle: `tools/t8_whitepaper_check.py` (independent Python step-log
model, same digest) cross-checks the Rust run, `tests/t8_whitepaper.rs`
and `sim/t8-whitepaper.json` (schema `auralite-sim-1`).

## Named messages (governance)

1. `T8: every white paper claim must be traceable to a pinned model`
2. `T8: the MiCA checklist must cover articles 4, 6, 8 and 9`
3. `T8: the notification must be at least 20 working days before publication`
4. `T8: the white paper must be published before the offer starts`
5. `T8: marketing communications must reference the published white paper`
6. `T8: the white paper must carry the not-legal-advice disclaimer`
7. `T8: the reference simulation must agree with the pinned digest`

Each named message is a pinned gate test with named failing output;
`tools/nc_t8.py` proves 7/7 mutations go red (the literal NC: the
notification-window control is mutant 3).
