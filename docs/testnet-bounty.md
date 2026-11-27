# AuraLite public testnet — bug bounty policy

Scope: the public testnet (M2) surface — node, wallet/keys, storage,
WASM contract lane, RPC/faucet/explorer services, build and release
pipeline. This policy extends the H8 program in `SECURITY.md`; when the
two disagree, `SECURITY.md` wins.

## Rules

1. **Report privately first** (the disclosure contacts in
   `SECURITY.md`). Public testnet findings get a 90-day disclosure
   window; earlier publication is coordinated with the maintainers.
2. **No real-fund damage.** The testnet faucet funds have no value; do
   not attack exchanges, other users, or infrastructure outside the
   scope above.
3. **Safe harbor.** Good-faith research within scope will not trigger
   legal action; this includes the H8 re-audit registry
   (`tools/fixtures/h8_known_bugs.json`) — reintroducing a fixed bug is
   in scope and turns the gate red.

## Severity and triage

Findings are recorded in the public journal `release/triage.txt`
(`triage <id> <severity> <status>`). Severity classes follow the H8
convention (`critical`, `high`, `medium`, `low`, `informational`); the
published milestone gate counts **open `critical`/`high` findings** as
the "unfixed criticals" of `ROADMAP.md` §5 M2 and requires **zero** of
them (`aura-testnet triage`, named control
`unfixed_criticals_refused`).

## Response targets

- `critical`/`high`: triage within 72 hours, fix or mitigation plan
  published in the journal before the next release cut.
- `medium`: within 2 weeks.
- `low`/`informational`: best effort; recorded in the journal either
  way (status `fixed`, `accepted` or `not-applicable`).

## The journal is the receipt

Nothing in the milestone claim is asserted: the O4 gate closes on the
measured journal (`triage ledger: items=<n> open=<n> criticals=0`) and
the 30-day schedule receipts, all published under `docs/evidence/o4/`.
