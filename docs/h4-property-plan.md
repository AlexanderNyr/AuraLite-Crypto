# H4 — Property / differential plan (1M tx conservation)

Status: 🚧 IN PROGRESS — H4
Owner: core maintainer
Date: 2026-11-16
Gate: `1M generated txs: sum(balances) conserved`

## 1. Scope and DoD

SECURITY_PLAN row H4: reference implementation of tree/codec, model-based VM
tests, balance invariants; the gate is 1,000,000 generated transactions
after which `sum(balances)` is conserved. The phase adds **properties** (in
the proptest sense, over generated inputs) and **differential** checks (an
independent reference implementation must agree with production
byte-for-byte).

Deliverables:

- **D-H4-1 — reference tree/codec.** Two independent references:
  (a) in-test naive Rust re-implementations of the canonical Tx byte layout
  and the S1 merkle hash rules, compared against production over proptest
  inputs (`crates/aura-testkit/tests/h4_properties.rs`);
  (b) a from-scratch pure-Python reference (`tools/h4_reference.py`,
  blake3 from the pinned W1 deps) cross-checked both ways by the campaign
  runner.
- **D-H4-2 — model-based VM tests.** A reference model of
  `aura_exec::detcheck::run` transfer semantics (accept = debit+credit+
  nonce bump; overdraft = refused with gas still billed) predicts
  committed/refused/gas for generated scenarios; the production runner must
  match exactly. Node-label independence: `run(steps, i)` must not depend
  on `i`.
- **D-H4-3 — balance invariants.** `Account::{credit,debit}` algebra:
  conservation (credit+debit keeps the total), rejection without state
  change, checked (never wrapping) arithmetic.
- **D-H4-4 — the 1M-tx gate.** `crates/aura-testkit/src/bin/h4-properties.rs`
  generates N deterministic transfer transactions (Lcg, seeds from
  20261180), each fully signed (`UnsignedTx::sign`) and verified
  (`Tx::verify_signature`), canonically round-tripped; after **every** tx
  the invariant `sum(balances) == accounts × start` is checked against the
  production `Account` ops. `--txs 1000000` is the gate run;
  `--txs 2000` is the cheap `make test` companion.
- **D-H4-5 — campaign runner.** `tools/h4_property_campaign.py`: recorded,
  resumable (h3 pattern: command, seed, source manifest before/after,
  `run.log` SHA-256, `result.json`); schedules `smoke` (200k txs) and
  `full` (1M txs); also runs the Python differential both ways.
- **D-H4-6 — mutation controls** (`tools/nc_h4.py`, 6): §5.

## 2. Properties (tests)

| test | property | NC |
| --- | --- | --- |
| `h4_balance_conservation` | generated transfer sequences conserve `sum(balances)`; rejects leave state unchanged | NC-1 |
| `h4_canonical_roundtrip` | `decode_canonical(encode(x)) == x`, re-encode reproduces bytes; non-minimal varints rejected; naive reference bytes == production bytes | NC-2 |
| `h4_signature_properties` | sign→verify ok; tampering any signed field (incl. expiry) is rejected; signer↔pubkey binding enforced | NC-3 |
| `h4_model_based_vm` | model-predicted committed/refused/gas == `detcheck::run` report; node-label independence | NC-4 |
| `h4_tree_differential` | naive S1 merkle reference == `MerkleTree::root()` over generated key/value sets | NC-5 |
| `h4_credit_debit_algebra` | credit/debit add/subtract exactly; overdraft rejected without mutation | NC-6 |

All tests are deterministic-plus-proptest (`proptest` 1.12.0, already the
pinned dev-dependency of aura-testkit; `failure_persistence: None`). The
deterministic part guarantees each mutation trigger fires.

## 3. Campaigns and deviation record

- `make property-h4` runs the in-tree tests (cheap gate, in `make test`).
  `make property-h4-full` runs the recorded 1M-tx campaign plus the Python
  differential.
- **Deviation (2026-11-16, session environment).** Closeout evidence
  records the `smoke` schedule (200k txs) actually executed here, plus the
  deterministic seed contract that makes `--txs 1000000 --seed S` a single
  parameter change on reference infrastructure. The **1M number is the
  gate**; this session's evidence reports the run it actually performed
  with exact counts — no extrapolation. Runtime note: every tx is signed
  and verified (no sampled signatures), so the campaign is wall-time bound
  by Ed25519 on one core (`CARGO_BUILD_JOBS=1`).

## 4. Crash triage

A failed property prints the violating case index and the seed; triage =
reproduce with `--seed S --txs N` (deterministic), minimise, fix with a
regression vector in the golden fixtures, re-run the campaign.

## 5. Gates and controls

- `tools/nc_h4.py` injects bugs (source mutations, restored after each
  control) and requires the H4 tests to go red with the named message:
  **NC-1 (named) injected bug → property finds it**: `Account::debit`
  silently shrinks a 7-unit amount ⇒ `H4: balance conservation violated`;
  NC-2 varint minimality guard disabled ⇒ `H4: canonical
  roundtrip violated`; NC-3 `signing_bytes` drops `expiry` ⇒ `H4:
  signature property violated`; NC-4 transfer path refuses amount 7 ⇒
  `H4: model/VM divergence`; NC-5 `node_hash` swaps left/right ⇒ `H4: tree
  differential mismatch`; NC-6 `Account::credit` inflates a 7-unit amount
  ⇒ `H4: credit/debit algebra violated`.
- Evidence `docs/evidence/h4/` via `make evidence-h4` (`gates.txt`,
  `nc.log` with `RESULT: 6/6 negative controls PASS`, `campaign/` bundle,
  `report.md`, `inventory.txt` cumulative TOTAL: 454, `SHA256SUMS`).
- The in-tree properties are part of `make test` (cheap, deterministic);
  the 1M campaign and the Python differential are recorded closeout
  artifacts (W1/H3 precedent).
