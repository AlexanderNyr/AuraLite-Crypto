# AuraLite Legal Review — EU/Latvia perimeter (MiCA checklist)

**This document is an engineering checklist and not legal advice**
(decision D-T4). It records the disclosure perimeter of the AuraLite
research tree against Regulation (EU) 2023/1114 (MiCA) Articles 4, 6, 8
and 9 (the T8 gate), for a future public offer by a legal person. A
qualified legal review in the EU/Latvia must be obtained before any
offer; nothing here substitutes for it.

## Scope decision

AuraLite is a **research testnet**; there is no current offer to the
public and no admission to trading. If and when a public offer of
`ALCRY` (a crypto-asset other than an ART or an EMT, Title II MiCA) is
made, the offeror will be a legal person established in the EU
(Latvia — competent authority: Latvijas Banka) and the following
checklist applies.

## Checklist against MiCA Art. 4/6/8/9

| # | Requirement | Article | Status in this tree |
|---|---|---|---|
| 1 | Offeror is a legal person | 4(1)(a) | pending — required at offer time (model check: `NoLegalPerson`) |
| 2 | White paper drawn up: project, technology, risks, holder rights | 6 | **done** — `docs/whitepaper.md` (model check: `SectionMissing`) |
| 3 | Every claim traceable to a pinned model | 6 | **done** — `whitepaper.rs` claim pins (model check: `ClaimUnverified`) |
| 4 | Summary in non-technical language, ToC, notification date | 6(7)/(8) | **done** — white paper front matter |
| 5 | Official language of the home Member State | 6(9) | pending — EN draft; LV translation at notification |
| 6 | Machine-readable format (Annex) | 6 | pending — iXBRL rendering at notification |
| 7 | Notification to the competent authority ≥ 20 working days before publication | 8(1)/(5) | pinned — model constant `T8_NOTICE_WORKING_DAYS = 20` (model check: `NoticeTooShort`) |
| 8 | No prior approval required; explanation of Art. 2(4) non-exclusion | 8(3)/(4) | recorded here; formal text at notification |
| 9 | Publication on the website before the offer starts; published == notified | 9(1)/(2) | pinned — model check: `PublishedAfterOffer`, `VersionMismatch` |
| 10 | Marketing communications reference the published white paper | 7 / 4(1)(e) | **done** — `docs/marketing.md` (model check: `MarketingNotCompliant`) |
| 11 | Not-legal-advice disclaimer in the white paper and all materials | D-T4 | **done** — `T8_NOT_LEGAL_ADVICE` (model check: `NoDisclaimer`) |
| 12 | No income promises (no dividends/yield/expectation of profit) | D-T2 | **done** — white paper §3, §6; marketing disclaimer |

## Jurisdiction notes (EU / Latvia)

- MiCA is a Regulation: it applies directly in all Member States; the
  home competent authority of a Latvian offeror is Latvijas Banka.
- Notification (Art. 8) is at least **20 working days** before
  publication; the authority does not approve the paper beforehand.
- The white paper is a disclosure document, **not a prospectus** under
  the Prospectus Regulation.
- The research-tree unit `ALCRY` is a testnet accounting unit; D-T2:
  the project makes no income promises of any kind.

## Model enforcement

`crates/aura-storage/src/whitepaper.rs` turns every item above into an
executable check with named failing messages (see
`docs/economics/t8-whitepaper.md`); the pinned scenario drives 8
documents through the checklist with 7 negative controls
(`tools/nc_t8.py`), including the literal notification-window control.
