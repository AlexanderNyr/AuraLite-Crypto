# E0 — Baseline: what a user must be able to do

Date: 2026-11-18 · Phase: E0 (ECOSYSTEM_PLAN) · Patch: `patches/0074-E0-baseline.patch`
Gate: **checklist of 10 user scenarios (0/10 so far)** — the checklist
itself must be complete, unique and honest; E2 turns the list green.

## Why

A working chain with no applications is a lab bench. Before building the
indexer/explorer/faucet/SDK (E1–E3) we pin down what a *user* must be
able to do, so every later phase has a named, testable target and the
ecosystem cannot silently serve only the author's machine.

## The 10 user scenarios

<!-- E0: scenarios -->

| # | Scenario | What a user must be able to do | Surface | Status |
|---|---|---|---|---|
| US-01 | Create and protect a wallet | create or import a wallet, encrypt keys on disk and never expose the seed | wallet | not-ready |
| US-02 | Send and receive AURA | transfer funds with the fee shown before signing and a clear result after | wallet | not-ready |
| US-03 | See balance and history | look up an account balance and past transactions without trusting a third party | explorer | not-ready |
| US-04 | Follow a transaction to finality | submit a transaction and watch it reach finality with unambiguous status | explorer | not-ready |
| US-05 | Inspect validators and consensus health | see the validator set, participation and finality health of the network | explorer | not-ready |
| US-06 | Get testnet funds from a faucet | request testnet AURA with rate limits and no manual begging | faucet | not-ready |
| US-07 | Deploy and verify a contract | deploy a contract and verify its source so others can read what runs | contracts | not-ready |
| US-08 | Build with an SDK | integrate via Rust, TypeScript or Python SDK examples that pass in CI | sdk | not-ready |
| US-09 | Stake, delegate and track rewards | delegate to a validator and see rewards and slashing outcomes | wallet | not-ready |
| US-10 | Verify releases and report incidents | verify signed releases and know exactly how to report a security problem | security | not-ready |

Readiness: 0/10 ready.

Machine-readable copy: `tools/fixtures/e0_user_scenarios.json`;
oracle: `tools/e0_baseline_check.py` (wired into `make test`).

## Traceability

- US-01/US-02/US-09 rest on W1–W4 (wallet) and H7 (keystore hygiene).
- US-03–US-05 are E2's acceptance scenarios (explorer reads only the
  E1 indexer — decision D-E1).
- US-06 is E3 (faucet, testnet only, rate-limited).
- US-07 rests on X3–X6 (contracts) and decision D-E2 (verifiable source,
  reproducible WASM builds).
- US-08 is E3 (SDK + docs).
- US-10 is already green in spirit (H1 sign_release, SECURITY.md
  disclosure) but the *surface* (a documented user-facing path) lands
  with E2/E3 — status stays honest until then.

## What "ready" means

A scenario flips to `ready` only when its surface ships and its named
test gate passes (E2: "10 user scenarios green"); until then the count
in this report must match the registry exactly (oracle check).
