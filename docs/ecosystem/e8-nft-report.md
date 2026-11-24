# E8 — NFT + names + DAO (phase report)

Date: 2026-11-22. Phase patch: `patches/0091-E8-NFT-DAO.patch`
(E7 → `patches/0090-E7-Lending.patch`). Aggregate digest
`39674d6a0813cb9beae5837025e1a8b6c00fb8ebe1bc620a06e844a5829fb313`.

## DoD and what was done

| Requirement | Status |
|---|---|
| ARC-721/1155 | ✅ the model `crates/aura-storage/src/nft.rs` (mint/transfer/approve/transfer_from/burn, per-id supply, operators, batch, URI) |
| `.aura` name service | ✅ register/renew/transfer/set_target/resolve/set_primary/reverse, a 365-day term + 90-day grace |
| on-chain identity | ✅ `set_identity`/`clear_identity`/`identity` — the meta digest is bound to a live name |
| governance UI | ✅ `crates/aura-storage/src/dao_ui.rs`: propose/vote/finalize/execute/veto through the controller, a receipt log, a self-contained HTML page |
| **50 standard vectors** | ✅ `tests/fixtures/e8-standards.txt` (generator `tools/gen_e8_vectors.py`), all 50 green, digest `8cb9da94…62bd3` |
| **first DAO vote through the UI** | ✅ `run_e8_scenario`: vote №1 (voter 1, yes, 150000 ALCRY) goes through the UI, a receipt is recorded, tally `250000/75000/25000` ALCRY, proposal → executed; the HTML page is identical in Rust and Python (sha `bf1486e2…1167f`) |

## The model

### ARC-721 (per `standards/arc721`, X5)

A token exists exactly when the owner slot is present —
a burned id can be minted again. **Every transfer (by the owner or an
approved spender) removes the approval slot** — the classic
stale-approval bug; vector `v013` pins it (after a transfer the old
approval is dead → `E8: not approved (dave/2)`). Mint/URI — owner-only
of the collection (`root`).

### ARC-1155

One contract — many ids; each id has its own supply (supply 1 = NFT
semantics, > 1 — fungible). The holder grants an operator
(`set_operator`) the right to move any of their ids; batch transfers are
all-or-nothing: every leg is checked before any is applied.

### `.aura` names (ENS form)

The registry holds: owner, resolve target, term. Registration is for
[`TERM_DAYS=365`] days; after expiry the name still resolves for
[`GRACE_DAYS=90`] (anyone may renew — as in ENSv2), then
`E8: name expired` and re-registration is allowed. Transferring a name
clears the old owner's primary name. The identity record (meta digest)
can be set only by the owner of a live name.

### Governance UI

`DaoUi` wraps the T6 `Governance` (threshold, 20% quorum, snapshot of
voting power, voting period, 48-hour timelock, veto with sunset).
**The path is UI-only**: every accepted vote yields a [`UiReceipt`] in
the action log; `render_html()` renders the DAO page (proposal cards,
tally, vote table) — reproduced byte-for-byte by the
independent Python model.

## The gate

1. **50 standard vectors** — a block-format fixture (`vector=vNNN`,
   `status=ok|err` with the exact literal `E8: …`, `read.<key>=<value>`).
   The Rust test and the Python model run it independently and both yield
   digest `8cb9da945b0eed8f4432d31b71d20fffb72a6317fc90204d1890bd480c062bd3`.
   The vectors cover: mint/duplicate id/nonexistent id/transfer by a non-
   owner/zero-address, approve→transfer_from, **stale-approval**,
   burn→re-mint, 1155 balances/operators/revocation/batch/length-
   mismatch/zero-amount, registration/taken name/invalid name/
   resolve/target/transfer/primary/reverse, renew on someone else's right,
   the expiry+grace boundaries (day 822 alive / day 823 dead / re-
   registration), identity set/update/clear/read.
2. **First DAO vote through the UI** — the scenario: propose(0) → **vote
   №1 through the UI** (1/1/yes/150000·ALCRY) → three more votes → a repeat
   vote rejected (`E8: vote already cast (1/1)`) → finalize (passed:
   quorum 120000·ALCRY, yes 250000 > no 75000) → execute after the
   timelock → `executed`. The steps and digest
   `17fd44850375c54c9cf9826b8ed55da0faf1855da72514184c83501f07409d71`
   are pinned in the Rust test and the Python model; the HTML page
   (`docs/evidence/e8/governance-ui.html`) is identical on both sides.

## Tests and metamorphisms

`crates/aura-storage/tests/e8_nft.rs` — 15 tests: all 50 vectors with
exact literals, digests, the gate scenario, the HTML page, exact
messages for all DAO errors (below threshold, voting closed/still open,
already finalized, timelock, already executed, not passed, no voting
power, proposal not found, veto sunset), metamorphisms (batch ≡
sequential transfers; mint-order independence; vote-order independence;
double-run determinism), the name rules.

## Negative controls

`tools/nc_e8.py` — 6 mutations with named messages (each weakens exactly
one literal, anchor count==1, after the run — restore + green):
`bad-mint-exists`, `bad-transfer-owner`, `bad-stale-approval`
(the not-approved message — the stale-path guard), `bad-name-taken`,
`bad-name-owner`, `bad-ui-vote` (the gate one). **6/6 RED + GREEN.**
Cumulative: **602 NC / 30 phases**.

## Sources

- **ERC-1155**: rareskills.io "ERC-1155 Explained" — one id with supply 1
  = NFT, mint outside the spec; eco.com "When to Use It" —
  `safeBatchTransferFrom`, `setApprovalForAll`, id patterns; webopedia
  "ERC Token Standards" and metana "Guide" — comparing 20/721/1155,
  batch gas.
- **ENS / name service**: gate.com «ENS Comprehensive Analysis» —
  registry/registrar/resolver, reverse resolution, annual renewal,
  grace period; chainscorelabs.com — the registry stores owner/resolver/TTL,
  names are NFTs (ERC-721); cryptoslate.com — a 90-day grace and
  a subsequent auction; docs.ens.domains (ETH Registrar v2) —
  renewal from any account, base rate with no premium.
- **Internal**: `standards/arc721/src/lib.rs` (X5: the method table,
  "exists = owner slot", transfers clear approvals), T6
  `crates/aura-storage/src/governance.rs` (the proposal lifecycle).

## Phase decisions

- **D-E6**: the 50 standard vectors are a block-format fixture in
  `tests/fixtures/e8-standards.txt` (counted by `tools/count_vectors.py`);
  the generator `tools/gen_e8_vectors.py` writes literal expectations,
  Rust and Python execute them independently.
- **D-E7**: `.aura` is ENS-shaped: a 365-day term, a 90-day grace (resolve
  stays alive; anyone may renew), re-registration after
  expiry+grace; transferring a name clears the old owner's primary.
- **D-E8**: the governance UI is the only voting path (DaoUi
  wraps the T6 DAO and journals receipts); the DAO HTML page
  is byte-identical in Rust and Python (cross-checked by sha).
