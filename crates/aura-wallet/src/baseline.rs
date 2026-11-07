//! W0 baseline — **audit of where keys could end up**, threat notes (`D-W1`..`D-W4`),
//! and the tree/log secret-material scanner (`NC: key in a log -> gate red`).
//!
//! # Why W0 exists before any keystore code
//!
//! "A private key in a text file" (or in a structured log line, CLI argument, or
//! unencrypted debug dump) is the most common reason funds and validator keys are
//! compromised. Before W1 implements BIP-39/32/44 derivation and the Argon2id +
//! AES-256-GCM keystore, W0 pins:
//!
//! 1. The **layer survey** of `crates/aura-wallet/src` across W1–W4 (`keystore`,
//!    `wallet-cli`, `offline-signing`, `multisig`), refusing to claim a working
//!    wallet while capabilities are still placeholders (`EXIT_STUB = 2`).
//! 2. The **8 key-exposure surfaces** (`EXPOSURE_SURFACES`) auditing every path
//!    where secret material could end up (`logs-and-tracing`, `cli-argv-and-env`,
//!    `plaintext-disk-files`, `process-memory-and-core-dumps`,
//!    `rpc-and-network-frames`, `online-hot-machine-signing`,
//!    `single-key-treasury-or-validator`, `ci-and-repository-fixtures`).
//! 3. The **6 threat notes** (`THREAT_NOTES`, `T-W0-1`..`T-W0-6`) binding each
//!    exposure surface to the mandatory controls in `D-W1`..`D-W4`.
//! 4. The **tree/log secret-material scanner** ([`scan_text_for_key_leaks`] and
//!    [`audit_tree_for_key_leaks`]), which walks workspace logs, evidence files,
//!    configs, and source files and fails closed if any private key, seed, PEM
//!    block, or BIP-39 mnemonic appears in a log or file.

#![deny(missing_docs)]

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The phase these numbers belong to.
pub const PHASE: &str = "W0";

/// Exit code of the refusal: wallet capabilities are not yet implemented.
pub const EXIT_STUB: i32 = 2;

/// Exit code when `report` is asked to measure a tree that is not a workspace.
pub const EXIT_NO_WORKSPACE: i32 = 3;

/// Exit code of a usage error (sysexits `EX_USAGE`).
pub const EXIT_USAGE: i32 = 64;

/// Historical W0 BIP-44 path; superseded by D-W1-testnet for this branch.
pub const BIP44_PATH_TEMPLATE: &str = "m/44'/ALCRY'/0'/0/i";

/// One wallet component that will land in W1–W4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    /// Component name, e.g. `keystore`.
    pub name: &'static str,
    /// The phase that lands the real implementation.
    pub phase: &'static str,
    /// What the component is responsible for.
    pub what: &'static str,
}

/// Every Stage 1 wallet component, in landing order (`W1`–`W4`).
pub const LAYERS: [Layer; 4] = [
    Layer {
        name: "keystore",
        phase: "W1",
        what: "BIP-39 and SLIP-0010 Ed25519 testnet derivation, encrypted keystore, zeroize",
    },
    Layer {
        name: "wallet-cli",
        phase: "W2",
        what: "`aura-wallet` CLI: new/import/list/balance/transfer/sign/broadcast/stake/vote",
    },
    Layer {
        name: "offline-signing",
        phase: "W3",
        what: "air-gapped unsigned/signed tx format, QR/USB bundle, online verification",
    },
    Layer {
        name: "multisig",
        phase: "W4",
        what: "M-of-N multisig coordination, signature collection, timeouts, member rotation",
    },
];

impl Layer {
    /// Source file under `crates/aura-wallet/src/` that implements this layer.
    #[must_use]
    pub fn module_file(&self) -> &'static str {
        match self.name {
            "keystore" => "keystore.rs",
            "wallet-cli" => "cli.rs",
            "offline-signing" => "offline.rs",
            "multisig" => "multisig.rs",
            _ => unreachable!(),
        }
    }

    /// True when this layer's source file declares public wallet logic.
    /// W1 lands the keystore; W2–W4 remain placeholders.
    #[must_use]
    pub fn has_wallet_logic(&self) -> bool {
        matches!(self.name, "keystore")
    }
}

/// One wallet capability that does not exist yet, and the phase that will add it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// Stable name, used by the gate and the refusal message.
    pub name: &'static str,
    /// The phase that lands it.
    pub phase: &'static str,
    /// What the capability will do.
    pub what: &'static str,
}

/// Capabilities still absent at W0 (`W1`–`W4`).
pub const MISSING_CAPABILITIES: [Capability; 3] = [
    Capability {
        name: "wallet-cli",
        phase: "W2",
        what: "`aura-wallet` new/import/list/balance/transfer/sign/broadcast/stake/vote",
    },
    Capability {
        name: "offline-signing",
        phase: "W3",
        what: "air-gapped unsigned/signed tx bundle, QR/USB transport, domain verification",
    },
    Capability {
        name: "multisig",
        phase: "W4",
        what: "M-of-N multisig contract workflow, signature collection, timeouts, rotation",
    },
];

/// Audit entry describing a concrete surface where secret key material could leak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyExposureSurface {
    /// Stable surface identifier.
    pub id: &'static str,
    /// Where secret material could appear if unguarded.
    pub location: &'static str,
    /// Severity if exposed (`CRITICAL` or `HIGH`).
    pub severity: &'static str,
    /// Mandatory defence enforced across W0–W4.
    pub mitigation: &'static str,
    /// Phase that enforces or verifies the control.
    pub owner_phase: &'static str,
}

/// The 8 audited key-exposure surfaces ("where keys could end up").
pub const EXPOSURE_SURFACES: [KeyExposureSurface; 8] = [
    KeyExposureSurface {
        id: "logs-and-tracing",
        location: "structured/plain daemon or test logs (`*.log`, `tracing`, `Debug`/`Display`)",
        severity: "CRITICAL",
        mitigation: "redacted Debug/Display on key types; tree-search log scanner gate in W0",
        owner_phase: "W0",
    },
    KeyExposureSurface {
        id: "cli-argv-and-env",
        location: "CLI flags (`--private-key`, `--mnemonic`) visible in `/proc/<pid>/cmdline` and shell history",
        severity: "CRITICAL",
        mitigation: "forbid raw secret keys on argv; read encrypted keystore path or stdin prompt",
        owner_phase: "W1",
    },
    KeyExposureSurface {
        id: "plaintext-disk-files",
        location: "unencrypted key files, swap/temp files, or world-readable keystore permissions",
        severity: "CRITICAL",
        mitigation: "Argon2id + AES-256-GCM versioned keystore with explicit KDF parameters (D-W2)",
        owner_phase: "W1",
    },
    KeyExposureSurface {
        id: "process-memory-and-core-dumps",
        location: "unzeroized heap/stack buffers after signing, decryption, or BIP-32 derivation",
        severity: "HIGH",
        mitigation: "`Zeroize` / `ZeroizeOnDrop` on seed, derived key, and decrypted buffer types (D-W2)",
        owner_phase: "W1",
    },
    KeyExposureSurface {
        id: "rpc-and-network-frames",
        location: "transmitting secret keys or mnemonics to RPC nodes instead of local signing",
        severity: "CRITICAL",
        mitigation: "local signing with `chain_id` + domain separation; RPC accepts only signed `Tx` (D-W2/W2)",
        owner_phase: "W2",
    },
    KeyExposureSurface {
        id: "online-hot-machine-signing",
        location: "signing high-value or validator transactions on an internet-connected host",
        severity: "HIGH",
        mitigation: "first-class air-gapped `unsigned tx -> QR/file -> signed tx -> broadcast` flow (D-W3)",
        owner_phase: "W3",
    },
    KeyExposureSurface {
        id: "single-key-treasury-or-validator",
        location: "single private key controlling treasury reserves or validator governance authority",
        severity: "CRITICAL",
        mitigation: "mandatory M-of-N multisig with threshold verification, timeouts, and rotation (D-W4)",
        owner_phase: "W4",
    },
    KeyExposureSurface {
        id: "ci-and-repository-fixtures",
        location: "committing private keys, mnemonics, or unencrypted keystores into git or CI artifacts",
        severity: "CRITICAL",
        mitigation: "repo-wide W0 tree scanner rejects secret markers in logs, fixtures, and docs",
        owner_phase: "W0",
    },
];

/// Threat model note for `WALLET_PLAN` (`D-W1`..`D-W4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreatNote {
    /// Stable threat identifier (`T-W0-1`..`T-W0-6`).
    pub id: &'static str,
    /// Threat title.
    pub title: &'static str,
    /// Attack vector description.
    pub attack_vector: &'static str,
    /// Architectural decision (`D-W1`..`D-W4`) and control that neutralises it.
    pub control: &'static str,
    /// Phase gate that tests the defence.
    pub verification_gate: &'static str,
}

/// The 6 W0 wallet threat notes.
pub const THREAT_NOTES: [ThreatNote; 6] = [
    ThreatNote {
        id: "T-W0-1",
        title: "Accidental private-key or mnemonic emission in structured logs",
        attack_vector: "Developer logs a config, keystore struct, or signing request via tracing/println, leaking raw hex secret keys or BIP-39 words into log files or OTLP spans.",
        control: "D-W2: never write a private key to logs; opaque secret wrappers redact Debug/Display; W0 tree scanner audits all workspace logs and text artifacts.",
        verification_gate: "W0 gate: key in a log -> gate red (tree search)",
    },
    ThreatNote {
        id: "T-W0-2",
        title: "Offline brute-force of stolen keystore file",
        attack_vector: "Attacker exfiltrates a keystore file from disk or backup and runs GPU/ASIC dictionary attacks against weak password hashing.",
        control: "D-W2: Argon2id KDF + AES-256-GCM authenticated encryption + versioned KDF parameters stored with the ciphertext; zeroize plaintext keys on drop.",
        verification_gate: "W1 gate: 1000 wrong passwords -> 0 decryptions; format fuzz",
    },
    ThreatNote {
        id: "T-W0-3",
        title: "Non-deterministic or colliding key derivation paths",
        attack_vector: "Ad-hoc key generation prevents mnemonic backup recovery or mixes keys across chains and account indices.",
        control: "D-W1-testnet: BIP-39 -> hardened SLIP-0010 Ed25519 path m/44'/1'/0'/0'/i'; mainnet requires a separate decision.",
        verification_gate: "W1 gate: BIP-39 and SLIP-0010 vectors and deterministic address derivation",
    },
    ThreatNote {
        id: "T-W0-4",
        title: "Cross-chain or cross-domain signature replay",
        attack_vector: "A transaction signed for testnet or a different protocol domain is replayed against mainnet or another state machine.",
        control: "D-C5 / D-W3: every signature covers domain || chain_id || canonical_bytes; wallet CLI and offline signer reject foreign chain-ids and domains.",
        verification_gate: "W2/W3 gates: foreign chain-id and foreign domain signature rejection",
    },
    ThreatNote {
        id: "T-W0-5",
        title: "Compromise of online broadcast host during transaction signing",
        attack_vector: "Malware on the online machine steals hot signing keys or tampers with unsigned transaction fields before broadcast.",
        control: "D-W3: air-gapped signing pipeline where secret keys never leave the offline machine and the online machine re-verifies canonical unsigned/signed tx digests.",
        verification_gate: "W3 gate: air-gapped scenario end-to-end",
    },
    ThreatNote {
        id: "T-W0-6",
        title: "Single-operator compromise or lost key on treasury / validator accounts",
        attack_vector: "A single leaked or lost private key drains treasury funds or halts validator governance actions.",
        control: "D-W4: treasury and validator control require M-of-N multisig with bounded proposal timeouts and explicit member rotation.",
        verification_gate: "W4 gate: 3-of-5 multisig (3 -> success, 2 -> reject; NC: threshold bypass -> red)",
    },
];

/// Classification of a detected secret-material leak in a log or file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyLeakKind {
    /// A 32-byte (64-hex) or 64-byte (128-hex) secret key following a secret-key field marker.
    HexPrivateKeyInLog,
    /// A BIP-39 mnemonic / seed phrase (>= 12 lowercase words) following a mnemonic marker.
    MnemonicInLog,
    /// An unencrypted PEM private-key block header.
    PemPrivateKey,
    /// A raw master/derivation seed (>= 32 hex chars) following a seed marker.
    RawSeedInLog,
}

impl KeyLeakKind {
    /// Stable identifier used in fixtures and audit reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HexPrivateKeyInLog => "hex_private_key_in_log",
            Self::MnemonicInLog => "mnemonic_in_log",
            Self::PemPrivateKey => "pem_private_key",
            Self::RawSeedInLog => "raw_seed_in_log",
        }
    }
}

/// One secret-material leak detected by [`scan_text_for_key_leaks`] or [`audit_tree_for_key_leaks`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyLeakFinding {
    /// Relative path or label of the scanned file/stream.
    pub path: String,
    /// 1-based line number where the leak was detected.
    pub line_number: usize,
    /// Classification of the leaked secret material.
    pub kind: KeyLeakKind,
    /// Marker token that triggered detection.
    pub marker: String,
    /// Safe redacted preview (never echoes the raw secret back into logs).
    pub redacted_preview: String,
}

/// Result of running [`audit_tree_for_key_leaks`] over a workspace tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyAuditReport {
    /// Workspace root that was audited.
    pub root: PathBuf,
    /// Total number of text/source/config/log files scanned.
    pub scanned_files: usize,
    /// Number of `.log` and `gates.txt` evidence log files scanned.
    pub scanned_log_files: usize,
    /// Any secret-material leaks found (must be empty for the W0 gate to pass).
    pub findings: Vec<KeyLeakFinding>,
}

impl KeyAuditReport {
    /// True when the tree search found zero secret keys in logs or files.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// What a wallet layer turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// No source files at all.
    Missing,
    /// Source files exist in the crate, but this layer's module does not yet
    /// implement wallet logic.
    Placeholder,
    /// The layer's module exists and declares public wallet items.
    Implemented,
}

impl Presence {
    /// Stable string form used by the fixture and the report.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Placeholder => "placeholder",
            Self::Implemented => "implemented",
        }
    }
}

/// One measured wallet component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measurement {
    /// The layer this measurement belongs to.
    pub layer: Layer,
    /// What the source says it is.
    pub presence: Presence,
    /// Number of `.rs` files under `crates/aura-wallet/src/`.
    pub files: usize,
    /// Number of lines in those files.
    pub lines: usize,
    /// Number of public item declarations in those files.
    pub public_items: usize,
}

/// Result of [`survey`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survey {
    /// Root that was measured.
    pub root: PathBuf,
    /// One entry per layer of [`LAYERS`], in that order.
    pub layers: Vec<Measurement>,
}

impl Survey {
    /// Layers that report [`Presence::Implemented`].
    #[must_use]
    pub fn implemented(&self) -> usize {
        self.count(Presence::Implemented)
    }

    /// Layers that are still placeholders.
    #[must_use]
    pub fn placeholders(&self) -> usize {
        self.count(Presence::Placeholder)
    }

    /// Layers with no source at all.
    #[must_use]
    pub fn missing(&self) -> usize {
        self.count(Presence::Missing)
    }

    fn count(&self, want: Presence) -> usize {
        self.layers.iter().filter(|m| m.presence == want).count()
    }
}

/// Scan a text buffer (such as a `.log` file) for leaked private keys, mnemonics,
/// PEM private-key blocks, or raw seeds.
///
/// Public identifiers (`public_key`, `pubkey`, `block_hash`, `state_root`,
/// `tx_hash`, `txid`, `address`, `signature`) are explicitly ignored so normal
/// daemon and test logs do not trigger false positives.
#[must_use]
pub fn scan_text_for_key_leaks(path_label: &str, content: &str) -> Vec<KeyLeakFinding> {
    let mut findings = Vec::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line_number = idx + 1;
        let lower = raw_line.to_ascii_lowercase();

        // 1. Check for PEM private key headers on the trimmed line.
        let trimmed_line = raw_line.trim();
        if trimmed_line.starts_with("-----BEGIN ") && trimmed_line.ends_with("PRIVATE KEY-----") {
            findings.push(KeyLeakFinding {
                path: path_label.to_string(),
                line_number,
                kind: KeyLeakKind::PemPrivateKey,
                marker: "PEM_PRIVATE_KEY".to_string(),
                redacted_preview: format!(
                    "{}:{}: <REDACTED PEM PRIVATE KEY>",
                    path_label, line_number
                ),
            });
            continue;
        }

        // 2. Check for secret-key markers followed by 64-hex (32-byte) or 128-hex (64-byte) material,
        // or a `aura1priv` / `xprv` encoded secret key.
        let secret_key_markers = [
            "private_key",
            "secret_key",
            "privkey",
            "ed25519_secret",
            "signing_key",
            "bls_secret",
            "xprv",
            "aura1priv",
        ];
        let mut matched_secret = false;
        for marker in secret_key_markers {
            if let Some(pos) = lower.find(marker) {
                let tail = &raw_line[pos + marker.len()..];
                if (marker == "xprv" || marker == "aura1priv")
                    && has_prefixed_token_after_marker(&raw_line[pos..], marker, 24)
                {
                    findings.push(KeyLeakFinding {
                        path: path_label.to_string(),
                        line_number,
                        kind: KeyLeakKind::HexPrivateKeyInLog,
                        marker: marker.to_string(),
                        redacted_preview: format!(
                            "{}:{}: {}=<REDACTED SECRET KEY>",
                            path_label, line_number, marker
                        ),
                    });
                    matched_secret = true;
                    break;
                }
                if let Some(hex_len) = find_contiguous_hex_after_delimiter(tail, 64)
                    && (hex_len == 64 || hex_len == 128)
                {
                    findings.push(KeyLeakFinding {
                        path: path_label.to_string(),
                        line_number,
                        kind: KeyLeakKind::HexPrivateKeyInLog,
                        marker: marker.to_string(),
                        redacted_preview: format!(
                            "{}:{}: {}=<REDACTED {}-HEX PRIVATE KEY>",
                            path_label, line_number, marker, hex_len
                        ),
                    });
                    matched_secret = true;
                    break;
                }
            }
        }
        if matched_secret {
            continue;
        }

        // 3. Check for raw seed markers followed by >= 32 contiguous hex characters.
        let seed_markers = ["raw_seed", "master_seed", "secret_seed", "bip39_seed"];
        let mut matched_seed = false;
        for marker in seed_markers {
            if let Some(pos) = lower.find(marker) {
                let tail = &raw_line[pos + marker.len()..];
                if let Some(hex_len) = find_contiguous_hex_after_delimiter(tail, 32) {
                    findings.push(KeyLeakFinding {
                        path: path_label.to_string(),
                        line_number,
                        kind: KeyLeakKind::RawSeedInLog,
                        marker: marker.to_string(),
                        redacted_preview: format!(
                            "{}:{}: {}=<REDACTED {}-HEX SEED>",
                            path_label, line_number, marker, hex_len
                        ),
                    });
                    matched_seed = true;
                    break;
                }
            }
        }
        if matched_seed {
            continue;
        }

        // 4. Check for mnemonic / seed_phrase markers followed by >= 12 alphabetic words.
        let mnemonic_markers = ["mnemonic", "seed_phrase", "recovery_phrase"];
        for marker in mnemonic_markers {
            if let Some(pos) = lower.find(marker) {
                let tail = &lower[pos + marker.len()..];
                if has_mnemonic_word_run_after_delimiter(tail, 12) {
                    findings.push(KeyLeakFinding {
                        path: path_label.to_string(),
                        line_number,
                        kind: KeyLeakKind::MnemonicInLog,
                        marker: marker.to_string(),
                        redacted_preview: format!(
                            "{}:{}: {}=<REDACTED MNEMONIC PHRASE>",
                            path_label, line_number, marker
                        ),
                    });
                    break;
                }
            }
        }
    }

    findings
}

fn has_prefixed_token_after_marker(segment: &str, prefix: &str, min_total_len: usize) -> bool {
    let lower = segment.to_ascii_lowercase();
    if !lower.starts_with(prefix) {
        return false;
    }
    let token_len = segment
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .count();
    token_len >= min_total_len
}

fn find_contiguous_hex_after_delimiter(tail: &str, min_hex_len: usize) -> Option<usize> {
    let trimmed = tail.trim_start();
    let after_key_quote = trimmed
        .strip_prefix('"')
        .or_else(|| trimmed.strip_prefix('\''))
        .unwrap_or(trimmed)
        .trim_start();
    let after_delim = after_key_quote
        .strip_prefix('=')
        .or_else(|| after_key_quote.strip_prefix(':'))?
        .trim_start();
    let after_val_quote = after_delim
        .strip_prefix('"')
        .or_else(|| after_delim.strip_prefix('\''))
        .unwrap_or(after_delim);
    let after_0x = after_val_quote
        .strip_prefix("0x")
        .or_else(|| after_val_quote.strip_prefix("0X"))
        .unwrap_or(after_val_quote);
    let hex_len = after_0x
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .count();
    if hex_len >= min_hex_len {
        Some(hex_len)
    } else {
        None
    }
}

fn has_mnemonic_word_run_after_delimiter(tail: &str, min_words: usize) -> bool {
    let trimmed = tail.trim_start();
    let after_key_quote = trimmed
        .strip_prefix('"')
        .or_else(|| trimmed.strip_prefix('\''))
        .unwrap_or(trimmed)
        .trim_start();
    let Some(after_delim) = after_key_quote
        .strip_prefix('=')
        .or_else(|| after_key_quote.strip_prefix(':'))
    else {
        return false;
    };
    let after_val_quote = after_delim
        .trim_start()
        .strip_prefix('"')
        .or_else(|| after_delim.trim_start().strip_prefix('\''))
        .unwrap_or_else(|| after_delim.trim_start());
    let mut count = 0usize;
    for token in after_val_quote.split_whitespace() {
        let clean =
            token.trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ';' || c == '}');
        if clean.len() >= 3 && clean.chars().all(|c| c.is_ascii_lowercase()) {
            count += 1;
            if count >= min_words {
                return true;
            }
        } else {
            break;
        }
    }
    false
}

/// Walk the workspace tree rooted at `root` and scan all logs (`docs/evidence/**/*.log`,
/// `docs/evidence/**/gates.txt`) and source/config files for secret-key leaks.
///
/// # Errors
///
/// Returns `Err` if `root/crates` does not exist.
pub fn audit_tree_for_key_leaks(root: &Path) -> Result<KeyAuditReport, String> {
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return Err(format!(
            "{} is not an AuraLite-Crypto workspace (no crates/ directory)",
            root.display()
        ));
    }

    let mut scanned_files = 0usize;
    let mut scanned_log_files = 0usize;
    let mut findings = Vec::new();

    let scan_roots = ["docs", "crates", "examples", "standards", "tools"];
    for rel_dir in scan_roots {
        let dir = root.join(rel_dir);
        if dir.is_dir() {
            walk_and_scan_dir(
                root,
                &dir,
                &mut scanned_files,
                &mut scanned_log_files,
                &mut findings,
            )?;
        }
    }

    // Also scan any top-level `.log`, `.toml`, or `.json` files in `root`.
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut top_files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|de| de.path()))
            .filter(|p| p.is_file())
            .collect();
        top_files.sort();
        for path in top_files {
            if should_scan_file(&path) {
                scan_one_file(
                    root,
                    &path,
                    &mut scanned_files,
                    &mut scanned_log_files,
                    &mut findings,
                )?;
            }
        }
    }

    Ok(KeyAuditReport {
        root: root.to_path_buf(),
        scanned_files,
        scanned_log_files,
        findings,
    })
}

fn walk_and_scan_dir(
    root: &Path,
    dir: &Path,
    scanned_files: &mut usize,
    scanned_log_files: &mut usize,
    findings: &mut Vec<KeyLeakFinding>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if matches!(name, "target" | ".git" | "__pycache__") {
                continue;
            }
            walk_and_scan_dir(root, &path, scanned_files, scanned_log_files, findings)?;
        } else if path.is_file() && should_scan_file(&path) {
            scan_one_file(root, &path, scanned_files, scanned_log_files, findings)?;
        }
    }
    Ok(())
}

fn should_scan_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(ext, "log" | "txt" | "rs" | "toml" | "json" | "yaml" | "md") || name == "SHA256SUMS"
}

fn is_log_evidence_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    ext == "log" || name == "gates.txt"
}

fn scan_one_file(
    root: &Path,
    path: &Path,
    scanned_files: &mut usize,
    scanned_log_files: &mut usize,
    findings: &mut Vec<KeyLeakFinding>,
) -> Result<(), String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    *scanned_files += 1;
    if is_log_evidence_file(path) {
        *scanned_log_files += 1;
    }
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut file_findings = scan_text_for_key_leaks(&rel, &content);
    findings.append(&mut file_findings);
    Ok(())
}

/// Measure `crates/aura-wallet/src` under `root`.
///
/// # Errors
///
/// Returns `Err` if `root/crates` does not exist or cannot be read.
pub fn survey(root: &Path) -> Result<Survey, String> {
    let crates = root.join("crates");
    if !crates.is_dir() {
        return Err(format!(
            "{} is not an AuraLite-Crypto workspace (no crates/ directory)",
            root.display()
        ));
    }

    let src = crates.join("aura-wallet").join("src");
    let (total_files, total_lines, total_pub) = if src.is_dir() {
        let mut files = 0usize;
        let mut lines = 0usize;
        let mut pub_items = 0usize;
        measure_dir(&src, &mut files, &mut lines, &mut pub_items)?;
        (files, lines, pub_items)
    } else {
        (0, 0, 0)
    };

    let mut layers = Vec::with_capacity(LAYERS.len());
    for layer in LAYERS {
        let presence = if total_files == 0 {
            Presence::Missing
        } else {
            let mod_path = src.join(layer.module_file());
            if mod_path.is_file() && layer.has_wallet_logic() {
                let text = std::fs::read_to_string(&mod_path)
                    .map_err(|e| format!("{}: {e}", mod_path.display()))?;
                if count_public_items(&text) > 0 {
                    Presence::Implemented
                } else {
                    Presence::Placeholder
                }
            } else {
                Presence::Placeholder
            }
        };
        layers.push(Measurement {
            layer,
            presence,
            files: total_files,
            lines: total_lines,
            public_items: total_pub,
        });
    }

    Ok(Survey {
        root: root.to_path_buf(),
        layers,
    })
}

fn measure_dir(
    dir: &Path,
    files: &mut usize,
    lines: &mut usize,
    public_items: &mut usize,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        if path.is_dir() {
            measure_dir(&path, files, lines, public_items)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            *files += 1;
            *lines += text.lines().count();
            *public_items += count_public_items(&text);
        }
    }
    Ok(())
}

/// Count lines that declare a public Rust item (`pub fn`, `pub struct`, etc.).
#[must_use]
pub fn count_public_items(source: &str) -> usize {
    const PREFIXES: [&str; 9] = [
        "pub fn ",
        "pub const fn ",
        "pub async fn ",
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "pub type ",
        "pub const ",
        "pub mod ",
    ];
    source
        .lines()
        .map(str::trim_start)
        .filter(|line| {
            !line.starts_with("//") && PREFIXES.iter().any(|prefix| line.starts_with(prefix))
        })
        .count()
}

/// Format a [`Survey`] as the deterministic, `key=value` fixture text.
#[must_use]
pub fn format_fixture(survey: &Survey) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# W0 aura-wallet baseline fixture (generated by tools/gen_w0_vectors.py)"
    );
    let _ = writeln!(out, "phase={PHASE}");
    let _ = writeln!(out, "exit_stub={EXIT_STUB}");
    let _ = writeln!(out, "exit_usage={EXIT_USAGE}");
    let _ = writeln!(out, "exit_no_workspace={EXIT_NO_WORKSPACE}");
    let _ = writeln!(out, "historical_bip44_path_template={BIP44_PATH_TEMPLATE}");
    let _ = writeln!(
        out,
        "testnet_path_template={}",
        crate::derivation::AURA_PATH_TEMPLATE
    );
    let _ = writeln!(out, "layers={}", survey.layers.len());
    let _ = writeln!(out, "implemented={}", survey.implemented());
    let _ = writeln!(out, "placeholders={}", survey.placeholders());
    let _ = writeln!(out, "missing={}", survey.missing());
    let _ = writeln!(out, "exposure_surfaces={}", EXPOSURE_SURFACES.len());
    let _ = writeln!(out, "threat_notes={}", THREAT_NOTES.len());
    let _ = writeln!(out, "missing_capabilities={}", MISSING_CAPABILITIES.len());
    for m in &survey.layers {
        let _ = writeln!(
            out,
            "layer={}\t{}\t{}\t{}\t{}\t{}\t{}",
            m.layer.name,
            m.layer.phase,
            m.presence.as_str(),
            m.files,
            m.lines,
            m.public_items,
            m.layer.what,
        );
    }
    for surface in EXPOSURE_SURFACES {
        let _ = writeln!(
            out,
            "surface={}\t{}\t{}\t{}",
            surface.id, surface.owner_phase, surface.severity, surface.location,
        );
    }
    for note in THREAT_NOTES {
        let _ = writeln!(
            out,
            "threat={}\t{}\t{}",
            note.id, note.verification_gate, note.title,
        );
    }
    for cap in MISSING_CAPABILITIES {
        let _ = writeln!(out, "absent={}\t{}\t{}", cap.name, cap.phase, cap.what);
    }
    out
}

/// Format a [`Survey`] as the human-readable `aura-wallet report` output.
#[must_use]
pub fn format_report(survey: &Survey) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "aura-wallet ({PHASE} baseline) — wallet capability survey & key-exposure audit"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {:<18} {:<6} {:<13} {:>5} {:>7} {:>9}  responsibility",
        "component", "phase", "presence", "files", "lines", "pub items"
    );
    let _ = writeln!(out, "  {}", "-".repeat(88));
    for m in &survey.layers {
        let _ = writeln!(
            out,
            "  {:<18} {:<6} {:<13} {:>5} {:>7} {:>9}  {}",
            m.layer.name,
            m.layer.phase,
            m.presence.as_str(),
            m.files,
            m.lines,
            m.public_items,
            m.layer.what,
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  measured: {} implemented, {} placeholder, {} missing (of {} wallet components)",
        survey.implemented(),
        survey.placeholders(),
        survey.missing(),
        survey.layers.len(),
    );
    let _ = writeln!(
        out,
        "  key-exposure audit: {} surfaces mapped, {} threat notes; testnet path `{}` (historical BIP-44 path `{BIP44_PATH_TEMPLATE}` superseded)",
        EXPOSURE_SURFACES.len(),
        THREAT_NOTES.len(),
        crate::derivation::AURA_PATH_TEMPLATE,
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "  wallet capabilities NOT present at {PHASE}:");
    for cap in MISSING_CAPABILITIES {
        let _ = writeln!(
            out,
            "    - {:<18} (lands in {}): {}",
            cap.name, cap.phase, cap.what
        );
    }
    out
}

/// Format the refusal printed when `aura-wallet` is invoked without a subcommand
/// or with an unimplemented wallet subcommand at W0.
#[must_use]
pub fn format_refusal(invoked_with: Option<&str>) -> String {
    let mut out = String::new();
    match invoked_with {
        Some(arg) => {
            let _ = writeln!(
                out,
                "aura-wallet: refusing `{arg}` — wallet keystore and CLI are not implemented at phase {PHASE}."
            );
        }
        None => {
            let _ = writeln!(
                out,
                "aura-wallet: refusing to run — phase {PHASE} is the key-exposure baseline; no keystore or signing CLI exists yet."
            );
        }
    }
    let _ = writeln!(
        out,
        "aura-wallet: capabilities absent at {PHASE} (see docs/plans/WALLET_PLAN.md):"
    );
    for cap in MISSING_CAPABILITIES {
        let _ = writeln!(out, "  - {:<18} [{}]: {}", cap.name, cap.phase, cap.what);
    }
    let _ = writeln!(
        out,
        "aura-wallet: run `aura-wallet report` to inspect the W0 key-exposure audit and layer survey."
    );
    out
}

const KNOWN_WALLET_COMMANDS: [&str; 11] = [
    "new",
    "import",
    "list",
    "balance",
    "transfer",
    "sign",
    "broadcast",
    "stake",
    "vote",
    "offline-sign",
    "multisig",
];

/// Entry point of the `aura-wallet` binary.
pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let workspace = default_workspace_root();
    run_with_workspace(args, &workspace, stdout, stderr)
}

/// Entry point with an explicit default workspace root, used by tests.
pub fn run_with_workspace(
    args: &[String],
    default_root: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let Some(first) = args.first().map(String::as_str) else {
        let _ = write!(stderr, "{}", format_refusal(None));
        return EXIT_STUB;
    };

    match first {
        "--help" | "-h" => {
            let _ = writeln!(
                stdout,
                "Usage: aura-wallet report [--fixture] [--workspace <DIR>]"
            );
            let _ = writeln!(stdout, "       aura-wallet audit-tree [--workspace <DIR>]");
            let _ = writeln!(
                stdout,
                "Phase {PHASE} baseline: audits key-exposure surfaces and refuses unimplemented wallet commands (exit {EXIT_STUB})."
            );
            0
        }
        "report" => run_report(&args[1..], default_root, stdout, stderr),
        "audit-tree" => run_audit_tree(&args[1..], default_root, stdout, stderr),
        cmd if KNOWN_WALLET_COMMANDS.contains(&cmd) => {
            let _ = write!(stderr, "{}", format_refusal(Some(cmd)));
            EXIT_STUB
        }
        other => {
            let _ = writeln!(
                stderr,
                "aura-wallet: unknown command `{other}` (try `aura-wallet --help`)"
            );
            EXIT_USAGE
        }
    }
}

fn run_report(
    args: &[String],
    default_root: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut fixture = false;
    let mut root: PathBuf = default_root.to_path_buf();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--fixture" => {
                fixture = true;
                i += 1;
            }
            "--workspace" => {
                let Some(val) = args.get(i + 1) else {
                    let _ = writeln!(stderr, "aura-wallet report: --workspace requires a path");
                    return EXIT_USAGE;
                };
                root = PathBuf::from(val);
                i += 2;
            }
            other => {
                let _ = writeln!(stderr, "aura-wallet report: unknown option `{other}`");
                return EXIT_USAGE;
            }
        }
    }

    match survey(&root) {
        Ok(s) => {
            let text = if fixture {
                format_fixture(&s)
            } else {
                format_report(&s)
            };
            let _ = write!(stdout, "{text}");
            0
        }
        Err(msg) => {
            let _ = writeln!(stderr, "aura-wallet report: {msg}");
            EXIT_NO_WORKSPACE
        }
    }
}

fn run_audit_tree(
    args: &[String],
    default_root: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut root: PathBuf = default_root.to_path_buf();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--workspace" => {
                let Some(val) = args.get(i + 1) else {
                    let _ = writeln!(
                        stderr,
                        "aura-wallet audit-tree: --workspace requires a path"
                    );
                    return EXIT_USAGE;
                };
                root = PathBuf::from(val);
                i += 2;
            }
            other => {
                let _ = writeln!(stderr, "aura-wallet audit-tree: unknown option `{other}`");
                return EXIT_USAGE;
            }
        }
    }

    match audit_tree_for_key_leaks(&root) {
        Ok(report) if report.is_clean() => {
            let _ = writeln!(
                stdout,
                "W0 KEY AUDIT: OK — scanned {} files ({} log files), 0 secret-key leaks",
                report.scanned_files, report.scanned_log_files
            );
            0
        }
        Ok(report) => {
            let _ = writeln!(
                stderr,
                "W0 KEY AUDIT: FAIL — {} secret-key leak(s) found across {} files",
                report.findings.len(),
                report.scanned_files
            );
            for finding in &report.findings {
                let _ = writeln!(stderr, "  - {}", finding.redacted_preview);
            }
            EXIT_STUB
        }
        Err(msg) => {
            let _ = writeln!(stderr, "aura-wallet audit-tree: {msg}");
            EXIT_NO_WORKSPACE
        }
    }
}

/// Locate the workspace root from `CARGO_MANIFEST_DIR` (`crates/aura-wallet`).
#[must_use]
pub fn default_workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn make_temp_dir(label: &str) -> PathBuf {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("aura-w0-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn w1_survey_reports_only_keystore_implemented() {
        let root = default_workspace_root();
        let s = survey(&root).expect("survey workspace");
        assert_eq!(s.layers.len(), 4);
        assert_eq!(s.implemented(), 1, "W1 keystore is implemented");
        assert_eq!(s.placeholders(), 3, "W2-W4 remain placeholders");
        assert_eq!(s.missing(), 0);
    }

    #[test]
    fn the_pinned_fixture_matches_this_tree() {
        let root = default_workspace_root();
        let s = survey(&root).expect("survey workspace");
        let fixture_path = root
            .join("crates")
            .join("aura-wallet")
            .join("tests")
            .join("fixtures")
            .join("w1_baseline.txt");
        let pinned = std::fs::read_to_string(&fixture_path).expect("read w1_baseline.txt");
        assert_eq!(format_fixture(&s), pinned);
    }

    #[test]
    fn key_in_a_log_turns_tree_search_gate_red() {
        // 1. Verify the real repository tree is completely clean.
        let root = default_workspace_root();
        let real_report = audit_tree_for_key_leaks(&root).expect("audit real workspace");
        assert!(
            real_report.is_clean(),
            "workspace tree must have 0 leaked keys, found: {:?}",
            real_report.findings
        );
        assert!(real_report.scanned_files > 50);
        assert!(real_report.scanned_log_files > 10);

        // 2. Create a minimal workspace with a clean log file -> audit passes.
        let temp = make_temp_dir("key-in-log");
        let crates_src = temp.join("crates/aura-wallet/src");
        let evidence_dir = temp.join("docs/evidence/w0");
        std::fs::create_dir_all(&crates_src).unwrap();
        std::fs::create_dir_all(&evidence_dir).unwrap();
        std::fs::write(crates_src.join("lib.rs"), "// clean\n").unwrap();
        let log_path = evidence_dir.join("daemon.log");
        std::fs::write(
            &log_path,
            "2026-11-02T00:00:00Z INFO aurad started public_key=1111111111111111111111111111111111111111111111111111111111111111\n",
        )
        .unwrap();

        let clean_audit = audit_tree_for_key_leaks(&temp).expect("audit clean temp tree");
        assert!(clean_audit.is_clean());
        assert_eq!(clean_audit.scanned_log_files, 1);

        // 3. Inject a private key into the log file -> tree search gate MUST go red.
        let leaked_hex = "ab".repeat(32);
        let poisoned_log =
            format!("2026-11-02T00:00:01Z WARN signer debug dump private_key={leaked_hex}\n");
        std::fs::write(&log_path, poisoned_log).unwrap();

        let poisoned_audit = audit_tree_for_key_leaks(&temp).expect("audit poisoned temp tree");
        assert!(
            !poisoned_audit.is_clean(),
            "key in a log must turn the tree-search audit red"
        );
        assert_eq!(poisoned_audit.findings.len(), 1);
        assert_eq!(
            poisoned_audit.findings[0].kind,
            KeyLeakKind::HexPrivateKeyInLog
        );
        assert_eq!(
            poisoned_audit.findings[0].path,
            "docs/evidence/w0/daemon.log"
        );
        assert_eq!(poisoned_audit.findings[0].line_number, 1);
        assert!(
            !poisoned_audit.findings[0]
                .redacted_preview
                .contains(&leaked_hex),
            "redacted preview must never echo the leaked secret"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn scanner_detects_mnemonic_pem_and_seed_leaks_and_ignores_public_hashes() {
        let clean_lines = concat!(
            "INFO block committed block_hash=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "INFO validator vote public_key=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n",
            "INFO state updated state_root=cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\n",
        );
        assert!(scan_text_for_key_leaks("node.log", clean_lines).is_empty());

        let words = ["abandon"; 12].join(" ");
        let mnemonic_line = format!("ERROR wallet backup mnemonic=\"{words}\"");
        let f_mnemonic = scan_text_for_key_leaks("wallet.log", &mnemonic_line);
        assert_eq!(f_mnemonic.len(), 1);
        assert_eq!(f_mnemonic[0].kind, KeyLeakKind::MnemonicInLog);

        let pem_line = format!("{}ED25519 {}", "-----BEGIN ", "PRIVATE KEY-----");
        let f_pem = scan_text_for_key_leaks("key.pem", &pem_line);
        assert_eq!(f_pem.len(), 1);
        assert_eq!(f_pem[0].kind, KeyLeakKind::PemPrivateKey);

        let seed_hex = "cd".repeat(32);
        let seed_line = format!("{{\"master_seed\": \"{seed_hex}\"}}");
        let f_seed = scan_text_for_key_leaks("trace.log", &seed_line);
        assert_eq!(f_seed.len(), 1);
        assert_eq!(f_seed[0].kind, KeyLeakKind::RawSeedInLog);
    }

    #[test]
    fn refusal_exits_stub_without_arguments() {
        let root = default_workspace_root();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_with_workspace(&[], &root, &mut out, &mut err);
        assert_eq!(code, EXIT_STUB);
        assert!(out.is_empty());
        let err_str = String::from_utf8(err).unwrap();
        assert!(err_str.contains("refusing to run"));
    }

    #[test]
    fn refusal_names_every_missing_capability() {
        let text = format_refusal(Some("transfer"));
        for cap in MISSING_CAPABILITIES {
            assert!(
                text.contains(cap.name) && text.contains(cap.phase),
                "refusal missing {} ({})",
                cap.name,
                cap.phase
            );
        }
    }

    #[test]
    fn report_refuses_a_tree_that_is_not_a_workspace() {
        let temp = make_temp_dir("not-a-workspace");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let args = vec![
            "report".to_string(),
            "--workspace".to_string(),
            temp.display().to_string(),
        ];
        let code = run_with_workspace(&args, &temp, &mut out, &mut err);
        assert_eq!(code, EXIT_NO_WORKSPACE);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn documentation_is_not_a_public_item() {
        let sample = "// pub fn nope()\n/// pub struct NotReal\npub fn real_one() {}\n";
        assert_eq!(count_public_items(sample), 1);
    }
}
