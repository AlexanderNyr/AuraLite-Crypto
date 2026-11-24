# E2 report — chain explorer

Date: 2026-11-19. Phase **E2** per ECOSYSTEM_PLAN (the "Explorer" row):
Rust backend + web UI: blocks/tx/accounts/validators/contracts, verified
source. The phase gate: **10 user scenarios green; reproducible contract build =
same code_hash**.

## What a user must be able to do (10 scenarios)

<!-- E2: scenarios -->

| ID | Scenario | What a user must be able to do | Gate |
|---|---|---|---|
| E2S-01 | Open the home page | see all sections: blocks, transactions, accounts, validators, contracts, search | `E2: home page must show blocks, tx, accounts, validators, contracts and search` |
| E2S-02 | Browse recent blocks | see the latest canonical blocks exactly as in the chain indexer | `E2: explorer must read blocks from the indexer` |
| E2S-03 | Open a block by height or hash | reach the same block page by height and by hash | `E2: block lookup by height and hash must agree` |
| E2S-04 | Find a transaction and its status | see confirmed/finalized/orphaned/unknown per the canonical chain | `E2: tx status must follow the canonical chain` |
| E2S-05 | Account balance and history | view the balance and past transactions without intermediaries | `E2: account history must match the chain` |
| E2S-06 | Take a transaction to finality | see one unambiguous status word | `E2: transaction finality must be unambiguous` |
| E2S-07 | Validators and consensus health | see the validator set, participation and network health | `E2: validator view must show consensus health` |
| E2S-08 | Search everything | enter a height, hash, address or contract address and land on the object | `E2: search must find blocks, txs, accounts and contracts` |
| E2S-09 | Verified contract source | read the verified source matching the deployed code_hash | `E2: contract page must show the verified source` |
| E2S-10 | Page = API | see in the page the same data the JSON API returns | `E2: the web UI must render the same data as the API` |

**Readiness: 10/10 ready.**

Traceability: E2S-05 = US-03, E2S-06 = US-04, E2S-07 = US-05, E2S-09 = the
verified-source step of US-07 (registry `tools/fixtures/e0_user_scenarios.json`,
the E0 report). The scenarios are served by the explorer, which reads blocks
**only** from the E1 indexer (decision D-E1).

## Reproducible contract build

Contract builds are deterministic (the ALVM-1 format, the assembler in
`crates/aura-storage/src/explorer.rs`): source canonicalization
(whitespace/case/comments) → identical code bytes → identical
`code_hash`. The independent reference (the Python assembler in
`tools/e2_explorer_check.py`) recomputes `tools/fixtures/e2_contracts.json`:
`adder` and its alias `  PUSH   003  …` produce one hash
`64e3e32d…d0b0d`, `counter` — `b17231dd…d412a`. The property in the test:
`E2: contract build must be reproducible (same code_hash)`; the check
verified source: `E2: verified source must match the deployed code_hash`.

## How it works

- Backend: `crates/aura-storage/src/explorer.rs` — read-only views
  (blocks, transactions, accounts, validators, contracts) over
  `ChainIndexer`; transaction statuses derive from block canonicity
  (finalized at ≥8 confirmations).
- Web UI: a single page `render_page()` + JSON routes
  (`/blocks`, `/block/<h>`, `/tx/<hash>`, `/account/<addr>`,
  `/validators`, `/contracts`, `/contract/<addr>`, `/search?q=`,
  `/health`), served by the stdlib HTTP server `explorer::serve`
  (no new dependencies — Cargo.lock is pinned by H1).
- Gate: `crates/aura-storage/tests/e2_explorer.rs` — 10 scenarios through
  real HTTP requests + 2 properties; mutation controls
  `tools/nc_e2.py` (6/6).
