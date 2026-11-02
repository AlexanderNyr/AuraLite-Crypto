# W0 Key-Exposure Audit & Threat Notes (`WALLET_PLAN` W0)

**Phase:** W0 (Baseline — audit of "where keys could end up" + threat notes)  
**Crate:** `crates/aura-wallet` (`aura_wallet::baseline`)  
**Gate:** `make test-w0` / `make nc-w0` (`NC: key in a log -> gate red (tree search)`)

---

## 1. Where Keys Could End Up (8 Audited Exposure Surfaces)

| Surface ID | Location | Severity | Mandatory Control | Owner Phase |
|---|---|---|---|---|
| `logs-and-tracing` | Structured/plain daemon or test logs (`*.log`, `tracing`, `Debug`/`Display`) | `CRITICAL` | Redacted `Debug`/`Display` on key types; tree-search log scanner gate in W0 | `W0` |
| `cli-argv-and-env` | CLI flags (`--private-key`, `--mnemonic`) visible in `/proc/<pid>/cmdline` and shell history | `CRITICAL` | Forbid raw secret keys on `argv`; read encrypted keystore path or stdin prompt | `W1` |
| `plaintext-disk-files` | Unencrypted key files, swap/temp files, or world-readable keystore permissions | `CRITICAL` | Argon2id + AES-256-GCM versioned keystore with explicit KDF parameters (`D-W2`) | `W1` |
| `process-memory-and-core-dumps` | Unzeroized heap/stack buffers after signing, decryption, or BIP-32 derivation | `HIGH` | `Zeroize` / `ZeroizeOnDrop` on seed, derived key, and decrypted buffer types (`D-W2`) | `W1` |
| `rpc-and-network-frames` | Transmitting secret keys or mnemonics to RPC nodes instead of local signing | `CRITICAL` | Local signing with `chain_id` + domain separation; RPC accepts only signed `Tx` (`D-W2`/W2) | `W2` |
| `online-hot-machine-signing` | Signing high-value or validator transactions on an internet-connected host | `HIGH` | First-class air-gapped `unsigned tx -> QR/file -> signed tx -> broadcast` flow (`D-W3`) | `W3` |
| `single-key-treasury-or-validator` | Single private key controlling treasury reserves or validator governance authority | `CRITICAL` | Mandatory M-of-N multisig with threshold verification, timeouts, and rotation (`D-W4`) | `W4` |
| `ci-and-repository-fixtures` | Committing private keys, mnemonics, or unencrypted keystores into git or CI artifacts | `CRITICAL` | Repo-wide W0 tree scanner rejects secret markers in logs, fixtures, and docs | `W0` |

---

## 2. Threat Model Notes (`T-W0-1` .. `T-W0-6`)

### `T-W0-1` — Accidental private-key or mnemonic emission in structured logs
- **Attack vector:** Developer logs a config, keystore struct, or signing request via `tracing` or `println!`, leaking raw hex secret keys or BIP-39 words into `.log` files or OTLP spans.
- **Control (`D-W2`):** Never write a private key to logs; opaque secret wrappers redact `Debug`/`Display`; W0 tree scanner (`audit_tree_for_key_leaks`) audits all workspace logs and text artifacts.
- **Verification gate:** `W0 gate: key in a log -> gate red (tree search)`.

### `T-W0-2` — Offline brute-force of stolen keystore file
- **Attack vector:** Attacker exfiltrates a keystore file from disk or backup and runs GPU/ASIC dictionary attacks against weak password hashing.
- **Control (`D-W2`):** Argon2id KDF + AES-256-GCM authenticated encryption + versioned KDF parameters stored with the ciphertext; zeroize plaintext keys on drop.
- **Verification gate:** `W1 gate: 1000 wrong passwords -> 0 decryptions; format fuzz`.

### `T-W0-3` — Non-deterministic or colliding key derivation paths
- **Attack vector:** Ad-hoc key generation prevents mnemonic backup recovery or mixes keys across chains and account indices.
- **Control (`D-W1`):** Canonical BIP-39 mnemonic -> BIP-32/44 hierarchical derivation path `m/44'/ALCRY'/0'/0/i`.
- **Verification gate:** `W1 gate: BIP-32 test vectors and deterministic address derivation`.

### `T-W0-4` — Cross-chain or cross-domain signature replay
- **Attack vector:** A transaction signed for testnet or a different protocol domain is replayed against mainnet or another state machine.
- **Control (`D-C5` / `D-W3`):** Every signature covers `domain || chain_id || canonical_bytes`; wallet CLI and offline signer reject foreign `chain_id`s and domains.
- **Verification gate:** `W2/W3 gates: foreign chain-id and foreign domain signature rejection`.

### `T-W0-5` — Compromise of online broadcast host during transaction signing
- **Attack vector:** Malware on the online machine steals hot signing keys or tampers with unsigned transaction fields before broadcast.
- **Control (`D-W3`):** Air-gapped signing pipeline where secret keys never leave the offline machine and the online machine re-verifies canonical unsigned/signed tx digests.
- **Verification gate:** `W3 gate: air-gapped scenario end-to-end`.

### `T-W0-6` — Single-operator compromise or lost key on treasury / validator accounts
- **Attack vector:** A single leaked or lost private key drains treasury funds or halts validator governance actions.
- **Control (`D-W4`):** Treasury and validator control require M-of-N multisig with bounded proposal timeouts and explicit member rotation.
- **Verification gate:** `W4 gate: 3-of-5 multisig (3 -> success, 2 -> reject; NC: threshold bypass -> red)`.
