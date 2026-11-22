# AuraLite Marketing Materials

**Rules of use (MiCA Art. 7 / Art. 4(1))**

1. Every communication references the published white paper
   (`docs/whitepaper.md`) and this rules file.
2. Communications are disseminated **only after** the white paper is
   published; nothing here is disseminated before publication.
3. Every communication carries the disclaimer below verbatim and makes
   **no income promises** (decision D-T2).
4. Communications are factual; they do not exaggerate claims or omit
   material risks.

**Disclaimer (mandatory in every communication)**

> AuraLite is a research testnet. This material references the AuraLite
> white paper and is not legal advice and not an investment offer. The
> testnet unit ALCRY has no financial rights and no promise of income,
> profit or value.

---

## Template A — short description (non-technical)

AuraLite is a small, auditable proof-of-stake testnet with a fixed
emission schedule and fully pinned economic parameters. Every claim
about the protocol is checked by executable models and mutation
controls in the public repository. Details and risks: see the white
paper `docs/whitepaper.md`. *Disclaimer (as above).*

## Template B — technical one-liner

AuraLite: deterministic PoS testnet — Ed25519 messages, closed emission
(6M ALCRY cap), EIP-1559-style base fee, council governance with a
2-of-3 treasury multisig; white paper: `docs/whitepaper.md`. *Disclaimer
(as above).*

## Template C — event/conference description

AuraLite presents an engineering-first approach to crypto-asset
networks: small code, pinned economics, evidence packs and negative
controls for every phase. The white paper
(`docs/whitepaper.md`) lists the project, technology, risks and holder
rights, with every quantitative claim traceable to a pinned model.
*Disclaimer (as above).*

---

## Compliance notes

- The white paper machine (`crates/aura-storage/src/whitepaper.rs`)
  enforces `Marketing { references_wp, after_publication }`; a
  communication without a reference or before publication turns the
  checklist red (`T8: marketing communications must reference the
  published white paper`).
- Competent authorities may request these communications (Art. 8(2));
  they are kept in version control for that purpose.
