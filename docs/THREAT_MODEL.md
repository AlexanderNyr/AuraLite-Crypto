# AuraLite threat model (H0)

Scope: the AuraLite-Crypto repository as of the Stage 1 close (phases C/S/X/B/
N/D shipped, wallet W0–W4 shipped; W5–W7 and H8–H9 are Stage 2). Every
scenario below has exactly one owner; an unowned scenario fails the review
gate (`make check-h0`, control NC-1 in `tools/nc_h0.py`). Document format is
fixed in §6 so the gate can parse this file deterministically.

## 1. Assets

### A1 — Wallet root secrets

Entropy and seeds used to derive wallet keys (including BIP-39 mnemonics).

### A2 — Keystore files and passwords

Argon2id-encrypted keystore documents (`wallet-v1`), their passwords and
`--password-file` inputs.

### A3 — Offline-signing keys on the air-gapped device

Private keys and decoded payloads on the machine that signs
(W1-FORMAT-v1 / W3 offline workflow).

### A4 — Validator and node keys plus chain state

Node private keys, validator signing keys, database state and the mempool of a
running devnet node.

### A5 — User funds (on-chain balances)

Balances and allowances that transactions can move.

### A6 — Release artifacts and toolchain

Repository tree, patches, toolchains, Python dependency wheels and the
reproducibility of builds (`patches/NNNN-*`, evidence bundles).

### A7 — Transaction intent integrity

The exact bytes a human approved: chain id, kind (`tx`/`vote`/`block`/
`contract`/`genesis`), amounts, destinations, expiry.

### A8 — Backups, evidence and receipts

Workspace backup tarballs, `SHA256SUMS`, receipts and audit trails
(`docs/evidence/**`).

## 2. Adversaries

### ADV-EXT — External network attacker

No repository access; controls network paths, phishing sites, malicious
inputs over RPC/p2p.

### ADV-VAL — Byzantine validator

Controls validator keys within the consensus set; can double-sign, censor,
reorder.

### ADV-INS — Insider

Maintainer or operator with partial trust (repo access, CI access, node
access) acting maliciously or being coerced.

### ADV-SUP — Supply chain attacker

Compromises dependencies, toolchains, release artifacts or generated
fixtures before they reach users.

### ADV-PHY — Physical attacker

Device seizure, evil-maid, key exfiltration by hardware access. Physical
security of validators is documented as a requirement for O6 (SECURITY_PLAN
non-goals) — scenarios below cover only the wallet/human side.

## 3. Trust boundaries

### B1 — User ↔ offline wallet device

Human eyes and hands approving what the device signs.

### B2 — Offline device ↔ online transport

QR codes / payload files crossing the air gap in both directions.

### B3 — Wallet ↔ RPC node

Online submission and queries (fees, nonce, chain id, status).

### B4 — Node ↔ node

p2p frames, consensus votes, block propagation.

### B5 — Maintainer ↔ build & release pipeline

Commits, CI, patches, toolchains, published artifacts.

### B6 — User ↔ keystore & backup storage

Where encrypted keys, passwords and backups live at rest.

## 4. Scenarios

### H0-S01 — Weak or reused entropy at keygen

- Class: ADV-EXT
- Boundary: B1
- Assets: A1
- Entry: A user generates keys with predictable entropy (zeroed test vectors,
  reused seeds, low-entropy `--entropy-file`).
- Impact: Full key compromise; funds and identity stolen.
- Owner: wallet maintainer
- Mitigations: `new`/`restore` require entropy bytes (`--entropy-file` is
  explicit and hex-decoded); W0 vector suite pins key derivation; documentation
  forbids test vectors in production.
- Residual: A user can still paste weak entropy manually.

### H0-S02 — Offline brute force of a stolen keystore

- Class: ADV-EXT
- Boundary: B6
- Assets: A2
- Entry: An attacker obtains `wallet-v1` keystore files from a backup or disk
  and runs offline guessing (hashcat-style).
- Impact: Account compromise if the password is weak.
- Owner: wallet maintainer
- Mitigations: Argon2id `m=65536,t=3,p=4` (`KdfParams`), password minimum
  length 8, `password < len 8` refused with `error: usage: password must be
  at least 8 characters` (W1); keystore tests pin the KDF parameters.
- Residual: Weak-but-compliant passwords remain guessable at lower cost.

### H0-S03 — Phishing password through a fake workflow

- Class: ADV-EXT
- Boundary: B1
- Assets: A2, A3
- Entry: A fake "wallet" tool or site asks for the keystore password or seed
  phrase (W1 review abuse case "phishing password").
- Impact: Credential theft, full account compromise.
- Owner: wallet maintainer
- Mitigations: The wallet never prints seeds (`list` prints `entry=N`,
  `address=`, `public_key=` only — W0 log-scan NC); password is only read via
  `--password-file`, never argv; documentation warns users.
- Residual: Users can still be tricked into typing a password into hostile
  software.

### H0-S04 — Malicious payload swapped in transit (QR / file)

- Class: ADV-EXT
- Boundary: B2
- Assets: A7, A5
- Entry: While the user moves the encoded payload across the air gap, the
  attacker swaps it for a payload paying a different destination (W1 review
  case "malicious payload swap").
- Impact: The user signs a transaction they never intended.
- Owner: wallet maintainer
- Mitigations: W3 shows `chain/kind/fee/note` before signing and binds the
  signature to the decoded bytes (`payload_hash`); W1-FORMAT-v1 summary UX
  shows `kind`, `signer`, `chain`, `amount`, `destination`, `fee`, `nonce`,
  `memo`; envelope trailer checksum detects accidental truncation.
- Residual: A user who approves without reading the summary is still exposed —
  mitigated operationally by the H0 review gate training.

### H0-S05 — Submitting an offline signature to the wrong network

- Class: ADV-EXT
- Boundary: B3
- Assets: A5
- Entry: An envelope produced for `auralite-test-1` is replayed against a
  different network's RPC.
- Impact: Replay of a signed intent on another chain.
- Owner: core maintainer
- Mitigations: `submit` rechecks the chain id and refuses cross-chain submit
  (`error: refused: cross-chain submit: tx=…, wallet=…`, W2); signatures bind
  the chain id through `domain_message` (W1).
- Residual: None known for cross-chain replay within AuraLite.

### H0-S06 — Signature domain confusion (kind reuse)

- Class: ADV-EXT
- Boundary: B2
- Assets: A7
- Entry: An attacker takes bytes signed for one kind (e.g. `vote`) and
  presents them as another kind (`tx`) to a verifier that forgets the domain.
- Impact: Forgery of a class of messages from one valid signature.
- Owner: core maintainer
- Mitigations: All signatures cover `chain || kind || message`
  (`domain_message`); W3 named control `verify_foreign_domain` proves a
  foreign domain fails `error: signature verification failed`; the frozen
  W1-FORMAT-v1 separates kinds and addresses.
- Residual: New code paths that add signatures must call `verify_raw` with the
  same domain construction (enforced by reviews).

### H0-S07 — RPC node lies about fees or nonce

- Class: ADV-EXT
- Boundary: B3
- Assets: A5, A7
- Entry: A malicious node returns inflated `max_fee` estimates or wrong nonce
  to make the user overpay or stall.
- Impact: Fee theft, stuck transactions.
- Owner: node operator
- Mitigations: W2 `encode`/`sign` enforce `--tip must not exceed --max-fee`;
  fee display is part of the summary the user approves; node software
  (D-phases) logs fee fields.
- Residual: A user who blindly accepts suggested fees is still exposed.

### H0-S08 — Denial of service against the RPC endpoint

- Class: ADV-EXT
- Boundary: B3
- Assets: A4
- Entry: Flooding the node RPC (D5 limit: 100 req/s per IP) to exhaust CPU,
  disk or the mempool.
- Impact: Node unavailability; mempool grows to 10,000 tx.
- Owner: node operator
- Mitigations: D5 rate limiting with the bounded mempool; p2p frame limits
  (N1 `MAX_FRAME`, handshakes); operational runbooks (H9 later).
- Residual: Distributed floods can still degrade a single node.

### H0-S09 — Hostile p2p frames / eclipse attempt

- Class: ADV-EXT
- Boundary: B4
- Assets: A4
- Entry: A peer sends oversized or malformed frames, or surrounds the node to
  eclipse its view (W2 review case "hostile rpc peer frame").
- Impact: Node crash, stale view, delayed consensus.
- Owner: core maintainer
- Mitigations: N1 bounded frames and handshake checks; N6/N7 verification
  chains; node restart drills (N8) cover recovery; W2 vectors pin frame
  rejection paths.
- Residual: Small networks remain eclipse-prone by topology.

### H0-S10 — Validator double-signing

- Class: ADV-VAL
- Boundary: B4
- Assets: A4, A5
- Entry: A validator (or its stolen key) signs two conflicting blocks at the
  same height.
- Impact: Consensus safety violation; chain halt or fork.
- Owner: validator operator
- Mitigations: B-series consensus rules (commit-verify, unlock semantics);
  `aura-signer` hardware workflow with slot height binding (B11 artifact
  spec); slashing/double-sign logging planned (H6).
- Residual: Key theft before detection (T9 test-keys case).

### H0-S11 — Validator censorship / reordering

- Class: ADV-VAL
- Boundary: B4
- Assets: A5, A7
- Entry: A block proposer omits or reorders transactions (W2 review case
  "validator censorship").
- Impact: Selective denial, MEV-style extraction.
- Owner: validator operator
- Mitigations: Rotating proposers (B-series round-robin determinism),
  proposal hash visibility (`BlockCommitVerified`), mempool ordering rules;
  economic analysis planned (H6).
- Residual: Bounded reordering inside a proposal window.

### H0-S12 — Long-range or checkpoint rewrite

- Class: ADV-VAL
- Boundary: B4
- Assets: A4, A8
- Entry: An attacker with old validator keys rewrites history from a snapshot
  (W2 review case "long-range snapshot").
- Impact: Users following an old checkpoint accept a forged chain.
- Owner: core maintainer
- Mitigations: Explicit handoff receipts with `next_validator_root` (W2
  vectors `receipt_round_7_*`); snapshot hashing in the DB; external review
  gate before trusting weak subjectivity parameters (H8).
- Residual: Weak subjectivity remains a social assumption for light clients.

### H0-S13 — Node key theft from the host

- Class: ADV-INS
- Boundary: B6
- Assets: A4
- Entry: An operator account or CI job reads node key files from disk/backups.
- Impact: Node impersonation, validator key misuse if co-hosted.
- Owner: node operator
- Mitigations: `init` refuses unsafe configurations (`error: refused:
  unsafe configuration`); key files documented as 0600; H7 will add HSM
  guidance and CI isolation.
- Residual: Host root is always game over.

### H0-S14 — Genesis ceremony forgery

- Class: ADV-INS
- Boundary: B5
- Assets: A4, A7
- Entry: A maintainer swaps genesis bytes or a ceremony parameter between
  the reviewed text and the executed artifact (W2 review case "genesis
  ceremony forgery").
- Impact: Network starts with hostile rules or balances.
- Owner: release manager
- Mitigations: D-phases pin the genesis document and validator root; the
  ceremony checklist (D-phase docs) demands N-of-M independent sha256
  confirmations; `domain_message` genesis kind separates genesis signatures.
- Residual: Colluding ceremony participants (threshold assumption).

### H0-S15 — Dependency or toolchain compromise

- Class: ADV-SUP
- Boundary: B5
- Assets: A6
- Entry: A crates.io/npm/PyPI dependency or a toolchain update ships malicious
  code that alters builds or fixtures.
- Impact: Backdoored binaries; fabricated evidence.
- Owner: release manager
- Mitigations: Rust builds use a pinned toolchain (1.99.1); `Cargo.lock` is
  committed; generated fixtures are re-derivable from committed oracles
  (`tools/gen_*_vectors.py`) and pinned in `SHA256SUMS`; H1 will add
  `cargo-deny`/`cargo-audit`/SBOM/gitleaks.
- Residual: Undetected upstream compromise before H1 automation.

### H0-S16 — Compromised patch or fixture in review

- Class: ADV-SUP
- Boundary: B5
- Assets: A6, A8
- Entry: An attacker submits a patch that subtly weakens a check (e.g. edits
  an oracle) while keeping tests green (W2 review case "compromised patch in
  review").
- Impact: Silent regression of security properties.
- Owner: audit reviewer
- Mitigations: Named mutation controls (`tools/nc_*.py`) pin that each gate is
  load-bearing; negative-control counts are audited (`tools/audit_closeout.py
  --strict`); phase patches carry receipts; source drift is recorded
  explicitly (`docs/evidence/review-w1-source-drift.txt`).
- Residual: Reviewer bandwidth; social engineering of reviewers.

### H0-S17 — Evidence tampering or rollback

- Class: ADV-INS
- Boundary: B5
- Assets: A8
- Entry: Someone edits `docs/evidence/**` or replaces a backup tarball to
  hide a failure.
- Impact: False confidence in a broken build.
- Owner: security officer
- Mitigations: Every evidence bundle has `SHA256SUMS` over sources and logs;
  audit_closeout verifies receipt/patch/NC-log/evidence coherence per phase;
  W-tarballs are sha256-pinned in `*-tarball-sha256.txt` (verified in each
  restore drill).
- Residual: An insider with full workspace access can rewrite history.

### H0-S18 — Build non-reproducibility hides a swap

- Class: ADV-SUP
- Boundary: B5
- Assets: A6
- Entry: Two honest builders get different binaries; a hostile build slips in
  unnoticed because nothing pins the output.
- Impact: Users run a binary nobody audited.
- Owner: release manager
- Mitigations: Deterministic derivations are pinned as vectors (W-series);
  H7 will add two-host reproducible builds with matching sha256; toolchains
  pinned.
- Residual: Rust/Cargo reproducibility is not yet enforced in CI (H7).

### H0-S19 — Keystore password in argv or logs

- Class: ADV-INS
- Boundary: B6
- Assets: A2
- Entry: A password is passed as a CLI argument (visible in `ps`, shell
  history) or accidentally logged.
- Impact: Credential leak on a shared host.
- Owner: wallet maintainer
- Mitigations: The CLI takes `--password-file` only (never a password
  argument); W0 log-scan control (`MnemonicInLog`) proves secrets are not in
  outputs; make logs pass the same scan.
- Residual: A hostile local process can read the password file.

### H0-S20 — Malicious keystore file from a third party

- Class: ADV-EXT
- Boundary: B6
- Assets: A2, A3
- Entry: A user imports an attacker-supplied keystore (e.g. a "recovery"
  service) that contains attacker-known keys or a hostile KDF downgrade.
- Impact: The user stores funds on attacker-controlled keys.
- Owner: wallet maintainer
- Mitigations: `import`/`restore` recompute keys from the secret and print the
  derived `address=` for user comparison; keystore KDF parameters are
  validated on read (W1 refactor pinned them); `wallet-v1` JSON format is
  fixed.
- Residual: Users may not compare addresses.

### H0-S21 — Lost or wrong password bricks the account

- Class: ADV-EXT
- Boundary: B6
- Assets: A2
- Entry: The user forgets the password or corrupts the keystore (W2 review
  case "lost password bricks account").
- Impact: Permanent loss of access to funds.
- Owner: wallet maintainer
- Mitigations: Standard exit 69 `error: keystore: …` taxonomy keeps failures
  explicit; documentation demands backups of keystore + password hints;
  H9 incident playbook will cover recovery paths.
- Residual: No recovery without the password (by design).

### H0-S22 — CLI misuse sends funds to a wrong address

- Class: ADV-EXT
- Boundary: B1
- Assets: A5, A7
- Entry: A user typos a destination or misunderstands units (W2 review case
  "misuse sends funds wrong address").
- Impact: Irreversible transfer to the wrong account.
- Owner: wallet maintainer
- Mitigations: The W1 summary UX shows `amount` and `destination` before
  signing; address checksums (`wallet-address-v1`) reject mistyped strings;
  `show` verifies decoded payloads before human approval.
- Residual: Users can confirm the wrong address (H1-human factor training).

### H0-S23 — Multi-approval bypass in the multisig contract

- Class: ADV-INS
- Boundary: B1
- Assets: A5, A7
- Entry: An insider constructs a bundle that claims threshold approvals
  without real member signatures (W4 review case "skip multi-approval").
- Impact: Theft of shared funds with a fake quorum.
- Owner: wallet maintainer
- Mitigations: `msig-collect` verifies every approval signature and
  membership and enforces `threshold` (D-W4-1..4); the named W4 control
  proves `error: multisig: threshold not met (2/3)` is load-bearing;
  approvals bind the exact proposal and contract ids.
- Residual: A quorum of real members can always act (governance assumption).

### H0-S24 — Membership stays stale after rotation

- Class: ADV-INS
- Boundary: B1
- Assets: A5
- Entry: A removed member keeps signing proposals that old tooling still
  accepts (W4 review case "membership stays stale").
- Impact: Ex-member moves funds after removal.
- Owner: wallet maintainer
- Mitigations: Rotation is a `rotate` proposal validated against the new
  descriptor (D-W4-3); the contract id changes with membership, and every
  approval binds the contract id (`error: multisig: proposal for a different
  contract`).
- Residual: Off-chain tooling caching old contract ids must re-resolve.

### H0-S25 — Proposal replay after expiry

- Class: ADV-EXT
- Boundary: B1
- Assets: A5, A7
- Entry: An attacker replays approvals for a proposal after its expiry or on
  another collection run (W4 review case "proposal replay after expiry").
- Impact: Execution of stale intents.
- Owner: wallet maintainer
- Mitigations: Proposals carry `expiry` and `--height` checks refuse stale
  proposals (`error: multisig: proposal expired`); approvals bind the
  proposal id and contract id; a second collect of the same bundle is a
  duplicate (`error: multisig: duplicate approval`).
- Residual: Replay protection across external execution layers belongs to
  the future on-chain multisig (H6/H8 review).

### H0-S26 — Human approves what they did not read

- Class: ADV-EXT
- Boundary: B1
- Assets: A5, A7
- Entry: Fatigue or urgency makes a user confirm a summary that does not match
  their intent (W1 review case "human confirms what they did not read").
- Impact: Signed hostile intent.
- Owner: wallet maintainer
- Mitigations: Fixed summary fields (kind, signer, chain, amount, destination,
  fee, nonce, memo) require explicit `--i-understand-testnet-only` on testnet
  commands; human-denied flow is a first-class path (`refused: human denied`).
- Residual: Confirmation bias cannot be engineered away; W5 hardware
  confirmation (Stage 2) targets exactly this scenario.

### H0-S27 — KDF parameters downgraded on import

- Class: ADV-SUP
- Boundary: B6
- Assets: A2
- Entry: A hostile tool rewrites a keystore with weaker Argon2id parameters so
  that later cracking becomes cheap.
- Impact: Cheap offline brute force after theft.
- Owner: wallet maintainer
- Mitigations: W1 refactor output pinned `KdfParams { m: 65536, t: 3, p: 4,
  salt: 32 bytes }`; keystore reads validate KDF fields; W1 fuzz vectors
  stress decode paths.
- Residual: Out-of-repo tools can still write weak files.

### H0-S28 — QR payload framing corruption

- Class: ADV-EXT
- Boundary: B2
- Assets: A7
- Entry: Chunked QR codes (`ALO1:ii/nn:chunk`) are re-ordered, truncated or
  mixed between two sessions (W1 review case "payload corrupt QR framing").
- Impact: Garbage or hostile bytes are decoded and possibly signed.
- Owner: wallet maintainer
- Mitigations: Decoder verifies index/count framing, single-session reassembly
  and the envelope trailer checksum; `parse` refuses bad framing with exit 69
  `error: bad framing`; W3 vectors pin corrupted-encoding refusals.
- Residual: A hostile re-framing that preserves checksums requires finding a
  BLAKE3 collision (assumed infeasible).

### H0-S29 — Snapshot chain evidence fake

- Class: ADV-INS
- Boundary: B5
- Assets: A8
- Entry: An operator fabricates a snapshot chain (W2 review case "snapshot
  chain evidence fake") to make a broken state look checkpointed.
- Impact: False recovery point; rollbacks to hostile state.
- Owner: audit reviewer
- Mitigations: Snapshot hash chain is pinned in W2 vectors
  (`snapshot_chain_broken_link` refuses broken links); receipts carry
  `snapshot_root`; evidence bundles pin generator hashes.
- Residual: A collusion of snapshot producers can fake their own chain.

### H0-S30 — Test scaffolding leaks into production

- Class: ADV-INS
- Boundary: B5
- Assets: A6
- Entry: `insecure-test-kdf`, testing entropy or fixtures accidentally ship in
  a release profile (W1 review case "test scaffolding leaks").
- Impact: Known-weak keys or parameters in production wallets.
- Owner: release manager
- Mitigations: `KdfParams::testing` is gated behind `cfg(feature =
  "insecure-test-kdf")`; tests are `#![cfg(feature = "insecure-test-kdf")]`;
  release checklist (WIP_SECURITY_AND_W1.md) forbids the feature outside
  tests.
- Residual: Manual release discipline until H1 CI automation.

### H0-S31 — PII or secrets in observability output

- Class: ADV-INS
- Boundary: B6
- Assets: A2, A8
- Entry: Logs or metrics include keys, passwords or user identifiers (W1
  review case "PII in observability output").
- Impact: Mass credential or privacy leak via log aggregation.
- Owner: security officer
- Mitigations: CLI prints only whitelisted fields (entry/address/public_key);
  W0 log-scan NC (`MnemonicInLog`) enforces "no mnemonic in logs"; audit
  evidence contains only public test material.
- Residual: Operators may add verbose logging in the future — the H7 access
  log requirement covers detection.

### H0-S32 — Unsafe node configuration in production

- Class: ADV-INS
- Boundary: B3
- Assets: A4
- Entry: An operator runs a node with debug endpoints open or a devnet key
  in a public network (W2 review case "unsafe configuration in production").
- Impact: Remote compromise of node keys and state.
- Owner: node operator
- Mitigations: `init` refuses unsafe configurations (`error: refused: unsafe
  configuration`); `preflight` verifies config keys; runbooks call out the
  devnet-only markers.
- Residual: Post-start configuration edits are out of scope of `init`.

## 5. Owners and roles

Each scenario has exactly one owner drawn from this list (the review gate
enforces it):

- wallet maintainer — wallet CLI, keystore, offline signing, multisig.
- core maintainer — codecs, consensus, node core code.
- node operator — running devnet nodes, RPC exposure, node keys.
- validator operator — validator keys, double-sign discipline.
- release manager — patches, releases, toolchains, ceremonies.
- security officer — evidence integrity, monitoring, disclosures.
- audit reviewer — external and internal review findings.

## 6. Review gate

The gate is deterministic and machine-checked:

- `make check-h0` runs `tools/check_threat_model.py`, which parses this file
  and fails (exit 1, `error: …` lines) on: fewer than 30 scenarios, duplicate
  scenario ids, a scenario missing any required field (Class, Boundary,
  Assets, Entry, Impact, Owner, Mitigations), an unknown class/boundary/asset
  reference, an owner outside §5, or empty mitigations. It also compares the
  parsed inventory against the pinned oracle output
  (`tools/fixtures/h0_threat_model_vectors.json`).
- `make gen-h0-vectors` regenerates that oracle output with
  `tools/gen_h0_vectors.py` (run it after editing this file).
- `make nc-h0` runs the mutation controls `tools/nc_h0.py`; NC-1 (the named
  control, "scenario without an owner") deletes one Owner field and requires
  the gate to fail with `error: scenario H0-S07: missing owner`.
- Extend this document by ADDING scenarios and owners; never remove an owner
  to make the gate pass.

This document is a living artifact: it is re-reviewed whenever a new phase
ships a boundary-crossing feature (H1–H7) and after any incident (H9).
