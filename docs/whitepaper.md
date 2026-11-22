# AuraLite Crypto-Asset White Paper

**Document status**: engineering disclosure draft (testnet, W1-FORMAT-v1
frozen). **This document is an engineering instrument and not legal
advice** (decision D-T4); see `docs/legal-review.md` for the compliance
perimeter and `docs/ecosystem/disclaimer.md` for the standing
disclaimer.

**Notification date (Art. 8)**: recorded at notification time.
**Table of contents**: summary; 1. Project; 2. Technology; 3. Risks;
4. Holder rights; 5. Claims traceable to the pinned models; 6. No
income promises; 7. Disclaimer.

---

## Summary (Art. 6(7), non-technical)

AuraLite is a research testnet for a small, auditable proof-of-stake
ledger with a fixed emission schedule, cheap verifiable transactions, a
treasury governed by council votes and a 2-of-3 multisig, and fully
pinned economic parameters. Everything this paper states about the
protocol is checked by executable models and negative controls in the
public repository. The native unit `ALCRY` is an abstract testnet
accounting unit; **nothing in this paper is a promise of income, profit
or value** (decision D-T2).

## 1. Project

AuraLite is an engineering project: a minimal crypto-asset network whose
consensus, execution and economic layers are small enough to be read in
full. The roadmap is executed in pinned phases (CORE, STATE, EXEC,
CONSENSUS, NET, NODE, WALLET, ECOSYSTEM, ECONOMICS and more), each
closed with patches, evidence packs and mutation controls. The offeror
of any future public offer must be a legal person (Art. 4(1)(a)); this
research tree is not itself an offer to the public.

## 2. Technology

- **Consensus / state**: deterministic state machine with signed
  messages (Ed25519) and strict message validation; see
  `docs/ARCHITECTURE.md`, `docs/spec/`.
- **Economics**: closed emission from a genesis supply with halvings
  (`crates/aura-storage/src/issuance.rs`), EIP-1559-style base fee
  (`fees.rs`), allocation (`allocation.rs`), staking (`staking.rs`),
  slashing (`slashing.rs`), governance (`governance.rs`), treasury
  (`treasury.rs`).
- **Determinism**: every economic phase ships an independent Python
  oracle and a golden digest pinned in the Rust tests; the digests are
  part of the evidence packs under `docs/evidence/`.

## 3. Risks

- **Protocol risk**: consensus or implementation bugs; mitigated by
  fuzzing, property tests, mutation controls and independent review
  (`docs/WIP_SECURITY_AND_W1.md`).
- **Economic risk**: parameter choices (emission, fees, slashing) can be
  wrong; all parameters are visible, pinned and changeable only through
  governance with a timelock (`docs/economics/t6-governance.md`).
- **Regulatory risk**: the legal treatment of crypto-assets differs by
  jurisdiction; see `docs/legal-review.md` (EU/MiCA perimeter). **This
  is not legal advice.**
- **Operational risk**: keys, multisig and treasury operations
  (`docs/economics/t7-treasury.md`).
- **No income promises**: the asset is a testnet accounting unit; there
  are no dividends, yields, buy-backs or any expectation of profit
  (decision D-T2).

## 4. Holder rights

AuraLite testnet units carry **no financial rights**: no redemption, no
interest, no governance over real-world entities. Protocol-level
rights are technical: holding a balance, transacting, staking and
voting in protocol governance exactly as the pinned code allows. Any
future public offer will define holder rights in its own notified white
paper (Art. 6 content, Art. 8 notification).

## 5. Claims traceable to the pinned models

| Claim | Value | Pinned model |
|---|---|---|
| Maximum supply | 6 000 000 ALCRY | `issuance::MAX_SUPPLY` |
| Genesis supply | 3 000 000 ALCRY | `issuance::GENESIS_SUPPLY` |
| Initial base fee | 1 000 000 000 | `fees::T3_INITIAL_BASE_FEE` |
| Proposal threshold | 10 000 ALCRY | `governance::T6_PROPOSAL_THRESHOLD` |
| Treasury multisig threshold | 2 of 3 | `treasury::T7_SIGNER_THRESHOLD` |

Every claim above is machine-checked by
`crates/aura-storage/src/whitepaper.rs` (`ClaimKind::pin`) against the
model constants; an unverifiable claim turns the gate red (named
message `T8: every white paper claim must be traceable to a pinned
model`).

## 6. Marketing communications

Marketing materials live in `docs/marketing.md`. They reference this
white paper, carry the same disclaimer, make no income promises and are
only disseminated **after** this paper is published (Art. 7; Art. 4(1)
last paragraph).

## 7. Disclaimer

**This document is an engineering instrument and not legal advice.**
No communication within this project constitutes an offer to the public,
an investment recommendation or a promise of income. Compliance work in
`docs/legal-review.md` is an engineering checklist, not a legal
opinion.
