# W4 — Multisig: design of record

Status: implemented in `patches/0062-W4-multisig.patch` (see the W4 receipt in
`docs/plans/WALLET_PLAN.md`). This document is the design of record; deviations
land as edits here.

Gate (WALLET_PLAN row W4): *"3-of-5: 3 collected → success, 2 → reject; NC:
threshold bypass → red"*. The named control is the vector
`collect_2_of_5_rejected`: a bundle with fewer approvals than the threshold
must refuse (`error: multisig: threshold not met (2/3)`, exit 65). The
mutation control NC-1 disables the threshold check and requires that test to
go red.

## Model

A wallet-level M-of-N multisig contract over the W1 Ed25519 keys (the chain
level is B-series consensus/BLS; this is the wallet coordination layer, like
D-W2-2 ballots). Four steps, all deterministic documents:

1. **Contract (descriptor)** — `msig-init`: sorted member public keys, chain,
   threshold `M`, and a contract `timeout` (absolute height). The contract id
   is `BLAKE3` of its canonical bytes.
2. **Proposal** — `msig-propose`: a transfer (`to`, `amount`) or a **rotation**
   (a new descriptor — member rotation, D-W4-3), bound to the contract id,
   a proposal id and an `expiry` height capped by the contract timeout.
3. **Approval** — `msig-approve`: each member signs the canonical proposal
   bytes under the fixed AuraLite `contract` domain; the approval file embeds
   the member key, the proposal bytes and the signature (self-describing USB
   handoff).
4. **Collection** — `msig-collect`: verifies every approval (membership,
   proposal binding, signature) and enforces the threshold; `--height` makes
   the timeout deterministic (no clocks).

## Decisions

- **D-W4-1 — canonical documents.**
  `descriptor = "AuraLite-Crypto/wallet-msig-v1\0" || u8(len chain) || chain
  || u8 M || u8 N || N×32 sorted member pubkeys`;
  `proposal = "AuraLite-Crypto/wallet-msig-proposal-v1\0" || u8(len chain)
  || chain || contract_id(32) || LE64 id || LE64 expiry || u8 kind
  (0 transfer, 1 rotate) || payload`; transfer payload = `to(20) || LE64
  amount`; rotate payload = the new descriptor's canonical bytes. Files are
  JSON (`AuraLite-MSIG-V1` / `-PROPOSAL-V1` / `-APPROVAL-V1`) with fixed key
  order; signatures always cover the canonical bytes, never the JSON.
- **D-W4-2 — signature domain.** Each approval signs
  `BLAKE3(domain_message(chain, "contract", proposal))` with plain Ed25519 —
  the fixed AuraLite domain-separation construction (`Domain::Contract`); a
  signature under any other domain refuses (`error: signature verification
  failed`).
- **D-W4-3 — rotation.** Member rotation is a proposal of kind `rotate` whose
  payload is the new descriptor; it passes under the OLD membership rules and
  is recorded by the collector as a passed proposal. The new contract is
  simply the next `msig-init` state; nothing in this layer rewrites history.
- **D-W4-4 — timeouts and refusal set.** `--height` is required for
  `msig-approve`/`msig-collect` (deterministic expiry checks). Fixed refusals
  (exit 65): `error: multisig: threshold not met (K/M)`,
  `error: multisig: duplicate approval`, `error: multisig: not a member`,
  `error: multisig: proposal expired`, `error: multisig: approval for a
  different proposal`, `error: multisig: proposal for a different contract`,
  `error: signature verification failed`, plus the W2/W3 shared
  `error: foreign chain-id: tx=…, wallet=…`. Usage errors exit 64.

## Test gate

`tests/fixtures/w4_multisig_vectors.json` — 22 deterministic cases generated
by the independent oracle `tools/gen_w4_vectors.py` (pure Python: BLAKE3,
Ed25519 RFC 8032-checked, domain_message, BIP-39/SLIP-0010, bech32) and
asserted byte-for-byte by `tests/w4_multisig.rs` (22 vectors + a count guard).
Groups: init (4), propose (5), approve (4), collect (7), rotation/e2e (2).

Mutation controls (`tools/nc_w4.py`, 6): threshold bypassed (named control →
`collect_2_of_5_rejected`), signature verification skipped →
`collect_forged_signature`, duplicate approvals accepted →
`collect_duplicate_member`, membership check dropped → `approve_not_a_member`,
expiry check dropped → `approve_expired`, proposal-binding comparison dropped
→ `collect_foreign_proposal`.
