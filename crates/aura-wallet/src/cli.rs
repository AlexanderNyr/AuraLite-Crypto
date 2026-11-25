//! W2 — `aura-wallet` CLI core: `new/import/list/balance/transfer/sign/broadcast/stake/vote`.
//!
//! Design of record: `docs/w2-cli-plan.md`. Security invariants:
//!
//! 1. **Chain binding (the W2 named NC).** Every artifact the CLI builds or
//!    accepts carries the chain-id; `sign`/`broadcast`/`vote --verify-ballot`
//!    refuse artifacts bound to a foreign chain with a deterministic
//!    `error: foreign chain-id` (exit 65).
//! 2. **No secrets in argv or output.** Passwords, mnemonics and entropy enter
//!    via files (`--password-file`, `--mnemonic-file`, `--entropy-file`), never
//!    as flag values (W0 `T-W0-2`). The mnemonic is printed exactly once by
//!    `new`; commands print only addresses, public keys and txids.
//! 3. **Fail closed.** Bad password, tampered keystore, bad signature, wrong
//!    nonce, expired tx, insufficient funds and usage errors all refuse with a
//!    fixed message and exit code (see [`CliError`]).
//!
//! Commands that need chain state run against a [`Ledger`]; [`FileLedger`] is
//! the deterministic offline implementation the vectors use (D-W2-1).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::derivation::{
    self, Bip39NormalizationProfile, DerivedAccount, derive_account, generate_mnemonic,
};
use crate::hardware::{
    DeviceCommand, DeviceResponse, HardwareDevice, HardwareError, HostClaim, HumanDecision,
};
use crate::keystore::{KdfParams, Keystore};
use crate::multisig::{
    Approval, MsigError, MultisigDescriptor, MultisigProposal, ProposalAction, verify_approval,
};
use crate::offline::{self, OfflineError};
use crate::secret::Password;
use crate::web::{self, UiClaim, WebError};
use aura_tx::{Tx, TxKind, TxParams, UnsignedTx, signer_address};
use aura_types::{Address, Amount, ChainId, Domain, Gas, Height, Nonce};

/// The phase these numbers belong to.
pub const PHASE: &str = "W2";

/// Exit code on success.
pub const EXIT_OK: i32 = 0;
/// Exit code on usage errors.
pub const EXIT_USAGE: i32 = 64;
/// Exit code on policy refusals (foreign chain-id, bad password, ...).
pub const EXIT_REFUSED: i32 = 65;
/// Exit code on ledger/keystore/crypto failures.
pub const EXIT_FAIL: i32 = 69;

/// Canonical prefix of `stake` payloads (D-W2-3).
pub const STAKE_PAYLOAD_PREFIX: &[u8] = b"AuraLite-Crypto/wallet-stake-v1\x00";
/// Canonical prefix of governance ballots (D-W2-2).
pub const BALLOT_PREFIX: &[u8] = b"AuraLite-Crypto/wallet-ballot-v1\x00";

/// The nine wallet commands the CLI implements.
pub const COMMANDS: [&str; 18] = [
    "new",
    "import",
    "list",
    "balance",
    "transfer",
    "sign",
    "broadcast",
    "stake",
    "vote",
    "tx-prepare",
    "tx-sign",
    "tx-verify",
    "qr-split",
    "qr-join",
    "msig-init",
    "msig-propose",
    "msig-approve",
    "msig-collect",
];

/// `stake` action tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StakeAction {
    /// Delegate stake to a validator.
    Delegate,
    /// Withdraw delegated stake.
    Undelegate,
}

impl StakeAction {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Delegate => 0,
            Self::Undelegate => 1,
        }
    }

    /// Parse from the CLI spelling.
    pub fn parse_cli(text: &str) -> Option<Self> {
        match text {
            "delegate" => Some(Self::Delegate),
            "undelegate" => Some(Self::Undelegate),
            _ => None,
        }
    }
}

/// Governance ballot choice (wallet-level, D-W2-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BallotChoice {
    /// Vote yes.
    Yes,
    /// Vote no.
    No,
    /// Abstain.
    Abstain,
    /// No with veto.
    NoWithVeto,
}

impl BallotChoice {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Yes => 0,
            Self::No => 1,
            Self::Abstain => 2,
            Self::NoWithVeto => 3,
        }
    }

    /// Parse from the CLI spelling.
    pub fn parse_cli(text: &str) -> Option<Self> {
        match text {
            "yes" => Some(Self::Yes),
            "no" => Some(Self::No),
            "abstain" => Some(Self::Abstain),
            "veto" | "no-with-veto" => Some(Self::NoWithVeto),
            _ => None,
        }
    }

    /// CLI spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Abstain => "abstain",
            Self::NoWithVeto => "veto",
        }
    }
}

/// Deterministic CLI failure with a fixed message and exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// Usage error (exit 64).
    Usage(String),
    /// A chain-bound artifact was built for another chain (exit 65).
    ForeignChainId {
        /// Chain-id found in the artifact.
        found: String,
        /// Chain-id the wallet is configured for.
        expected: String,
    },
    /// Wrong password (exit 65, uniform message).
    BadPassword,
    /// Policy refusal with a fixed reason (exit 65).
    Refused(String),
    /// Fixed invalid-artifact message (exit 65, W3 envelopes).
    Invalid(String),
    /// Keystore file failure (exit 69).
    Keystore(String),
    /// Ledger failure (exit 69).
    Ledger(String),
    /// Cryptographic failure (exit 69).
    Crypto(String),
}

impl CliError {
    /// Process exit code for this failure.
    #[must_use]
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Usage(_) => EXIT_USAGE,
            Self::ForeignChainId { .. }
            | Self::BadPassword
            | Self::Refused(_)
            | Self::Invalid(_) => EXIT_REFUSED,
            Self::Keystore(_) | Self::Ledger(_) | Self::Crypto(_) => EXIT_FAIL,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(m) => write!(f, "error: usage: {m}"),
            Self::ForeignChainId { found, expected } => {
                write!(f, "error: foreign chain-id: tx={found}, wallet={expected}")
            }
            Self::BadPassword => write!(f, "error: bad password"),
            Self::Refused(m) => write!(f, "error: refused: {m}"),
            Self::Invalid(m) => write!(f, "error: {m}"),
            Self::Keystore(m) => write!(f, "error: keystore: {m}"),
            Self::Ledger(m) => write!(f, "error: ledger: {m}"),
            Self::Crypto(m) => write!(f, "error: crypto: {m}"),
        }
    }
}

/// Canonical `stake` payload: prefix || action tag || validator (20 bytes) || LE amount.
#[must_use]
pub fn stake_payload(action: StakeAction, validator: &Address, amount: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(STAKE_PAYLOAD_PREFIX.len() + 1 + 20 + 8);
    out.extend_from_slice(STAKE_PAYLOAD_PREFIX);
    out.push(action.tag());
    out.extend_from_slice(validator.as_bytes());
    out.extend_from_slice(&amount.to_le_bytes());
    out
}

/// Canonical ballot signing bytes:
/// prefix || u8 chain length || chain || LE proposal id || voter (20) || choice tag || LE height.
#[must_use]
pub fn ballot_signing_bytes(
    chain: &str,
    proposal_id: u64,
    voter: &Address,
    choice: BallotChoice,
    cast_height: u64,
) -> Vec<u8> {
    let chain_bytes = chain.as_bytes();
    let mut out = Vec::with_capacity(BALLOT_PREFIX.len() + 1 + chain_bytes.len() + 8 + 20 + 1 + 8);
    out.extend_from_slice(BALLOT_PREFIX);
    out.push(u8::try_from(chain_bytes.len()).expect("chain-id fits u8"));
    out.extend_from_slice(chain_bytes);
    out.extend_from_slice(&proposal_id.to_le_bytes());
    out.extend_from_slice(voter.as_bytes());
    out.push(choice.tag());
    out.extend_from_slice(&cast_height.to_le_bytes());
    out
}

/// A signed wallet-level governance ballot (D-W2-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ballot {
    /// Chain the ballot is bound to.
    pub chain: String,
    /// Target proposal.
    pub proposal_id: u64,
    /// Voter address.
    pub voter: Address,
    /// Vote choice.
    pub choice: BallotChoice,
    /// Height the ballot claims to be cast at.
    pub cast_height: u64,
    /// Ed25519 public key of the voter.
    pub public_key: [u8; 32],
    /// Ed25519 signature (domain-separated over the chain).
    pub signature: [u8; 64],
}

impl Ballot {
    /// Signing bytes of this ballot's fields.
    #[must_use]
    pub fn signing_bytes(&self) -> Vec<u8> {
        ballot_signing_bytes(
            &self.chain,
            self.proposal_id,
            &self.voter,
            self.choice,
            self.cast_height,
        )
    }

    /// Sign a fresh ballot with the account's Ed25519 seed.
    pub fn sign(
        seed: &[u8; 32],
        chain: &str,
        proposal_id: u64,
        voter: Address,
        choice: BallotChoice,
        cast_height: u64,
    ) -> Self {
        let bytes = ballot_signing_bytes(chain, proposal_id, &voter, choice, cast_height);
        let public_key = aura_crypto::ed25519_public_key(seed);
        let signature = aura_crypto::sign(seed, chain, Domain::Vote, &bytes);
        Self {
            chain: chain.to_string(),
            proposal_id,
            voter,
            choice,
            cast_height,
            public_key,
            signature,
        }
    }

    /// Verify signature and chain binding against `expected_chain`.
    ///
    /// # Errors
    ///
    /// [`CliError::ForeignChainId`] when the ballot is bound to another chain,
    /// [`CliError::Crypto`] when the signature or key does not verify.
    pub fn verify(&self, expected_chain: &str) -> Result<(), CliError> {
        if self.chain != expected_chain {
            return Err(CliError::ForeignChainId {
                found: self.chain.clone(),
                expected: expected_chain.to_string(),
            });
        }
        let bytes = self.signing_bytes();
        if aura_crypto::verify(
            &self.public_key,
            &self.chain,
            Domain::Vote,
            &bytes,
            &self.signature,
        )
        .is_err()
        {
            return Err(CliError::Crypto(
                "ballot signature does not verify".to_string(),
            ));
        }
        if signer_address(&self.public_key) != self.voter {
            return Err(CliError::Crypto(
                "ballot voter does not match public key".to_string(),
            ));
        }
        Ok(())
    }

    /// Canonical document bytes: signing bytes || public key || signature.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.signing_bytes();
        out.extend_from_slice(&self.public_key);
        out.extend_from_slice(&self.signature);
        out
    }

    /// Parse a canonical ballot document.
    ///
    /// # Errors
    ///
    /// [`CliError::Refused`] on malformed bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CliError> {
        let refused = |m: &str| CliError::Refused(format!("malformed ballot: {m}"));
        if bytes.len() < BALLOT_PREFIX.len() + 1 + 8 + 20 + 1 + 8 + 32 + 64 {
            return Err(refused("too short"));
        }
        if &bytes[..BALLOT_PREFIX.len()] != BALLOT_PREFIX {
            return Err(refused("bad prefix"));
        }
        let mut i = BALLOT_PREFIX.len();
        let chain_len = usize::from(bytes[i]);
        i += 1;
        if bytes.len() < i + chain_len + 8 + 20 + 1 + 8 + 32 + 64 {
            return Err(refused("truncated chain"));
        }
        let chain = std::str::from_utf8(&bytes[i..i + chain_len])
            .map_err(|_| refused("chain not utf-8"))?
            .to_string();
        i += chain_len;
        let proposal_id = u64::from_le_bytes(bytes[i..i + 8].try_into().expect("8 bytes"));
        i += 8;
        let voter = Address::from_slice(&bytes[i..i + 20]).map_err(|_| refused("bad voter"))?;
        i += 20;
        let choice = match bytes[i] {
            0 => BallotChoice::Yes,
            1 => BallotChoice::No,
            2 => BallotChoice::Abstain,
            3 => BallotChoice::NoWithVeto,
            _ => return Err(refused("bad choice tag")),
        };
        i += 1;
        let cast_height = u64::from_le_bytes(bytes[i..i + 8].try_into().expect("8 bytes"));
        i += 8;
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(&bytes[i..i + 32]);
        i += 32;
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&bytes[i..i + 64]);
        Ok(Self {
            chain,
            proposal_id,
            voter,
            choice,
            cast_height,
            public_key,
            signature,
        })
    }

    /// Ballot id: SHA-256 of the canonical document.
    #[must_use]
    pub fn id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.to_bytes());
        hex::encode(hasher.finalize())
    }
}

/// Chain-state source the CLI talks to (D-W2-1).
pub trait Ledger {
    /// Chain-id this ledger serves.
    fn chain_id(&self) -> &str;
    /// Balance of `addr` (0 for unknown addresses).
    fn balance_of(&self, addr: &Address) -> u64;
    /// Current nonce of `addr` (0 for unknown addresses).
    fn nonce_of(&self, addr: &Address) -> u64;
    /// Current ledger height (for expiry checks).
    fn height(&self) -> u64;
    /// Accept a fully verified tx: enforce nonce continuity, then append.
    ///
    /// # Errors
    ///
    /// [`CliError::Ledger`] when the chain, nonce or expiry check fails.
    fn broadcast(&mut self, tx: &Tx) -> Result<[u8; 32], CliError>;
}

/// Deterministic file-backed ledger used by the vectors and offline workflows.
#[derive(Debug, Clone)]
pub struct FileLedger {
    path: PathBuf,
    chain: String,
    height: u64,
    accounts: BTreeMap<String, (u64, u64)>,
    txs: Vec<String>,
}

impl FileLedger {
    /// Parse the ledger JSON document.
    ///
    /// # Errors
    ///
    /// [`CliError::Ledger`] on malformed documents.
    pub fn parse(path: &Path, text: &str) -> Result<Self, CliError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| CliError::Ledger(format!("parse: {e}")))?;
        let chain = value
            .get("chain")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| CliError::Ledger("missing chain".to_string()))?
            .to_string();
        let height = value
            .get("height")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let mut accounts = BTreeMap::new();
        if let Some(obj) = value.get("accounts").and_then(serde_json::Value::as_object) {
            for (addr, v) in obj {
                let balance = v
                    .get("balance")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| CliError::Ledger(format!("bad balance for {addr}")))?;
                let nonce = v
                    .get("nonce")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| CliError::Ledger(format!("bad nonce for {addr}")))?;
                accounts.insert(addr.clone(), (balance, nonce));
            }
        }
        let mut txs = Vec::new();
        if let Some(arr) = value.get("txs").and_then(serde_json::Value::as_array) {
            for t in arr {
                let s = t
                    .as_str()
                    .ok_or_else(|| CliError::Ledger("bad tx entry".to_string()))?;
                txs.push(s.to_string());
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            chain,
            height,
            accounts,
            txs,
        })
    }

    /// Load a ledger file from disk.
    ///
    /// # Errors
    ///
    /// [`CliError::Ledger`] when the file is missing or malformed.
    pub fn load(path: &Path) -> Result<Self, CliError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CliError::Ledger(format!("{}: {e}", path.display())))?;
        Self::parse(path, &text)
    }

    /// Whether the ledger knows this address (closed world).
    #[must_use]
    pub fn has_account(&self, addr: &Address) -> bool {
        self.accounts.contains_key(&addr.to_bech32())
    }

    /// Serialize to the canonical JSON document.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut accounts = serde_json::Map::new();
        for (addr, (balance, nonce)) in &self.accounts {
            accounts.insert(
                addr.clone(),
                serde_json::json!({ "balance": balance, "nonce": nonce }),
            );
        }
        let doc = serde_json::json!({
            "chain": self.chain,
            "height": self.height,
            "accounts": serde_json::Value::Object(accounts),
            "txs": self.txs,
        });
        let mut text = serde_json::to_string_pretty(&doc).expect("json serializes");
        text.push('\n');
        text
    }

    /// Persist to the backing file.
    ///
    /// # Errors
    ///
    /// [`CliError::Ledger`] on I/O failure.
    pub fn save(&self) -> Result<(), CliError> {
        std::fs::write(&self.path, self.to_json())
            .map_err(|e| CliError::Ledger(format!("{}: {e}", self.path.display())))
    }
}

impl Ledger for FileLedger {
    fn chain_id(&self) -> &str {
        &self.chain
    }

    fn balance_of(&self, addr: &Address) -> u64 {
        self.accounts
            .get(&addr.to_bech32())
            .map(|(balance, _)| *balance)
            .unwrap_or(0)
    }

    fn nonce_of(&self, addr: &Address) -> u64 {
        self.accounts
            .get(&addr.to_bech32())
            .map(|(_, nonce)| *nonce)
            .unwrap_or(0)
    }

    fn height(&self) -> u64 {
        self.height
    }

    fn broadcast(&mut self, tx: &Tx) -> Result<[u8; 32], CliError> {
        if tx.unsigned.chain_id.as_str() != self.chain {
            return Err(CliError::ForeignChainId {
                found: tx.unsigned.chain_id.as_str().to_string(),
                expected: self.chain.clone(),
            });
        }
        let addr = tx.unsigned.signer;
        let key = addr.to_bech32();
        let (balance, nonce) = self.accounts.get(&key).copied().unwrap_or((0, 0));
        if tx.unsigned.nonce != Nonce::new(nonce) {
            return Err(CliError::Ledger(format!(
                "nonce mismatch: account nonce is {nonce}"
            )));
        }
        let fee = tx.unsigned.max_fee;
        let cost = if tx.unsigned.kind == TxKind::Transfer && tx.unsigned.payload.len() == 8 {
            u64::from_le_bytes(tx.unsigned.payload[..8].try_into().expect("8 bytes"))
        } else {
            0
        };
        let total = cost.saturating_add(fee.units().min(u64::MAX as u128) as u64);
        if balance < total {
            return Err(CliError::Ledger("insufficient funds".to_string()));
        }
        self.accounts.insert(key, (balance - total, nonce + 1));
        let txid = tx.txid();
        self.txs.push(hex::encode(txid.as_bytes()));
        Ok(*txid.as_bytes())
    }
}

/// Parsed CLI arguments: `--flag value` pairs plus positionals.
#[derive(Debug, Clone, Default)]
pub struct Args {
    /// Flags in insertion-independent order.
    pub flags: BTreeMap<String, String>,
    /// Value-less flags.
    pub switches: BTreeMap<String, bool>,
    /// Positional arguments.
    pub positional: Vec<String>,
}

/// Parse `--flag value` pairs; bare `--flag` becomes a switch.
///
/// # Errors
///
/// [`CliError::Usage`] on `--` misuse.
pub fn parse_args(args: &[String]) -> Result<Args, CliError> {
    let mut out = Args::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(name) = a.strip_prefix("--") {
            if name.is_empty() {
                return Err(CliError::Usage("empty flag".to_string()));
            }
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                out.flags.insert(name.to_string(), args[i + 1].clone());
                i += 2;
            } else {
                out.switches.insert(name.to_string(), true);
                i += 1;
            }
        } else {
            out.positional.push(a.clone());
            i += 1;
        }
    }
    Ok(out)
}

impl Args {
    /// Required flag value.
    ///
    /// # Errors
    ///
    /// [`CliError::Usage`] when missing.
    pub fn need(&self, name: &str) -> Result<&str, CliError> {
        self.flags
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| CliError::Usage(format!("missing --{name}")))
    }

    /// Optional flag value.
    #[must_use]
    pub fn opt(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }

    /// Whether a switch is present.
    #[must_use]
    pub fn switch(&self, name: &str) -> bool {
        self.switches.get(name).copied().unwrap_or(false)
    }

    /// Reject flags outside the allowed set.
    ///
    /// # Errors
    ///
    /// [`CliError::Usage`] naming the unknown flag.
    pub fn only(&self, allowed: &[&str]) -> Result<(), CliError> {
        for key in self.flags.keys().chain(self.switches.keys()) {
            if !allowed.contains(&key.as_str()) {
                return Err(CliError::Usage(format!("unknown flag --{key}")));
            }
        }
        Ok(())
    }
}

fn read_password_file(path: &str) -> Result<Password, CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| CliError::Usage(format!("--password-file {path}: {e}")))?;
    let line = text
        .lines()
        .next()
        .unwrap_or("")
        .trim_end_matches(['\r', '\n']);
    Ok(Password::new(line))
}

fn read_hex_file(path: &str, what: &str) -> Result<Zeroizing<Vec<u8>>, CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| CliError::Usage(format!("--{what} {path}: {e}")))?;
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = hex::decode(&cleaned)
        .map_err(|e| CliError::Usage(format!("--{what} {path}: not hex: {e}")))?;
    Ok(Zeroizing::new(bytes))
}

fn read_mnemonic_file(path: &str) -> Result<String, CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| CliError::Usage(format!("--mnemonic-file {path}: {e}")))?;
    Ok(text.trim().to_string())
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::Usage(format!("--{flag} must be a u64")))
}

fn keystore_path(dir: &str, index: u32) -> PathBuf {
    Path::new(dir).join(format!("{index}.json"))
}

fn load_account(
    keystore_dir: &str,
    index: u32,
    password: &Password,
) -> Result<DerivedAccount, CliError> {
    let path = keystore_path(keystore_dir, index);
    let keystore = Keystore::load(&path).map_err(|e| CliError::Keystore(e.to_string()))?;
    let map_auth = |e: crate::keystore::KeystoreError| match e {
        crate::keystore::KeystoreError::AuthenticationFailed => CliError::BadPassword,
        other => CliError::Keystore(other.to_string()),
    };
    let seed = match keystore.decrypt_seed(password) {
        Ok(seed) => seed,
        Err(crate::keystore::KeystoreError::WeakKdfParameters) => {
            #[cfg(feature = "insecure-test-kdf")]
            {
                keystore
                    .decrypt_seed_insecure_for_testing(password)
                    .map_err(map_auth)?
            }
            #[cfg(not(feature = "insecure-test-kdf"))]
            {
                return Err(CliError::Keystore(
                    crate::keystore::KeystoreError::WeakKdfParameters.to_string(),
                ));
            }
        }
        Err(e) => return Err(map_auth(e)),
    };
    derive_account(&seed, index).map_err(|e| CliError::Crypto(e.to_string()))
}

fn kdf_from_args(args: &Args) -> Result<(KdfParams, bool), CliError> {
    match args.opt("kdf").unwrap_or("strong") {
        "strong" => Ok((KdfParams::default_argon2id(), false)),
        "testing" => {
            #[cfg(feature = "insecure-test-kdf")]
            {
                Ok((KdfParams::testing(), true))
            }
            #[cfg(not(feature = "insecure-test-kdf"))]
            {
                Err(CliError::Usage(
                    "--kdf testing requires the insecure-test-kdf feature".to_string(),
                ))
            }
        }
        other => Err(CliError::Usage(format!(
            "--kdf must be strong or testing, got {other}"
        ))),
    }
}

fn profile_from_args(args: &Args) -> Result<Bip39NormalizationProfile, CliError> {
    match args.opt("profile") {
        None => Ok(derivation::DEFAULT_BIP39_NORMALIZATION_PROFILE),
        Some(id) => Bip39NormalizationProfile::from_id(id).ok_or_else(|| {
            CliError::Usage(format!(
                "--profile must be a known normalization profile id, got {id}"
            ))
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn create_keystore_from_mnemonic(
    mnemonic: &str,
    mnemonic_passphrase: &str,
    profile: Bip39NormalizationProfile,
    password: &Password,
    kdf: KdfParams,
    testing: bool,
    keystore_dir: &str,
    index: u32,
    allow_reduced: bool,
) -> Result<DerivedAccount, CliError> {
    let words = mnemonic.split_whitespace().count();
    if words != 24 && !allow_reduced {
        return Err(CliError::Refused(format!(
            "reduced mnemonic refused ({words} words, expected 24; pass --accept-reduced-mnemonic to override)"
        )));
    }
    let seed = derivation::mnemonic_to_seed_with_profile(mnemonic, mnemonic_passphrase, profile)
        .map_err(|e| CliError::Crypto(e.to_string()))?;
    let account = derive_account(&seed, index).map_err(|e| CliError::Crypto(e.to_string()))?;
    let keystore = if testing {
        #[cfg(feature = "insecure-test-kdf")]
        {
            Keystore::encrypt_seed_insecure_for_testing(&seed, password, kdf)
                .map_err(|e| CliError::Keystore(e.to_string()))?
        }
        #[cfg(not(feature = "insecure-test-kdf"))]
        {
            let _ = kdf;
            return Err(CliError::Usage(
                "--kdf testing requires the insecure-test-kdf feature".to_string(),
            ));
        }
    } else {
        Keystore::create_from_mnemonic_with_profile(
            mnemonic,
            mnemonic_passphrase,
            profile,
            password,
            kdf,
        )
        .map_err(|e| CliError::Keystore(e.to_string()))?
    };
    let path = keystore_path(keystore_dir, index);
    if path.exists() {
        return Err(CliError::Refused(format!(
            "keystore entry {} already exists",
            path.display()
        )));
    }
    std::fs::create_dir_all(keystore_dir)
        .map_err(|e| CliError::Keystore(format!("{keystore_dir}: {e}")))?;
    keystore
        .save_new(&path)
        .map_err(|e| CliError::Keystore(e.to_string()))?;
    Ok(account)
}

fn account_index(args: &Args) -> Result<u32, CliError> {
    match args.opt("account") {
        None => Ok(0),
        Some(v) => v
            .parse::<u32>()
            .map_err(|_| CliError::Usage("--account must be a u32".to_string())),
    }
}

fn require_chain(artifact_chain: &str, wallet_chain: &str) -> Result<(), CliError> {
    if artifact_chain != wallet_chain {
        return Err(CliError::ForeignChainId {
            found: artifact_chain.to_string(),
            expected: wallet_chain.to_string(),
        });
    }
    Ok(())
}

fn open_ledger(args: &Args, chain: &str) -> Result<FileLedger, CliError> {
    let path = args.need("ledger")?;
    let ledger = FileLedger::load(Path::new(path))?;
    require_chain(ledger.chain_id(), chain)?;
    Ok(ledger)
}

fn tx_json_to_params(text: &str, signer: Address) -> Result<TxParams, CliError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| CliError::Usage(format!("tx json: {e}")))?;
    let get = |name: &str| -> Result<&serde_json::Value, CliError> {
        value
            .get(name)
            .ok_or_else(|| CliError::Usage(format!("tx json: missing {name}")))
    };
    let chain = get("chain")?
        .as_str()
        .ok_or_else(|| CliError::Usage("tx json: chain must be a string".to_string()))?
        .to_string();
    if let Some(signer_hex) = value.get("signer").and_then(serde_json::Value::as_str) {
        let want = hex::encode(signer.as_bytes());
        if signer_hex != want {
            return Err(CliError::Refused(format!(
                "signer mismatch: tx={signer_hex}, key={want}"
            )));
        }
    }
    let nonce = get("nonce")?
        .as_u64()
        .ok_or_else(|| CliError::Usage("tx json: nonce must be a u64".to_string()))?;
    let kind_tag = get("kind")?
        .as_u64()
        .ok_or_else(|| CliError::Usage("tx json: kind must be a u8".to_string()))?;
    let kind = u8::try_from(kind_tag)
        .ok()
        .and_then(TxKind::from_tag)
        .ok_or_else(|| CliError::Usage("tx json: unknown kind".to_string()))?;
    let payload = get("payload")?
        .as_str()
        .ok_or_else(|| CliError::Usage("tx json: payload must be hex".to_string()))?;
    let payload = hex::decode(payload)
        .map_err(|_| CliError::Usage("tx json: payload must be hex".to_string()))?;
    let gas = get("gas")?
        .as_u64()
        .ok_or_else(|| CliError::Usage("tx json: gas must be a u64".to_string()))?;
    let max_fee = get("max_fee")?
        .as_u64()
        .ok_or_else(|| CliError::Usage("tx json: max_fee must be a u64".to_string()))?;
    let tip = get("tip")?
        .as_u64()
        .ok_or_else(|| CliError::Usage("tx json: tip must be a u64".to_string()))?;
    let expiry = get("expiry")?
        .as_u64()
        .ok_or_else(|| CliError::Usage("tx json: expiry must be a u64".to_string()))?;
    Ok(TxParams {
        chain_id: ChainId::new(&chain)
            .map_err(|_| CliError::Usage("tx json: invalid chain-id".to_string()))?,
        signer,
        nonce: Nonce::new(nonce),
        kind,
        payload,
        gas_limit: Gas::new(gas),
        max_fee: Amount::from_units(u128::from(max_fee)),
        tip: Amount::from_units(u128::from(tip)),
        expiry: Height::new(expiry),
    })
}

fn build_and_sign(
    params: TxParams,
    account: &DerivedAccount,
) -> Result<(String, String), CliError> {
    let unsigned = UnsignedTx::new(params).map_err(|e| CliError::Crypto(e.to_string()))?;
    let signed = unsigned
        .sign(account.secret.as_bytes())
        .map_err(|e| CliError::Crypto(e.to_string()))?;
    let txid = hex::encode(signed.txid().as_bytes());
    let tx_hex = hex::encode(signed.to_bytes());
    Ok((txid, tx_hex))
}

fn load_signed_tx(text: &str) -> Result<Tx, CliError> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let envelope_prefix = offline::b64url_encode(offline::ENVELOPE_MAGIC);
    if cleaned.starts_with(&envelope_prefix) {
        let (kind, body) = offline::parse_envelope(&cleaned).map_err(offline_err)?;
        if kind != offline::KIND_SIGNED {
            return Err(CliError::Invalid("envelope: expected signed".to_string()));
        }
        let tx = Tx::from_canonical_bytes(&body)
            .map_err(|_| CliError::Invalid("envelope: bad encoding".to_string()))?;
        tx.verify_signature()
            .map_err(|_| CliError::Invalid("signature verification failed".to_string()))?;
        return Ok(tx);
    }
    let bytes = hex::decode(&cleaned).map_err(|_| CliError::Usage("tx must be hex".to_string()))?;
    let tx = Tx::from_canonical_bytes(&bytes)
        .map_err(|e| CliError::Refused(format!("malformed tx: {e}")))?;
    tx.verify_signature()
        .map_err(|_| CliError::Refused("tx signature does not verify".to_string()))?;
    Ok(tx)
}

fn offline_err(e: OfflineError) -> CliError {
    match e {
        OfflineError::ForeignChainId { found, expected } => {
            CliError::ForeignChainId { found, expected }
        }
        other => CliError::Invalid(other.to_string()),
    }
}

fn qr_size(args: &Args) -> Result<usize, CliError> {
    match args.opt("qr-size") {
        Some(v) => {
            let n = parse_u64(v, "qr-size")?;
            let n = usize::try_from(n)
                .map_err(|_| CliError::Usage("--qr-size must be 8..4096".to_string()))?;
            if !(8..=4096).contains(&n) {
                return Err(CliError::Usage("--qr-size must be 8..4096".to_string()));
            }
            Ok(n)
        }
        None => Ok(offline::DEFAULT_QR_SIZE),
    }
}

fn read_artifact(flag: &str, path: &str) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(|e| CliError::Usage(format!("{flag}: {e}")))
}

fn write_envelope_out(
    out_path: &str,
    qr_dir: Option<&str>,
    size: usize,
    text: &str,
) -> Result<usize, CliError> {
    std::fs::write(out_path, format!("{text}\n"))
        .map_err(|e| CliError::Usage(format!("--out: {e}")))?;
    let Some(dir) = qr_dir else {
        return Ok(0);
    };
    std::fs::create_dir_all(dir).map_err(|e| CliError::Usage(format!("--qr-dir: {e}")))?;
    let frames = offline::split_frames(text, size);
    for (i, frame) in frames.iter().enumerate() {
        let path = Path::new(dir).join(format!("frame-{i:03}.txt"));
        std::fs::write(&path, format!("{frame}\n"))
            .map_err(|e| CliError::Usage(format!("--qr-dir: {e}")))?;
    }
    Ok(frames.len())
}

fn cmd_new(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "keystore",
        "password-file",
        "chain",
        "entropy-file",
        "account",
        "kdf",
        "mnemonic-passphrase",
        "profile",
    ])?;
    let keystore_dir = args.need("keystore")?;
    let password = read_password_file(args.need("password-file")?)?;
    let chain = args.need("chain")?;
    let index = account_index(args)?;
    let (kdf, testing) = kdf_from_args(args)?;
    let profile = profile_from_args(args)?;
    let mnemonic_passphrase = args.opt("mnemonic-passphrase").unwrap_or("");
    let mnemonic = Zeroizing::new(match args.opt("entropy-file") {
        Some(path) => {
            let entropy = read_hex_file(path, "entropy-file")?;
            bip39::Mnemonic::from_entropy(&entropy)
                .map_err(|_| CliError::Usage("--entropy-file: invalid entropy length".to_string()))?
                .to_string()
        }
        None => generate_mnemonic(24).map_err(|e| CliError::Crypto(e.to_string()))?,
    });
    let account = create_keystore_from_mnemonic(
        &mnemonic,
        mnemonic_passphrase,
        profile,
        &password,
        kdf,
        testing,
        keystore_dir,
        index,
        false,
    )?;
    let _ = writeln!(out, "status=created");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "address={}", account.address.to_bech32());
    let _ = writeln!(out, "path={}", account.path);
    let _ = writeln!(out, "public_key={}", hex::encode(account.public_key));
    let _ = writeln!(
        out,
        "keystore={}",
        keystore_path(keystore_dir, index).display()
    );
    let _ = writeln!(out, "notice=write the mnemonic down; it is shown once");
    let _ = writeln!(out, "mnemonic={}", mnemonic.as_str());
    Ok(())
}

fn cmd_import(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "keystore",
        "password-file",
        "mnemonic-file",
        "chain",
        "account",
        "kdf",
        "mnemonic-passphrase",
        "profile",
        "accept-reduced-mnemonic",
    ])?;
    let keystore_dir = args.need("keystore")?;
    let password = read_password_file(args.need("password-file")?)?;
    let chain = args.need("chain")?;
    let mnemonic = read_mnemonic_file(args.need("mnemonic-file")?)?;
    let index = account_index(args)?;
    let (kdf, testing) = kdf_from_args(args)?;
    let profile = profile_from_args(args)?;
    let mnemonic_passphrase = args.opt("mnemonic-passphrase").unwrap_or("");
    let account = create_keystore_from_mnemonic(
        &mnemonic,
        mnemonic_passphrase,
        profile,
        &password,
        kdf,
        testing,
        keystore_dir,
        index,
        args.switch("accept-reduced-mnemonic"),
    )?;
    let _ = writeln!(out, "status=imported");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "address={}", account.address.to_bech32());
    let _ = writeln!(out, "path={}", account.path);
    let _ = writeln!(out, "public_key={}", hex::encode(account.public_key));
    let _ = writeln!(
        out,
        "keystore={}",
        keystore_path(keystore_dir, index).display()
    );
    Ok(())
}

fn cmd_list(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["keystore", "password-file"])?;
    let keystore_dir = args.need("keystore")?;
    let password = read_password_file(args.need("password-file")?)?;
    let mut entries: Vec<(u32, PathBuf)> = Vec::new();
    let dir = Path::new(keystore_dir);
    let read =
        std::fs::read_dir(dir).map_err(|e| CliError::Keystore(format!("{keystore_dir}: {e}")))?;
    for entry in read {
        let entry = entry.map_err(|e| CliError::Keystore(e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && let Ok(index) = stem.parse::<u32>()
        {
            entries.push((index, path));
        }
    }
    entries.sort();
    for (index, _) in &entries {
        let account = load_account(keystore_dir, *index, &password)?;
        let _ = writeln!(out, "entry={}", index);
        let _ = writeln!(out, "path={}", account.path);
        let _ = writeln!(out, "address={}", account.address.to_bech32());
        let _ = writeln!(out, "public_key={}", hex::encode(account.public_key));
    }
    let _ = writeln!(out, "entries={}", entries.len());
    Ok(())
}

fn cmd_balance(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["address", "ledger", "chain"])?;
    let chain = args.need("chain")?;
    let address = args.need("address")?;
    let addr = Address::from_bech32(address)
        .map_err(|_| CliError::Usage("--address must be bech32".to_string()))?;
    let ledger = open_ledger(args, chain)?;
    if !ledger.has_account(&addr) {
        return Err(CliError::Ledger("unknown address".to_string()));
    }
    let balance = ledger.balance_of(&addr);
    let nonce = ledger.nonce_of(&addr);
    let _ = writeln!(out, "address={}", addr.to_bech32());
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "balance={balance}");
    let _ = writeln!(out, "nonce={nonce}");
    Ok(())
}

fn fee_flags(args: &Args) -> Result<(u64, u64, u64, u64), CliError> {
    let gas = match args.opt("gas") {
        Some(v) => parse_u64(v, "gas")?,
        None => 21_000,
    };
    let max_fee = match args.opt("max-fee") {
        Some(v) => parse_u64(v, "max-fee")?,
        None => 1_000_000,
    };
    let tip = match args.opt("tip") {
        Some(v) => parse_u64(v, "tip")?,
        None => 0,
    };
    let expiry = match args.opt("expiry") {
        Some(v) => parse_u64(v, "expiry")?,
        None => 18_446_744_073_709_551_615,
    };
    if tip > max_fee {
        return Err(CliError::Usage(
            "--tip must not exceed --max-fee".to_string(),
        ));
    }
    Ok((gas, max_fee, tip, expiry))
}

fn cmd_transfer(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "keystore",
        "password-file",
        "ledger",
        "chain",
        "account",
        "to",
        "amount",
        "gas",
        "max-fee",
        "tip",
        "expiry",
    ])?;
    let chain = args.need("chain")?;
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    let to = Address::from_bech32(args.need("to")?)
        .map_err(|_| CliError::Usage("--to must be bech32".to_string()))?;
    let amount = parse_u64(args.need("amount")?, "amount")?;
    let (gas, max_fee, tip, expiry) = fee_flags(args)?;
    let ledger = open_ledger(args, chain)?;
    let nonce = ledger.nonce_of(&account.address);
    let balance = ledger.balance_of(&account.address);
    let total = amount.saturating_add(max_fee);
    if balance < total {
        return Err(CliError::Ledger("insufficient funds".to_string()));
    }
    let params = TxParams {
        chain_id: ChainId::new(chain)
            .map_err(|_| CliError::Usage("invalid --chain".to_string()))?,
        signer: account.address,
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: amount.to_le_bytes().to_vec(),
        gas_limit: Gas::new(gas),
        max_fee: Amount::from_units(u128::from(max_fee)),
        tip: Amount::from_units(u128::from(tip)),
        expiry: Height::new(expiry),
    };
    let (txid, tx_hex) = build_and_sign(params, &account)?;
    let _ = writeln!(out, "status=signed");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "from={}", account.address.to_bech32());
    let _ = writeln!(out, "to={}", to.to_bech32());
    let _ = writeln!(out, "amount={amount}");
    let _ = writeln!(out, "nonce={nonce}");
    let _ = writeln!(out, "txid={txid}");
    let _ = writeln!(out, "tx={tx_hex}");
    Ok(())
}

fn cmd_sign(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["tx-file", "keystore", "password-file", "chain", "account"])?;
    let chain = args.need("chain")?;
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    let text = std::fs::read_to_string(args.need("tx-file")?)
        .map_err(|e| CliError::Usage(format!("--tx-file: {e}")))?;
    let params = tx_json_to_params(&text, account.address)?;
    // The named W2 control: a tx built for a foreign chain is refused before signing.
    require_chain(params.chain_id.as_str(), chain)?;
    let (txid, tx_hex) = build_and_sign(params, &account)?;
    let _ = writeln!(out, "status=signed");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "txid={txid}");
    let _ = writeln!(out, "tx={tx_hex}");
    Ok(())
}

fn cmd_broadcast(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["tx-file", "ledger", "chain"])?;
    let chain = args.need("chain")?;
    let mut ledger = open_ledger(args, chain)?;
    let text = std::fs::read_to_string(args.need("tx-file")?)
        .map_err(|e| CliError::Usage(format!("--tx-file: {e}")))?;
    let tx = load_signed_tx(&text)?;
    require_chain(tx.unsigned.chain_id.as_str(), chain)?;
    if tx.unsigned.expiry != Height::new(u64::MAX)
        && tx.unsigned.expiry < Height::new(ledger.height())
    {
        return Err(CliError::Refused("tx expired".to_string()));
    }
    let txid = ledger.broadcast(&tx)?;
    ledger.save()?;
    let _ = writeln!(out, "status=broadcast");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "txid={}", hex::encode(txid));
    let _ = writeln!(out, "accepted=true");
    Ok(())
}

fn cmd_stake(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "action",
        "validator",
        "amount",
        "keystore",
        "password-file",
        "ledger",
        "chain",
        "account",
        "gas",
        "max-fee",
        "tip",
        "expiry",
    ])?;
    let chain = args.need("chain")?;
    let action = StakeAction::parse_cli(args.need("action")?)
        .ok_or_else(|| CliError::Usage("--action must be delegate or undelegate".to_string()))?;
    let validator = Address::from_bech32(args.need("validator")?)
        .map_err(|_| CliError::Usage("--validator must be bech32".to_string()))?;
    let amount = parse_u64(args.need("amount")?, "amount")?;
    if amount == 0 {
        return Err(CliError::Usage("--amount must be positive".to_string()));
    }
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    let (gas, max_fee, tip, expiry) = fee_flags(args)?;
    let ledger = open_ledger(args, chain)?;
    let nonce = ledger.nonce_of(&account.address);
    let kind = match action {
        StakeAction::Delegate => TxKind::Delegate,
        StakeAction::Undelegate => TxKind::Undelegate,
    };
    let params = TxParams {
        chain_id: ChainId::new(chain)
            .map_err(|_| CliError::Usage("invalid --chain".to_string()))?,
        signer: account.address,
        nonce: Nonce::new(nonce),
        kind,
        payload: stake_payload(action, &validator, amount),
        gas_limit: Gas::new(gas),
        max_fee: Amount::from_units(u128::from(max_fee)),
        tip: Amount::from_units(u128::from(tip)),
        expiry: Height::new(expiry),
    };
    let (txid, tx_hex) = build_and_sign(params, &account)?;
    let _ = writeln!(out, "status=signed");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(
        out,
        "action={}",
        match action {
            StakeAction::Delegate => "delegate",
            StakeAction::Undelegate => "undelegate",
        }
    );
    let _ = writeln!(out, "validator={}", validator.to_bech32());
    let _ = writeln!(out, "amount={amount}");
    let _ = writeln!(out, "txid={txid}");
    let _ = writeln!(out, "tx={tx_hex}");
    Ok(())
}

fn cmd_vote(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "proposal",
        "choice",
        "cast-height",
        "keystore",
        "password-file",
        "chain",
        "account",
        "verify-ballot",
    ])?;
    let chain = args.need("chain")?;
    if let Some(path) = args.opt("verify-ballot") {
        let bytes = read_hex_file(path, "verify-ballot")?;
        let ballot = Ballot::from_bytes(&bytes)?;
        ballot.verify(chain)?;
        let _ = writeln!(out, "verified=true");
        let _ = writeln!(out, "chain={}", ballot.chain);
        let _ = writeln!(out, "proposal={}", ballot.proposal_id);
        let _ = writeln!(out, "voter={}", ballot.voter.to_bech32());
        let _ = writeln!(out, "choice={}", ballot.choice.as_str());
        let _ = writeln!(out, "cast_height={}", ballot.cast_height);
        let _ = writeln!(out, "ballot_id={}", ballot.id());
        return Ok(());
    }
    let proposal_id = parse_u64(args.need("proposal")?, "proposal")?;
    let choice = BallotChoice::parse_cli(args.need("choice")?)
        .ok_or_else(|| CliError::Usage("--choice must be yes, no, abstain or veto".to_string()))?;
    let cast_height = match args.opt("cast-height") {
        Some(v) => parse_u64(v, "cast-height")?,
        None => 0,
    };
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    let ballot = Ballot::sign(
        account.secret.as_bytes(),
        chain,
        proposal_id,
        account.address,
        choice,
        cast_height,
    );
    let _ = writeln!(out, "status=signed");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "proposal={proposal_id}");
    let _ = writeln!(out, "voter={}", account.address.to_bech32());
    let _ = writeln!(out, "choice={}", choice.as_str());
    let _ = writeln!(out, "cast_height={cast_height}");
    let _ = writeln!(out, "ballot_id={}", ballot.id());
    let _ = writeln!(out, "ballot={}", hex::encode(ballot.to_bytes()));
    Ok(())
}

/// Run one CLI invocation, writing output to `out`/`err`; returns the exit code.
fn cmd_tx_prepare(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "chain", "from", "to", "amount", "nonce", "gas", "max-fee", "tip", "expiry", "out",
        "qr-dir", "qr-size",
    ])?;
    let chain = args.need("chain")?;
    let from = Address::from_bech32(args.need("from")?)
        .map_err(|_| CliError::Usage("--from must be bech32".to_string()))?;
    let _to = Address::from_bech32(args.need("to")?)
        .map_err(|_| CliError::Usage("--to must be bech32".to_string()))?;
    let amount = parse_u64(args.need("amount")?, "amount")?;
    let nonce = parse_u64(args.need("nonce")?, "nonce")?;
    let (gas, max_fee, tip, expiry) = fee_flags(args)?;
    let params = TxParams {
        chain_id: ChainId::new(chain)
            .map_err(|_| CliError::Usage("invalid --chain".to_string()))?,
        signer: from,
        nonce: Nonce::new(nonce),
        kind: TxKind::Transfer,
        payload: amount.to_le_bytes().to_vec(),
        gas_limit: Gas::new(gas),
        max_fee: Amount::from_units(u128::from(max_fee)),
        tip: Amount::from_units(u128::from(tip)),
        expiry: Height::new(expiry),
    };
    let unsigned = UnsignedTx::new(params).map_err(|e| CliError::Crypto(e.to_string()))?;
    let text = offline::encode_envelope(offline::KIND_UNSIGNED, &unsigned.to_bytes());
    let size = qr_size(args)?;
    let frames = write_envelope_out(args.need("out")?, args.opt("qr-dir"), size, &text)?;
    let _ = writeln!(out, "status=prepared");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "signer={}", from.to_bech32());
    let _ = writeln!(out, "frames={frames}");
    Ok(())
}

fn cmd_tx_sign(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "in",
        "out",
        "keystore",
        "password-file",
        "account",
        "chain",
        "qr-dir",
        "qr-size",
    ])?;
    let chain = args.need("chain")?;
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    let text = read_artifact("--in", args.need("in")?)?;
    let (kind, body) = offline::parse_envelope(&text).map_err(offline_err)?;
    if kind != offline::KIND_UNSIGNED {
        return Err(CliError::Invalid("envelope: expected unsigned".to_string()));
    }
    let unsigned = UnsignedTx::from_canonical_bytes(&body)
        .map_err(|_| CliError::Invalid("envelope: bad encoding".to_string()))?;
    if unsigned.chain_id.as_str() != chain {
        return Err(CliError::ForeignChainId {
            found: unsigned.chain_id.as_str().to_owned(),
            expected: chain.to_owned(),
        });
    }
    if unsigned.signer != account.address {
        return Err(CliError::Refused(format!(
            "signer mismatch: tx={}, key={}",
            hex::encode(unsigned.signer.as_bytes()),
            hex::encode(account.address.as_bytes())
        )));
    }
    let signed = unsigned
        .sign(account.secret.as_bytes())
        .map_err(|e| CliError::Crypto(e.to_string()))?;
    let signed_text = offline::encode_envelope(offline::KIND_SIGNED, &signed.to_bytes());
    let size = qr_size(args)?;
    let frames = write_envelope_out(args.need("out")?, args.opt("qr-dir"), size, &signed_text)?;
    let _ = writeln!(out, "status=signed");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "txid={}", hex::encode(signed.txid().as_bytes()));
    let _ = writeln!(out, "frames={frames}");
    Ok(())
}

fn cmd_tx_verify(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["in", "chain", "against"])?;
    let chain = args.need("chain")?;
    let text = read_artifact("--in", args.need("in")?)?;
    let against = match args.opt("against") {
        Some(path) => Some(read_artifact("--against", path)?),
        None => None,
    };
    let tx =
        offline::verify_signed_envelope(&text, chain, against.as_deref()).map_err(offline_err)?;
    let _ = writeln!(out, "status=verified");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "txid={}", hex::encode(tx.txid().as_bytes()));
    let _ = writeln!(out, "signer={}", tx.unsigned.signer.to_bech32());
    Ok(())
}

fn cmd_qr_split(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["in", "out-dir", "qr-size"])?;
    let text = read_artifact("--in", args.need("in")?)?;
    offline::parse_envelope(&text).map_err(offline_err)?;
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let size = qr_size(args)?;
    let dir = args.need("out-dir")?;
    std::fs::create_dir_all(dir).map_err(|e| CliError::Usage(format!("--out-dir: {e}")))?;
    let frames = offline::split_frames(&cleaned, size);
    for (i, frame) in frames.iter().enumerate() {
        let path = Path::new(dir).join(format!("frame-{i:03}.txt"));
        std::fs::write(&path, format!("{frame}\n"))
            .map_err(|e| CliError::Usage(format!("--out-dir: {e}")))?;
    }
    let _ = writeln!(out, "status=split");
    let _ = writeln!(out, "frames={}", frames.len());
    let _ = writeln!(out, "size={size}");
    Ok(())
}

fn cmd_qr_join(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["in-dir", "out"])?;
    let dir = Path::new(args.need("in-dir")?);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| CliError::Usage(format!("--in-dir: {e}")))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("txt"))
        .collect();
    paths.sort();
    let mut lines: Vec<String> = Vec::new();
    for path in &paths {
        let text =
            std::fs::read_to_string(path).map_err(|e| CliError::Usage(format!("--in-dir: {e}")))?;
        for line in text.lines() {
            lines.push(line.to_string());
        }
    }
    let joined = offline::join_frames(&lines).map_err(offline_err)?;
    offline::parse_envelope(&joined).map_err(offline_err)?;
    std::fs::write(args.need("out")?, format!("{joined}\n"))
        .map_err(|e| CliError::Usage(format!("--out: {e}")))?;
    let _ = writeln!(out, "status=joined");
    let _ = writeln!(out, "frames={}", paths.len());
    Ok(())
}

fn msig_err(e: MsigError) -> CliError {
    CliError::Invalid(e.to_string())
}

fn parse_pk_hex(text: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(text).ok()?;
    <[u8; 32]>::try_from(bytes).ok()
}

fn parse_sig_hex(text: &str) -> Option<[u8; 64]> {
    let bytes = hex::decode(text).ok()?;
    <[u8; 64]>::try_from(bytes).ok()
}

fn descriptor_to_json(d: &MultisigDescriptor) -> String {
    let members: Vec<String> = d.members.iter().map(hex::encode).collect();
    let value = serde_json::json!({
        "format": "AuraLite-MSIG-V1",
        "chain": d.chain,
        "threshold": d.threshold,
        "members": members,
        "timeout": d.timeout,
    });
    format!("{value:#}\n")
}

fn descriptor_from_text(text: &str) -> Result<MultisigDescriptor, MsigError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| MsigError::BadContract)?;
    let get = |name: &str| value.get(name).ok_or(MsigError::BadContract);
    let chain = get("chain")?
        .as_str()
        .ok_or(MsigError::BadContract)?
        .to_string();
    let threshold = get("threshold")?
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .ok_or(MsigError::BadContract)?;
    let timeout = get("timeout")?.as_u64().ok_or(MsigError::BadContract)?;
    let list = get("members")?.as_array().ok_or(MsigError::BadContract)?;
    let mut members: Vec<[u8; 32]> = Vec::with_capacity(list.len());
    for item in list {
        let hex_text = item.as_str().ok_or(MsigError::BadContract)?;
        members.push(parse_pk_hex(hex_text).ok_or(MsigError::BadContract)?);
    }
    if members.is_empty() || members.len() > 255 {
        return Err(MsigError::BadContract);
    }
    members.sort_unstable();
    if members.windows(2).any(|w| w[0] == w[1]) {
        return Err(MsigError::BadContract);
    }
    Ok(MultisigDescriptor {
        chain,
        threshold,
        members,
        timeout,
    })
}

fn proposal_to_json(p: &MultisigProposal) -> String {
    let mut value = serde_json::json!({
        "format": "AuraLite-MSIG-PROPOSAL-V1",
        "chain": p.chain,
        "contract": hex::encode(p.contract),
        "id": p.id,
        "expiry": p.expiry,
    });
    let obj = value.as_object_mut().expect("json object");
    match &p.action {
        ProposalAction::Transfer { to, amount } => {
            obj.insert("kind".to_string(), serde_json::json!("transfer"));
            obj.insert("to".to_string(), serde_json::json!(to.to_bech32()));
            obj.insert("amount".to_string(), serde_json::json!(amount));
        }
        ProposalAction::Rotate { descriptor } => {
            obj.insert("kind".to_string(), serde_json::json!("rotate"));
            obj.insert(
                "new_multisig".to_string(),
                serde_json::from_str(&descriptor_to_json(descriptor)).expect("descriptor json"),
            );
        }
    }
    format!("{value:#}\n")
}

fn proposal_from_text(text: &str) -> Result<MultisigProposal, MsigError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| MsigError::BadProposal)?;
    let get = |name: &str| value.get(name).ok_or(MsigError::BadProposal);
    let chain = get("chain")?
        .as_str()
        .ok_or(MsigError::BadProposal)?
        .to_string();
    let contract_hex = get("contract")?.as_str().ok_or(MsigError::BadProposal)?;
    let contract = parse_pk_hex(contract_hex).ok_or(MsigError::BadProposal)?;
    let id = get("id")?.as_u64().ok_or(MsigError::BadProposal)?;
    let expiry = get("expiry")?.as_u64().ok_or(MsigError::BadProposal)?;
    let kind = get("kind")?.as_str().ok_or(MsigError::BadProposal)?;
    let action = match kind {
        "transfer" => {
            let to_hex = get("to")?.as_str().ok_or(MsigError::BadProposal)?;
            let to = Address::from_bech32(to_hex).map_err(|_| MsigError::BadProposal)?;
            let amount = get("amount")?.as_u64().ok_or(MsigError::BadProposal)?;
            ProposalAction::Transfer { to, amount }
        }
        "rotate" => {
            let nested = get("new_multisig")?.to_string();
            let descriptor = descriptor_from_text(&nested)?;
            ProposalAction::Rotate { descriptor }
        }
        _ => return Err(MsigError::BadProposal),
    };
    Ok(MultisigProposal {
        chain,
        contract,
        id,
        expiry,
        action,
    })
}

fn approval_to_json(a: &Approval) -> String {
    let value = serde_json::json!({
        "format": "AuraLite-MSIG-APPROVAL-V1",
        "member": hex::encode(a.member),
        "proposal": hex::encode(&a.proposal),
        "sig": hex::encode(a.sig),
    });
    format!("{value:#}\n")
}

fn approval_from_text(text: &str) -> Result<Approval, MsigError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| MsigError::BadApproval)?;
    let get = |name: &str| value.get(name).ok_or(MsigError::BadApproval);
    let member = parse_pk_hex(get("member")?.as_str().ok_or(MsigError::BadApproval)?)
        .ok_or(MsigError::BadApproval)?;
    let proposal = hex::decode(get("proposal")?.as_str().ok_or(MsigError::BadApproval)?)
        .map_err(|_| MsigError::BadApproval)?;
    let sig = parse_sig_hex(get("sig")?.as_str().ok_or(MsigError::BadApproval)?)
        .ok_or(MsigError::BadApproval)?;
    Ok(Approval {
        member,
        proposal,
        sig,
    })
}

fn load_descriptor(flag: &str, path: &str) -> Result<MultisigDescriptor, CliError> {
    let text = read_artifact(flag, path)?;
    descriptor_from_text(&text).map_err(msig_err)
}

fn load_proposal_file(flag: &str, path: &str) -> Result<MultisigProposal, CliError> {
    let text = read_artifact(flag, path)?;
    proposal_from_text(&text).map_err(msig_err)
}

/// `hw-sign` — drive the hardware device protocol over one unsigned envelope.
///
/// The device screen is derived from the transaction bytes (D-W5-1) and the
/// human's answer decides the outcome (D-W5-3): `confirm` signs, `deny`
/// refuses (retry via `--human deny,confirm`), `blind` is always refused.
fn cmd_hw_sign(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "in",
        "keystore",
        "password-file",
        "account",
        "chain",
        "human",
        "show-recipient",
        "show-amount",
        "out",
    ])?;
    let chain = args.need("chain")?.to_owned();
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    let text = read_artifact("--in", args.need("in")?)?;
    let (kind, body) = offline::parse_envelope(&text).map_err(offline_err)?;
    if kind != offline::KIND_UNSIGNED {
        return Err(CliError::Invalid("envelope: expected unsigned".to_string()));
    }
    let unsigned = UnsignedTx::from_canonical_bytes(&body)
        .map_err(|_| CliError::Invalid("envelope: bad encoding".to_string()))?;
    let claim = match (args.opt("show-recipient"), args.opt("show-amount")) {
        (None, None) => None,
        (recipient, amount) => Some(HostClaim {
            recipient: recipient
                .map(|v| {
                    Address::from_bech32(v)
                        .map_err(|_| CliError::Usage("--show-recipient must be bech32".to_string()))
                })
                .transpose()?,
            amount: amount.map(|v| parse_u64(v, "show-amount")).transpose()?,
        }),
    };
    let mut device = HardwareDevice::new(account.secret.clone(), chain);
    let version = match device.exchange(DeviceCommand::GetVersion) {
        DeviceResponse::Version(v) => v,
        _ => return Err(CliError::Invalid("hardware: bad response".to_string())),
    };
    let _ = writeln!(out, "device={version}");
    let rounds = args.need("human")?;
    if rounds == "blind" {
        return match device.exchange(DeviceCommand::BlindSign { body: &body }) {
            DeviceResponse::Signed(tx) => write_signed_result(args, out, &tx),
            DeviceResponse::Refused(e) => Err(hardware_err(e)),
            _ => Err(CliError::Invalid("hardware: bad response".to_string())),
        };
    }
    let items: Vec<&str> = rounds.split(',').collect();
    for (i, item) in items.iter().enumerate() {
        let decision = match *item {
            "confirm" => HumanDecision::Confirm,
            "deny" => HumanDecision::Deny,
            _ => {
                return Err(CliError::Usage(
                    "--human must be confirm, deny or blind".to_string(),
                ));
            }
        };
        let frame = match device.exchange(DeviceCommand::BeginSign {
            unsigned: &unsigned,
            claim: claim.as_ref(),
        }) {
            DeviceResponse::Display(frame) => frame,
            DeviceResponse::Refused(e) => return Err(hardware_err(e)),
            _ => return Err(CliError::Invalid("hardware: bad response".to_string())),
        };
        for line in frame.render_lines() {
            let _ = writeln!(out, "{line}");
        }
        match device.exchange(DeviceCommand::Decide { decision }) {
            DeviceResponse::Signed(tx) => return write_signed_result(args, out, &tx),
            DeviceResponse::Refused(HardwareError::DeniedByUser) if i + 1 < items.len() => (),
            DeviceResponse::Refused(e) => return Err(hardware_err(e)),
            _ => return Err(CliError::Invalid("hardware: bad response".to_string())),
        }
    }
    Err(hardware_err(HardwareError::DeniedByUser))
}

/// Write the produced signature and the optional signed envelope.
fn write_signed_result(args: &Args, out: &mut dyn std::io::Write, tx: &Tx) -> Result<(), CliError> {
    let signed = offline::encode_envelope(offline::KIND_SIGNED, &tx.to_bytes());
    if let Some(path) = args.opt("out") {
        std::fs::write(path, &signed).map_err(|e| CliError::Usage(format!("--out {path}: {e}")))?;
    }
    let _ = writeln!(out, "status=signed");
    let _ = writeln!(out, "sig={}", hex::encode(tx.sig.bytes));
    Ok(())
}

/// Map a device refusal to the fixed CLI error vocabulary.
fn hardware_err(e: HardwareError) -> CliError {
    match e {
        HardwareError::ForeignChainId { found, expected } => {
            CliError::ForeignChainId { found, expected }
        }
        other => CliError::Invalid(format!("hardware: {other}")),
    }
}

/// `web-sign` — drive the web wallet signing flow over a deep link or an
/// unsigned envelope (W6).
///
/// The confirmation view is derived from the transaction bytes (D-W6-1);
/// signing requires that view (D-W6-2) and the request origin must pass the
/// phishing policy (D-W6-3).
fn cmd_web_sign(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "in",
        "deep-link",
        "origin",
        "keystore",
        "password-file",
        "account",
        "chain",
        "human",
        "out",
    ])?;
    let chain = args.need("chain")?.to_owned();
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    let _ = writeln!(out, "wallet={}", web::WEB_UI_VERSION);
    web::check_origin(args.need("origin")?).map_err(web_err)?;
    let (envelope, claim) = match (args.opt("deep-link"), args.opt("in")) {
        (Some(uri), None) => {
            let link = web::parse_deep_link(uri).map_err(web_err)?;
            if let Some(link_chain) = &link.chain
                && link_chain != &chain
            {
                return Err(CliError::ForeignChainId {
                    found: link_chain.clone(),
                    expected: chain.clone(),
                });
            }
            (link.envelope, link.claim)
        }
        (None, Some(path)) => (read_artifact("--in", path)?, UiClaim::default()),
        _ => {
            return Err(CliError::Usage(
                "exactly one of --in or --deep-link is required".to_string(),
            ));
        }
    };
    let secret = *account.secret.as_bytes();
    let rounds = args.need("human")?;
    if rounds == "blind" {
        return match web::sign_without_view(&envelope, &secret) {
            Ok(tx) => write_signed_result(args, out, &tx),
            Err(e) => Err(web_err(e)),
        };
    }
    let items: Vec<&str> = rounds.split(',').collect();
    for (i, item) in items.iter().enumerate() {
        let decision = match *item {
            "confirm" => true,
            "deny" => false,
            _ => {
                return Err(CliError::Usage(
                    "--human must be confirm, deny or blind".to_string(),
                ));
            }
        };
        let view = web::derive_view(&envelope, &chain).map_err(web_err)?;
        for line in &view.lines {
            let _ = writeln!(out, "{line}");
        }
        view.bind(&claim).map_err(web_err)?;
        if decision {
            let tx = web::sign_confirmed(&view, &secret).map_err(web_err)?;
            return write_signed_result(args, out, &tx);
        }
        if i + 1 == items.len() {
            return Err(web_err(WebError::DeniedByUser));
        }
    }
    Err(web_err(WebError::DeniedByUser))
}

/// Map a web wallet refusal to the fixed CLI error vocabulary.
fn web_err(e: WebError) -> CliError {
    match e {
        WebError::ForeignChainId { found, expected } => {
            CliError::ForeignChainId { found, expected }
        }
        other => CliError::Invalid(format!("web: {other}")),
    }
}

fn cmd_msig_init(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["chain", "threshold", "members", "timeout", "out"])?;
    let chain = args.need("chain")?;
    let threshold = parse_u64(args.need("threshold")?, "threshold")?;
    let threshold = u8::try_from(threshold)
        .map_err(|_| CliError::Usage("--threshold must be 1..=255".to_string()))?;
    if threshold == 0 {
        return Err(CliError::Usage("--threshold must be 1..=255".to_string()));
    }
    let timeout = parse_u64(args.need("timeout")?, "timeout")?;
    let mut members: Vec<[u8; 32]> = Vec::new();
    for item in args.need("members")?.split(',') {
        members.push(parse_pk_hex(item).ok_or_else(|| {
            CliError::Usage("--members must be comma-separated hex pubkeys".to_string())
        })?);
    }
    if members.is_empty() || members.len() > 255 {
        return Err(CliError::Usage(
            "--members must list 1..=255 keys".to_string(),
        ));
    }
    members.sort_unstable();
    if members.windows(2).any(|w| w[0] == w[1]) {
        return Err(CliError::Usage("duplicate member".to_string()));
    }
    if usize::from(threshold) > members.len() {
        return Err(CliError::Usage(
            "--threshold must not exceed members".to_string(),
        ));
    }
    let descriptor = MultisigDescriptor {
        chain: chain.to_string(),
        threshold,
        members,
        timeout,
    };
    std::fs::write(args.need("out")?, descriptor_to_json(&descriptor))
        .map_err(|e| CliError::Usage(format!("--out: {e}")))?;
    let _ = writeln!(out, "status=created");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "threshold={threshold}");
    let _ = writeln!(out, "members={}", descriptor.members.len());
    let _ = writeln!(out, "multisig_id={}", hex::encode(descriptor.contract_id()));
    Ok(())
}

fn cmd_msig_propose(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "multisig",
        "chain",
        "proposal-id",
        "expiry",
        "action",
        "to",
        "amount",
        "new-multisig",
        "out",
    ])?;
    let chain = args.need("chain")?;
    let descriptor = load_descriptor("--multisig", args.need("multisig")?)?;
    if descriptor.chain != chain {
        return Err(CliError::ForeignChainId {
            found: descriptor.chain.clone(),
            expected: chain.to_string(),
        });
    }
    let id = parse_u64(args.need("proposal-id")?, "proposal-id")?;
    let expiry = parse_u64(args.need("expiry")?, "expiry")?;
    if expiry > descriptor.timeout {
        return Err(CliError::Usage(
            "--expiry must not exceed contract timeout".to_string(),
        ));
    }
    let action = match args.need("action")? {
        "transfer" => {
            let to = Address::from_bech32(args.need("to")?)
                .map_err(|_| CliError::Usage("--to must be bech32".to_string()))?;
            let amount = parse_u64(args.need("amount")?, "amount")?;
            ProposalAction::Transfer { to, amount }
        }
        "rotate" => {
            let new_descriptor = load_descriptor("--new-multisig", args.need("new-multisig")?)?;
            if new_descriptor.chain != descriptor.chain {
                return Err(CliError::Usage("new contract chain must match".to_string()));
            }
            ProposalAction::Rotate {
                descriptor: new_descriptor,
            }
        }
        _ => {
            return Err(CliError::Usage(
                "--action must be transfer or rotate".to_string(),
            ));
        }
    };
    let proposal = MultisigProposal {
        chain: chain.to_string(),
        contract: descriptor.contract_id(),
        id,
        expiry,
        action,
    };
    let kind = match proposal.action {
        ProposalAction::Transfer { .. } => "transfer",
        ProposalAction::Rotate { .. } => "rotate",
    };
    std::fs::write(args.need("out")?, proposal_to_json(&proposal))
        .map_err(|e| CliError::Usage(format!("--out: {e}")))?;
    let _ = writeln!(out, "status=proposed");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "multisig_id={}", hex::encode(proposal.contract));
    let _ = writeln!(out, "proposal_id={id}");
    let _ = writeln!(out, "kind={kind}");
    Ok(())
}

fn cmd_msig_approve(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&[
        "proposal",
        "multisig",
        "keystore",
        "password-file",
        "account",
        "chain",
        "height",
        "out",
    ])?;
    let chain = args.need("chain")?;
    let descriptor = load_descriptor("--multisig", args.need("multisig")?)?;
    let proposal = load_proposal_file("--proposal", args.need("proposal")?)?;
    if proposal.chain != chain {
        return Err(CliError::ForeignChainId {
            found: proposal.chain.clone(),
            expected: chain.to_string(),
        });
    }
    if proposal.contract != descriptor.contract_id() {
        return Err(msig_err(MsigError::WrongContract));
    }
    let height = parse_u64(args.need("height")?, "height")?;
    if proposal.expiry < height {
        return Err(msig_err(MsigError::ProposalExpired));
    }
    let password = read_password_file(args.need("password-file")?)?;
    let index = account_index(args)?;
    let account = load_account(args.need("keystore")?, index, &password)?;
    if !descriptor.is_member(&account.public_key) {
        return Err(msig_err(MsigError::NotAMember));
    }
    let bytes = proposal.canonical_bytes();
    let sig = aura_crypto::sign(
        account.secret.as_bytes(),
        chain,
        aura_types::Domain::Contract,
        &bytes,
    );
    let approval = Approval {
        member: account.public_key,
        proposal: bytes,
        sig,
    };
    std::fs::write(args.need("out")?, approval_to_json(&approval))
        .map_err(|e| CliError::Usage(format!("--out: {e}")))?;
    let _ = writeln!(out, "status=approved");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "proposal_id={}", proposal.id);
    let _ = writeln!(out, "member={}", hex::encode(account.public_key));
    Ok(())
}

fn cmd_msig_collect(args: &Args, out: &mut dyn std::io::Write) -> Result<(), CliError> {
    args.only(&["proposal", "multisig", "approvals", "chain", "height"])?;
    let chain = args.need("chain")?;
    let descriptor = load_descriptor("--multisig", args.need("multisig")?)?;
    let proposal = load_proposal_file("--proposal", args.need("proposal")?)?;
    if proposal.chain != chain {
        return Err(CliError::ForeignChainId {
            found: proposal.chain.clone(),
            expected: chain.to_string(),
        });
    }
    if proposal.contract != descriptor.contract_id() {
        return Err(msig_err(MsigError::WrongContract));
    }
    let height = parse_u64(args.need("height")?, "height")?;
    if proposal.expiry < height {
        return Err(msig_err(MsigError::ProposalExpired));
    }
    let dir = Path::new(args.need("approvals")?);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| CliError::Usage(format!("--approvals: {e}")))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut seen: Vec<[u8; 32]> = Vec::new();
    let mut collected = 0usize;
    for path in &paths {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CliError::Usage(format!("--approvals: {e}")))?;
        let approval = approval_from_text(&text).map_err(msig_err)?;
        if !descriptor.is_member(&approval.member) {
            return Err(msig_err(MsigError::NotAMember));
        }
        if seen.contains(&approval.member) {
            return Err(msig_err(MsigError::DuplicateApproval));
        }
        seen.push(approval.member);
        verify_approval(&approval, &descriptor, &proposal).map_err(msig_err)?;
        collected += 1;
    }
    if collected < usize::from(descriptor.threshold) {
        return Err(msig_err(MsigError::ThresholdNotMet {
            collected,
            threshold: descriptor.threshold,
        }));
    }
    let _ = writeln!(out, "status=passed");
    let _ = writeln!(out, "chain={chain}");
    let _ = writeln!(out, "multisig_id={}", hex::encode(proposal.contract));
    let _ = writeln!(out, "proposal_id={}", proposal.id);
    let _ = writeln!(out, "signatures={collected}");
    let _ = writeln!(out, "threshold={}", descriptor.threshold);
    Ok(())
}

/// Execute one CLI invocation against the given streams; returns the exit code.
pub fn run(args: &[String], out: &mut dyn std::io::Write, err: &mut dyn std::io::Write) -> i32 {
    let mut dispatch = || -> Result<(), CliError> {
        let (command, rest) = args
            .split_first()
            .ok_or_else(|| CliError::Usage("missing command".to_string()))?;
        let parsed = parse_args(rest)?;
        match command.as_str() {
            "new" => cmd_new(&parsed, out),
            "import" => cmd_import(&parsed, out),
            "list" => cmd_list(&parsed, out),
            "balance" => cmd_balance(&parsed, out),
            "transfer" => cmd_transfer(&parsed, out),
            "sign" => cmd_sign(&parsed, out),
            "broadcast" => cmd_broadcast(&parsed, out),
            "stake" => cmd_stake(&parsed, out),
            "vote" => cmd_vote(&parsed, out),
            "tx-prepare" => cmd_tx_prepare(&parsed, out),
            "tx-sign" => cmd_tx_sign(&parsed, out),
            "tx-verify" => cmd_tx_verify(&parsed, out),
            "qr-split" => cmd_qr_split(&parsed, out),
            "qr-join" => cmd_qr_join(&parsed, out),
            "hw-sign" => cmd_hw_sign(&parsed, out),
            "web-sign" => cmd_web_sign(&parsed, out),
            "msig-init" => cmd_msig_init(&parsed, out),
            "msig-propose" => cmd_msig_propose(&parsed, out),
            "msig-approve" => cmd_msig_approve(&parsed, out),
            "msig-collect" => cmd_msig_collect(&parsed, out),
            other => Err(CliError::Usage(format!("unknown command {other}"))),
        }
    };
    match dispatch() {
        Ok(()) => EXIT_OK,
        Err(e) => {
            let _ = writeln!(err, "{e}");
            e.exit_code()
        }
    }
}
