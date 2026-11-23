//! E9 bridge: an IBC-like light-client bridge with limits, pause and a
//! finalized-checkpoint client for AuraLite OS (ECOSYSTEM_PLAN E9,
//! decision D-E4 — the bridge is the most dangerous component).
//!
//! # Light client (Core Verification, IBC/Tendermint shape)
//!
//! A header is accepted only when it **chains** onto the trusted header
//! (parent hash), is **newer** than the trusted height, and its commit
//! carries ed25519 signatures of **more than two thirds** of the
//! validator power over the header hash. Every listed signature is
//! verified — an unknown signer (the MITM appending its own key) or a
//! bit-flipped signature is rejected as
//! `E9: forged header signature (…)`; there is no early stop, so a bad
//! surplus signature is never ignored. A commit without quorum is
//! `E9: commit lacks quorum`. Each accepted header is a **finalized
//! checkpoint** (the model has instant 2/3 finality).
//!
//! # Bridge protections (D-E4, incident lessons)
//!
//! Every cross-chain message is processed at most once (`message already
//! processed` — the Nomad replay lesson), under a per-transaction and a
//! daily velocity limit (the Nomad drain lesson: no limits → $190M in
//! 150 minutes), behind a guardian **pause** (circuit breaker), and the
//! reserve invariant `locked >= minted` is enforced (the Wormhole
//! lesson: a mint without a matching deposit). Withdrawals cannot
//! overdraw the reserve.
//!
//! # AuraLite OS client
//!
//! [`OsClient`] is the checkpoint-verification entry point the AuraLite
//! OS embeds: it verifies a checkpoint commit with the validator set and
//! accepts messages only up to the **finalized** checkpoint height
//! (`E9: checkpoint not finalized`). Full OS integration lives in the
//! AuraLite-OS repository (deviation recorded in the E9 report).
//!
//! The pinned scenario [`run_e9_scenario`] drives a deterministic remote
//! chain emulator (headers 1..5, three validators committing each),
//! then the attack lane: stale header, under-quorum commit, two MITM
//! forgeries, a forked parent, replay, limit breaches, a root mismatch,
//! pause, and over-reserve withdrawal. Its step log is cross-run
//! against the independent Python model (`tools/e9_bridge_check.py`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use aura_crypto::{ed25519_public_key, sign_raw, verify_raw};
use aura_state::sha256_bytes;

/// Ed25519 signature bytes.
pub type Sig = [u8; 64];

/// 32-byte digest / key material.
pub type Word32 = [u8; 32];

/// Basis of the quorum rule: signatures of more than 2/3 of the total
/// voting power are required (`total * 2 / 3 + 1`).
pub const E9_QUORUM_TWO_THIRDS: u128 = 2;

/// The deterministic per-transaction limit of the pinned scenario.
pub const E9_PER_TX_LIMIT: u128 = 1_000_000_000_000_000_000_000; // 1000 tokens

/// The deterministic daily velocity limit of the pinned scenario.
pub const E9_DAILY_LIMIT: u128 = 1_200_000_000_000_000_000_000; // 1200 tokens

/// Errors carry the exact `E9: …` named messages pinned by the suite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum E9Error {
    /// A commit signature is unknown or does not verify (MITM forgery).
    ForgedSignature {
        /// Offending signer id.
        signer: u32,
    },
    /// Valid signatures below the 2/3 quorum.
    Quorum {
        /// Power that signed validly.
        signed: u128,
        /// Power required.
        required: u128,
    },
    /// The header parent hash does not match the trusted header.
    NotChained {
        /// Rejected header height.
        height: u64,
    },
    /// The header does not advance the trusted height.
    NotNewer {
        /// Rejected header height.
        height: u64,
        /// Trusted height.
        trusted: u64,
    },
    /// The message is beyond the finalized checkpoint.
    NotFinalized {
        /// Message / checkpoint height.
        height: u64,
    },
    /// The message was proven against a different state root.
    RootMismatch {
        /// Message height.
        height: u64,
    },
    /// The guardian paused the bridge.
    Paused,
    /// Message id already processed (replay).
    AlreadyProcessed {
        /// Replayed message id.
        id: u64,
    },
    /// Amount above the per-transaction limit.
    PerTxLimit {
        /// Requested amount.
        amount: u128,
        /// Configured limit.
        limit: u128,
    },
    /// Amount above what the daily window still allows.
    DailyLimit {
        /// Requested amount.
        amount: u128,
        /// Remaining daily allowance.
        remaining: u128,
    },
    /// Withdrawal above the locked reserve (no overdraw).
    Reserve {
        /// Requested amount.
        amount: u128,
        /// Locked reserve.
        locked: u128,
    },
    /// Amounts must be positive.
    ZeroAmount,
}

impl fmt::Display for E9Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForgedSignature { signer } => {
                write!(f, "E9: forged header signature ({signer})")
            }
            Self::Quorum { signed, required } => {
                write!(f, "E9: commit lacks quorum ({signed}/{required})")
            }
            Self::NotChained { height } => {
                write!(f, "E9: header does not chain ({height})")
            }
            Self::NotNewer { height, trusted } => {
                write!(f, "E9: header not newer ({height}/{trusted})")
            }
            Self::NotFinalized { height } => {
                write!(f, "E9: checkpoint not finalized ({height})")
            }
            Self::RootMismatch { height } => {
                write!(f, "E9: message root mismatch ({height})")
            }
            Self::Paused => write!(f, "E9: bridge is paused"),
            Self::AlreadyProcessed { id } => {
                write!(f, "E9: message already processed ({id})")
            }
            Self::PerTxLimit { amount, limit } => {
                write!(f, "E9: transfer exceeds per-tx limit ({amount}/{limit})")
            }
            Self::DailyLimit { amount, remaining } => {
                write!(f, "E9: transfer exceeds daily limit ({amount}/{remaining})")
            }
            Self::Reserve { amount, locked } => {
                write!(f, "E9: withdraw exceeds reserve ({amount}/{locked})")
            }
            Self::ZeroAmount => write!(f, "E9: amount must be positive"),
        }
    }
}

impl std::error::Error for E9Error {}

fn sha256_32(data: &[u8]) -> Word32 {
    let h = sha256_bytes(data);
    let b = h.as_bytes();
    b[..32].try_into().expect("sha256 is 32 bytes")
}

/// Lowercase hex of a byte slice.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A remote chain header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Block height.
    pub height: u64,
    /// Parent header hash.
    pub parent: Word32,
    /// State root (commitment to the message set of this height).
    pub root: Word32,
    /// Deterministic timestamp.
    pub time: u64,
}

impl Header {
    /// Canonical signing/hash input: `e9h|height|parent|root|time`.
    pub fn canonical(&self) -> String {
        format!(
            "e9h|{}|{}|{}|{}",
            self.height,
            hex(&self.parent),
            hex(&self.root),
            self.time
        )
    }

    /// The header hash (sha256 of the canonical form).
    pub fn hash(&self) -> Word32 {
        sha256_32(self.canonical().as_bytes())
    }
}

/// One validator of the remote chain set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validator {
    /// Validator id (also the commit signer label).
    pub id: u32,
    /// Voting power.
    pub power: u128,
    /// Ed25519 verification key.
    pub pubkey: Word32,
}

/// A commit: signatures over a header hash, sorted by signer id.
pub type Commit = Vec<(u32, Sig)>;

/// The pinned validator seed derivation: sha256("aura-e9-validator-{i}").
pub fn validator_seed(id: u32) -> Word32 {
    sha256_32(format!("aura-e9-validator-{id}").as_bytes())
}

/// The pinned attacker seed (not a validator): sha256("aura-e9-attacker").
pub fn attacker_seed() -> Word32 {
    sha256_32(b"aura-e9-attacker")
}

/// Total voting power of a validator set.
pub fn total_power(validators: &[Validator]) -> u128 {
    validators.iter().map(|v| v.power).sum()
}

/// Required signing power: strictly more than two thirds.
pub fn required_power(validators: &[Validator]) -> u128 {
    total_power(validators) * E9_QUORUM_TWO_THIRDS / 3 + 1
}

/// Verify a commit for `header`: every signature must be from a known
/// validator and verify against the header hash (no early stop — a
/// forged surplus signature is a rejection, not noise), duplicates are
/// rejected, and the valid power must exceed 2/3.
pub fn verify_commit(
    validators: &[Validator],
    header: &Header,
    commit: &Commit,
) -> Result<(), E9Error> {
    let hash = header.hash();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut signed: u128 = 0;
    let mut ordered = commit.clone();
    ordered.sort_by_key(|(id, _)| *id);
    for (signer, sig) in ordered {
        let Some(v) = validators.iter().find(|v| v.id == signer) else {
            return Err(E9Error::ForgedSignature { signer });
        };
        if !seen.insert(signer) {
            return Err(E9Error::ForgedSignature { signer });
        }
        if verify_raw(&v.pubkey, &hash, &sig).is_err() {
            return Err(E9Error::ForgedSignature { signer });
        }
        signed += v.power;
    }
    let required = required_power(validators);
    if signed < required {
        return Err(E9Error::Quorum { signed, required });
    }
    Ok(())
}

/// The IBC-shaped light client: trusted header + validator set.
#[derive(Debug, Clone)]
pub struct LightClient {
    validators: Vec<Validator>,
    trusted_height: u64,
    trusted_hash: Word32,
    roots: BTreeMap<u64, Word32>,
}

impl LightClient {
    /// New light client trusting `genesis` and `validators`.
    pub fn new(validators: Vec<Validator>, genesis: Header) -> Self {
        let mut roots = BTreeMap::new();
        roots.insert(genesis.height, genesis.root);
        Self {
            validators,
            trusted_height: genesis.height,
            trusted_hash: genesis.hash(),
            roots,
        }
    }

    /// Update the trusted header with a quorum-signed successor.
    pub fn update(&mut self, header: &Header, commit: &Commit) -> Result<(), E9Error> {
        if header.height <= self.trusted_height {
            return Err(E9Error::NotNewer {
                height: header.height,
                trusted: self.trusted_height,
            });
        }
        if header.parent != self.trusted_hash {
            return Err(E9Error::NotChained {
                height: header.height,
            });
        }
        verify_commit(&self.validators, header, commit)?;
        self.trusted_height = header.height;
        self.trusted_hash = header.hash();
        self.roots.insert(header.height, header.root);
        Ok(())
    }

    /// Latest finalized checkpoint height.
    pub fn finalized_height(&self) -> u64 {
        self.trusted_height
    }

    /// State root recorded at `height`, if that header was accepted.
    pub fn root_at(&self, height: u64) -> Option<Word32> {
        self.roots.get(&height).copied()
    }

    /// The validator set this client trusts.
    pub fn validators(&self) -> &[Validator] {
        &self.validators
    }
}

/// Cross-chain message direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Lock on the remote chain, mint wrapped locally.
    Deposit,
    /// Burn wrapped locally, release from the reserve.
    Withdraw,
}

/// One cross-chain transfer message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    /// Unique message id (replay protection).
    pub id: u64,
    /// Direction.
    pub kind: Kind,
    /// Transfer amount (18 decimals).
    pub amount: u128,
}

/// The bridge: light client + limits + pause + reserve.
#[derive(Debug, Clone)]
pub struct Bridge {
    lc: LightClient,
    paused: bool,
    per_tx: u128,
    daily: u128,
    day: u64,
    spent: u128,
    processed: BTreeSet<u64>,
    locked: u128,
    minted: u128,
}

impl Bridge {
    /// New bridge over a trusted genesis and validator set.
    pub fn new(validators: Vec<Validator>, genesis: Header, per_tx: u128, daily: u128) -> Self {
        Self {
            lc: LightClient::new(validators, genesis),
            paused: false,
            per_tx,
            daily,
            day: 1,
            spent: 0,
            processed: BTreeSet::new(),
            locked: 0,
            minted: 0,
        }
    }

    /// Relay a header update (the light-client lane).
    pub fn update_header(&mut self, header: &Header, commit: &Commit) -> Result<(), E9Error> {
        self.lc.update(header, commit)
    }

    /// Guardian pause (circuit breaker).
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Guardian unpause.
    pub fn unpause(&mut self) {
        self.paused = false;
    }

    /// Whether the bridge is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Advance the velocity window day (resets the daily allowance).
    pub fn advance_day(&mut self, day: u64) {
        if day != self.day {
            self.day = day;
            self.spent = 0;
        }
    }

    /// Process one message proven against `root` at `height`.
    pub fn submit(&mut self, msg: &Msg, height: u64, root: Word32) -> Result<(), E9Error> {
        if self.paused {
            return Err(E9Error::Paused);
        }
        if self.processed.contains(&msg.id) {
            return Err(E9Error::AlreadyProcessed { id: msg.id });
        }
        if msg.amount == 0 {
            return Err(E9Error::ZeroAmount);
        }
        if height > self.lc.finalized_height() {
            return Err(E9Error::NotFinalized { height });
        }
        if self.lc.root_at(height) != Some(root) {
            return Err(E9Error::RootMismatch { height });
        }
        if msg.amount > self.per_tx {
            return Err(E9Error::PerTxLimit {
                amount: msg.amount,
                limit: self.per_tx,
            });
        }
        let remaining = self.daily.saturating_sub(self.spent);
        if msg.amount > remaining {
            return Err(E9Error::DailyLimit {
                amount: msg.amount,
                remaining,
            });
        }
        if msg.kind == Kind::Withdraw {
            if msg.amount > self.locked {
                return Err(E9Error::Reserve {
                    amount: msg.amount,
                    locked: self.locked,
                });
            }
            self.locked -= msg.amount;
            self.minted -= msg.amount;
        } else {
            self.locked += msg.amount;
            self.minted += msg.amount;
        }
        self.processed.insert(msg.id);
        self.spent += msg.amount;
        Ok(())
    }

    /// Locked reserve (source-side backing).
    pub fn locked(&self) -> u128 {
        self.locked
    }

    /// Wrapped supply minted locally.
    pub fn minted(&self) -> u128 {
        self.minted
    }

    /// A snapshot of the finalized checkpoint for the OS client.
    pub fn os_client(&self) -> OsClient {
        OsClient {
            checkpoint_height: self.lc.finalized_height(),
            checkpoint_root: self
                .lc
                .root_at(self.lc.finalized_height())
                .expect("finalized root recorded"),
        }
    }

    /// The underlying light client.
    pub fn light_client(&self) -> &LightClient {
        &self.lc
    }
}

/// The AuraLite OS side client: it checks the **finalized checkpoint**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsClient {
    /// Height of the finalized checkpoint the OS trusts.
    pub checkpoint_height: u64,
    /// State root of that checkpoint.
    pub checkpoint_root: Word32,
}

impl OsClient {
    /// Verify a checkpoint (header + commit) against the validator set —
    /// the OS embedding entry point. Forged commits are rejected exactly
    /// like the bridge lane.
    pub fn verify_checkpoint(
        validators: &[Validator],
        header: &Header,
        commit: &Commit,
    ) -> Result<(), E9Error> {
        verify_commit(validators, header, commit)
    }

    /// Accept a message only at or below the finalized checkpoint.
    pub fn accept(&self, height: u64) -> Result<(), E9Error> {
        if height > self.checkpoint_height {
            return Err(E9Error::NotFinalized { height });
        }
        Ok(())
    }

    /// Check that `root` matches the checkpoint root of `height`.
    pub fn check_root(&self, height: u64, root: Word32) -> Result<(), E9Error> {
        self.accept(height)?;
        if height == self.checkpoint_height && root != self.checkpoint_root {
            return Err(E9Error::RootMismatch { height });
        }
        Ok(())
    }
}

// ------------------------------------------------------------- scenario

/// The pinned E9 run: emulator lane + attack lane + limits + OS client.
#[derive(Debug, Clone)]
pub struct E9Run {
    /// Canonical step log (the digest input).
    pub steps: Vec<String>,
    /// MITM forgeries rejected in the attack lane.
    pub forged_rejected: u64,
    /// Finalized checkpoint height.
    pub finalized_height: u64,
    /// Final locked reserve.
    pub locked: u128,
    /// Final wrapped supply.
    pub minted: u128,
    /// Emulator lane marker.
    pub lane: &'static str,
}

fn make_validators() -> Vec<Validator> {
    (0..4u32)
        .map(|id| Validator {
            id,
            power: 10,
            pubkey: ed25519_public_key(&validator_seed(id)),
        })
        .collect()
}

fn make_header(height: u64, parent: Word32) -> Header {
    Header {
        height,
        parent,
        root: sha256_32(format!("e9-root-{height}").as_bytes()),
        time: height * 10,
    }
}

fn signed_commit(signers: &[u32], header: &Header, flip: Option<u32>) -> Commit {
    let hash = header.hash();
    let mut commit = Commit::new();
    for id in signers {
        let mut sig = sign_raw(&validator_seed(*id), &hash);
        if Some(*id) == flip {
            sig[0] ^= 0x01;
        }
        commit.push((*id, sig));
    }
    commit.sort_by_key(|(id, _)| *id);
    commit
}

/// Run the pinned scenario and return its step log.
pub fn run_e9_scenario() -> E9Run {
    let validators = make_validators();
    let mut steps: Vec<String> = Vec::new();
    let mut forged_rejected = 0u64;

    let genesis = make_header(0, [0u8; 32]);
    let mut bridge = Bridge::new(
        validators.clone(),
        genesis.clone(),
        E9_PER_TX_LIMIT,
        E9_DAILY_LIMIT,
    );
    let mut trusted_hash = genesis.hash();

    // -- emulator lane: headers 1..5 committed by validators 0,1,2.
    let mut headers: BTreeMap<u64, Header> = BTreeMap::new();
    for height in 1..=5u64 {
        let header = make_header(height, trusted_hash);
        let commit = signed_commit(&[0, 1, 2], &header, None);
        bridge
            .update_header(&header, &commit)
            .expect("honest header");
        trusted_hash = header.hash();
        headers.insert(height, header);
        steps.push(format!("header|{height}|ok"));
    }

    // -- attack lane
    // Stale header (height 5 again).
    let err = bridge
        .update_header(&headers[&5], &signed_commit(&[0, 1, 2], &headers[&5], None))
        .expect_err("stale header");
    steps.push(format!("header|5|err|{err}"));

    // Under-quorum commit for height 6 (only 20 of 27 power).
    let h6 = make_header(6, trusted_hash);
    let err = bridge
        .update_header(&h6, &signed_commit(&[0, 1], &h6, None))
        .expect_err("under quorum");
    steps.push(format!("header|6|err|{err}"));

    // MITM forgery A: attacker appends its own signature (id 99).
    let mut forged_a = signed_commit(&[0, 1, 2], &h6, None);
    forged_a.push((99, sign_raw(&attacker_seed(), &h6.hash())));
    let err = bridge
        .update_header(&h6, &forged_a)
        .expect_err("unknown signer");
    steps.push(format!("forged|6|err|{err}"));
    forged_rejected += 1;

    // MITM forgery B: bit-flipped signature of validator 1.
    let err = bridge
        .update_header(&h6, &signed_commit(&[0, 1, 2], &h6, Some(1)))
        .expect_err("bad signature");
    steps.push(format!("forged|6|err|{err}"));
    forged_rejected += 1;

    // Forked parent: valid commit over a header that does not chain.
    let h7 = make_header(7, [7u8; 32]);
    let err = bridge
        .update_header(&h7, &signed_commit(&[0, 1, 2], &h7, None))
        .expect_err("fork");
    steps.push(format!("chain|7|err|{err}"));

    // -- transfers at finalized heights
    let root3 = headers[&3].root;
    let root4 = headers[&4].root;
    let root5 = headers[&5].root;
    let one = 1_000_000_000_000_000_000u128;

    let d1 = Msg {
        id: 1,
        kind: Kind::Deposit,
        amount: 500 * one,
    };
    bridge.submit(&d1, 3, root3).expect("deposit 1");
    steps.push(format!("deposit|1|{}|ok", d1.amount));

    let d2 = Msg {
        id: 2,
        kind: Kind::Deposit,
        amount: 400 * one,
    };
    bridge.submit(&d2, 4, root4).expect("deposit 2");
    steps.push(format!("deposit|2|{}|ok", d2.amount));

    let w3 = Msg {
        id: 3,
        kind: Kind::Withdraw,
        amount: 100 * one,
    };
    bridge.submit(&w3, 5, root5).expect("withdraw 3");
    steps.push(format!("withdraw|3|{}|ok", w3.amount));

    // Replay of message 1.
    let err = bridge.submit(&d1, 3, root3).expect_err("replay");
    steps.push(format!("deposit|1|{}|err|{err}", d1.amount));

    // Per-transaction limit.
    let d4 = Msg {
        id: 4,
        kind: Kind::Deposit,
        amount: 5_000 * one,
    };
    let err = bridge.submit(&d4, 5, root5).expect_err("per-tx");
    steps.push(format!("deposit|4|{}|err|{err}", d4.amount));

    // Daily limit (1000 of 1200 used, 200 remaining).
    let d5 = Msg {
        id: 5,
        kind: Kind::Deposit,
        amount: 400 * one,
    };
    let err = bridge.submit(&d5, 5, root5).expect_err("daily");
    steps.push(format!("deposit|5|{}|err|{err}", d5.amount));

    // Root mismatch: message proven against root 4 at height 3.
    let d8 = Msg {
        id: 8,
        kind: Kind::Deposit,
        amount: 10 * one,
    };
    let err = bridge.submit(&d8, 3, root4).expect_err("root");
    steps.push(format!("deposit|8|{}|err|{err}", d8.amount));

    // Pause / unpause (circuit breaker).
    bridge.pause();
    steps.push(String::from("pause|ok"));
    let d6 = Msg {
        id: 6,
        kind: Kind::Deposit,
        amount: 10 * one,
    };
    let err = bridge.submit(&d6, 5, root5).expect_err("paused");
    steps.push(format!("deposit|6|{}|err|{err}", d6.amount));
    bridge.unpause();
    steps.push(String::from("unpause|ok"));

    // -- OS client: finalized checkpoint only.
    let os = bridge.os_client();
    os.accept(5).expect("finalized");
    steps.push(String::from("os|5|ok"));
    let err = os.accept(6).expect_err("not finalized");
    steps.push(format!("os|6|err|{err}"));

    // -- day 2: fresh velocity window, over-reserve withdrawal refused.
    bridge.advance_day(2);
    steps.push(String::from("day|2|ok"));
    let w7 = Msg {
        id: 7,
        kind: Kind::Withdraw,
        amount: 900 * one,
    };
    let err = bridge.submit(&w7, 5, root5).expect_err("reserve");
    steps.push(format!("withdraw|7|{}|err|{err}", w7.amount));

    // -- reserve invariant
    let (locked, minted) = (bridge.locked(), bridge.minted());
    assert_eq!(locked, minted, "reserve invariant: locked == minted");
    steps.push(format!("invariant|locked={locked}|minted={minted}"));

    E9Run {
        steps,
        forged_rejected,
        finalized_height: bridge.light_client().finalized_height(),
        locked,
        minted,
        lane: "green",
    }
}

/// Digest of the scenario step log (sha256, hex).
pub fn scenario_digest(steps: &[String]) -> String {
    let blob = steps.join(";");
    sha256_bytes(blob.as_bytes()).to_hex()
}
