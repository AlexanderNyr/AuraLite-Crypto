//! N6 DoS defence: pre-crypto frame size & message rate limits, per-peer
//! signature verification CPU budgets (signature-storm defence), and
//! cryptographic equivocation detection (double-proposal / double-vote at the
//! same `(height, round)`).
//!
//! Enforces `D-N3`: every network envelope passes strict size, rate,
//! signature-budget, identity (`PeerId == BLAKE3(pubkey)`), Ed25519 signature,
//! and equivocation checks; a single malicious peer sending 10,000 msg/s is
//! throttled and banned in O(1) before exhausting validator signature budgets.

use std::collections::BTreeMap;

use aura_crypto::{blake3_hash, verify_raw};

use crate::ban::{BanList, BanReason};
use crate::error::P2pError;
use crate::identity::{PeerId, PeerIdentity};

/// Domain tag for N6 signed ingress envelopes.
pub const DOS_INGRESS_DOMAIN: &[u8] = b"AuraLite-Crypto/p2p-dos-ingress-v1\0";

/// Default maximum ingress frame size in bytes (64 KiB).
pub const DEFAULT_MAX_DOS_FRAME_BYTES: usize = 64 * 1024;

/// Default sliding-window duration in milliseconds (1 second).
pub const DEFAULT_DOS_WINDOW_MS: u64 = 1_000;

/// Default maximum messages per peer per 1-second window.
pub const DEFAULT_MAX_MSGS_PER_SEC: usize = 200;

/// Default maximum Ed25519 signature verifications per peer per 1-second window.
pub const DEFAULT_MAX_SIGS_PER_SEC: usize = 100;

/// Default number of rate/size strikes before automatic `BanList` isolation.
pub const DEFAULT_VIOLATIONS_BEFORE_BAN: usize = 3;

/// Per-peer and per-frame DoS defence limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DosDefenceLimits {
    /// Maximum allowed wire frame size in bytes (checked before signature verification).
    pub max_frame_bytes: usize,
    /// Maximum messages accepted for inspection per peer per window.
    pub max_messages_per_window: usize,
    /// Maximum cryptographic signature verifications per peer per window.
    pub max_sig_verifications_per_window: usize,
    /// Sliding window duration in milliseconds.
    pub window_ms: u64,
    /// Number of rate/size violations before the peer is banned.
    pub violations_before_ban: usize,
}

impl Default for DosDefenceLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: DEFAULT_MAX_DOS_FRAME_BYTES,
            max_messages_per_window: DEFAULT_MAX_MSGS_PER_SEC,
            max_sig_verifications_per_window: DEFAULT_MAX_SIGS_PER_SEC,
            window_ms: DEFAULT_DOS_WINDOW_MS,
            violations_before_ban: DEFAULT_VIOLATIONS_BEFORE_BAN,
        }
    }
}

/// Logical payload kind inside an N6 [`IngressEnvelope`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngressKind {
    /// User transaction broadcast.
    Transaction {
        /// Raw transaction bytes.
        payload: Vec<u8>,
    },
    /// Consensus block proposal for `(height, round)`.
    Proposal {
        /// Block height.
        height: u64,
        /// Consensus round.
        round: u64,
        /// Proposed block hash.
        block_hash: [u8; 32],
    },
    /// Consensus vote for `(height, round)`.
    Vote {
        /// Block height.
        height: u64,
        /// Consensus round.
        round: u64,
        /// Voted block hash.
        block_hash: [u8; 32],
    },
}

/// Signed network envelope inspected by [`DosDefenceEngine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngressEnvelope {
    /// Chain identifier.
    pub chain_id: String,
    /// Sender `PeerId = BLAKE3(sender_public_key)`.
    pub sender: PeerId,
    /// 32-byte Ed25519 verifying key.
    pub sender_public_key: [u8; 32],
    /// Per-sender sequence number.
    pub seq: u64,
    /// Logical message kind.
    pub kind: IngressKind,
    /// Total wire frame size in bytes.
    pub raw_size_bytes: usize,
    /// 64-byte Ed25519 signature over the canonical envelope digest.
    pub signature: [u8; 64],
}

impl IngressEnvelope {
    /// Construct and sign an [`IngressEnvelope`] with `identity`.
    pub fn sign(identity: &PeerIdentity, chain_id: &str, seq: u64, kind: IngressKind) -> Self {
        let sender = identity.peer_id();
        let sender_public_key = *identity.public_key();
        let raw_size_bytes = match &kind {
            IngressKind::Transaction { payload } => 128 + payload.len(),
            IngressKind::Proposal { .. } | IngressKind::Vote { .. } => 176,
        };
        let digest = envelope_signing_digest(chain_id, &sender, &sender_public_key, seq, &kind);
        let signature = identity.sign_digest(&digest);
        Self {
            chain_id: chain_id.to_owned(),
            sender,
            sender_public_key,
            seq,
            kind,
            raw_size_bytes,
            signature,
        }
    }
}

fn envelope_signing_digest(
    chain_id: &str,
    sender: &PeerId,
    sender_public_key: &[u8; 32],
    seq: u64,
    kind: &IngressKind,
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(DOS_INGRESS_DOMAIN);
    buf.extend_from_slice(&(chain_id.len() as u32).to_le_bytes());
    buf.extend_from_slice(chain_id.as_bytes());
    buf.extend_from_slice(sender.as_bytes());
    buf.extend_from_slice(sender_public_key);
    buf.extend_from_slice(&seq.to_le_bytes());
    match kind {
        IngressKind::Transaction { payload } => {
            buf.push(0);
            buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            buf.extend_from_slice(blake3_hash(payload).as_bytes());
        }
        IngressKind::Proposal {
            height,
            round,
            block_hash,
        } => {
            buf.push(1);
            buf.extend_from_slice(&height.to_le_bytes());
            buf.extend_from_slice(&round.to_le_bytes());
            buf.extend_from_slice(block_hash);
        }
        IngressKind::Vote {
            height,
            round,
            block_hash,
        } => {
            buf.push(2);
            buf.extend_from_slice(&height.to_le_bytes());
            buf.extend_from_slice(&round.to_le_bytes());
            buf.extend_from_slice(block_hash);
        }
    }
    *blake3_hash(&buf).as_bytes()
}

/// Cryptographic proof of equivocation (double-proposal or double-vote at the
/// same `(height, round)` by the same validator/peer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivocationEvidence {
    /// Offending peer / validator `PeerId`.
    pub peer_id: PeerId,
    /// True if equivocation occurred on a block proposal; false if on a vote.
    pub is_proposal: bool,
    /// Block height of the conflicting statements.
    pub height: u64,
    /// Consensus round of the conflicting statements.
    pub round: u64,
    /// First signed block hash.
    pub first_block_hash: [u8; 32],
    /// Second conflicting signed block hash.
    pub second_block_hash: [u8; 32],
    /// Ed25519 signature over the first statement.
    pub first_signature: [u8; 64],
    /// Ed25519 signature over the second statement.
    pub second_signature: [u8; 64],
}

#[derive(Debug, Clone, Default)]
struct PeerDosWindow {
    window_start_ms: u64,
    msg_count: usize,
    sig_count: usize,
    strikes: usize,
}

/// Key for equivocation tracking: `(peer_id, is_proposal, height, round)`.
type EquivocationKey = (PeerId, bool, u64, u64);

/// N6 DoS defence engine enforcing pre-crypto size & rate limits, per-peer
/// signature verification budgets, and equivocation detection (`D-N3`).
#[derive(Debug, Clone)]
pub struct DosDefenceEngine {
    chain_id: String,
    limits: DosDefenceLimits,
    ban_list: BanList,
    peer_windows: BTreeMap<PeerId, PeerDosWindow>,
    consensus_records: BTreeMap<EquivocationKey, ([u8; 32], [u8; 64])>,
    equivocation_evidence: Vec<EquivocationEvidence>,
    sig_verifications_performed: usize,
    dropped_pre_crypto: usize,
    accepted_messages: usize,
}

impl DosDefenceEngine {
    /// Construct a new [`DosDefenceEngine`] for `chain_id`.
    pub fn new(chain_id: &str, limits: DosDefenceLimits) -> Self {
        Self {
            chain_id: chain_id.to_owned(),
            limits,
            ban_list: BanList::new(),
            peer_windows: BTreeMap::new(),
            consensus_records: BTreeMap::new(),
            equivocation_evidence: Vec::new(),
            sig_verifications_performed: 0,
            dropped_pre_crypto: 0,
            accepted_messages: 0,
        }
    }

    /// Borrow the local [`BanList`].
    pub const fn ban_list(&self) -> &BanList {
        &self.ban_list
    }

    /// Borrow all recorded [`EquivocationEvidence`] items.
    pub fn equivocation_evidence(&self) -> &[EquivocationEvidence] {
        &self.equivocation_evidence
    }

    /// Total Ed25519 signature verifications executed so far.
    pub const fn sig_verifications_performed(&self) -> usize {
        self.sig_verifications_performed
    }

    /// Total envelopes dropped at the O(1) pre-crypto gate (ban, size, msg rate, sig budget).
    pub const fn dropped_pre_crypto(&self) -> usize {
        self.dropped_pre_crypto
    }

    /// Total valid envelopes accepted after all checks.
    pub const fn accepted_messages(&self) -> usize {
        self.accepted_messages
    }

    /// Inspect and validate an incoming [`IngressEnvelope`] at `now_ms`.
    pub fn ingest(&mut self, envelope: IngressEnvelope, now_ms: u64) -> Result<(), P2pError> {
        let peer_id = envelope.sender;

        // 1. O(1) BanList check BEFORE any signature verification.
        if let Err(err) = self.ban_list.check_peer(&peer_id, now_ms) {
            self.dropped_pre_crypto += 1;
            return Err(err);
        }

        // 2. O(1) Frame size check BEFORE any signature verification.
        if envelope.raw_size_bytes == 0 || envelope.raw_size_bytes > self.limits.max_frame_bytes {
            self.dropped_pre_crypto += 1;
            self.record_strike(peer_id, "dos_frame_size_exceeded");
            return Err(P2pError::DosFrameSizeExceeded {
                size: envelope.raw_size_bytes,
                max: self.limits.max_frame_bytes,
            });
        }

        // Update sliding window for `peer_id`.
        let window_ms = self.limits.window_ms.max(1);
        let max_msgs = self.limits.max_messages_per_window;
        let max_sigs = self.limits.max_sig_verifications_per_window;
        let state = self.peer_windows.entry(peer_id).or_default();
        if now_ms.saturating_sub(state.window_start_ms) >= window_ms {
            state.window_start_ms = now_ms;
            state.msg_count = 0;
            state.sig_count = 0;
        }
        state.msg_count += 1;
        let current_msgs = state.msg_count;

        // 3. O(1) Per-peer message rate limit BEFORE any signature verification.
        if current_msgs > max_msgs {
            self.dropped_pre_crypto += 1;
            self.record_strike(peer_id, "dos_message_rate_exceeded");
            return Err(P2pError::DosMessageRateExceeded {
                peer_id,
                count: current_msgs,
                max: max_msgs,
            });
        }

        // 4. O(1) Per-peer signature verification CPU budget BEFORE running Ed25519 verify.
        let next_sigs = self.peer_windows.get(&peer_id).unwrap().sig_count + 1;
        if next_sigs > max_sigs {
            self.dropped_pre_crypto += 1;
            self.record_strike(peer_id, "dos_signature_budget_exceeded");
            return Err(P2pError::DosSignatureBudgetExceeded {
                peer_id,
                count: next_sigs,
                max: max_sigs,
            });
        }
        self.peer_windows.get_mut(&peer_id).unwrap().sig_count = next_sigs;

        // 5. Cryptographic identity & domain-separated Ed25519 signature verification.
        self.sig_verifications_performed += 1;
        if let Err(err) = peer_id.verify_public_key(&envelope.sender_public_key) {
            self.ban_list
                .ban_peer(peer_id, BanReason::IdentityMismatch, None);
            return Err(err);
        }
        if envelope.chain_id != self.chain_id {
            self.ban_list.ban_peer(
                peer_id,
                BanReason::ProtocolViolation("dos_chain_id_mismatch".to_owned()),
                None,
            );
            return Err(P2pError::ChainIdMismatch {
                expected: self.chain_id.clone(),
                actual: envelope.chain_id,
            });
        }
        let digest = envelope_signing_digest(
            &envelope.chain_id,
            &peer_id,
            &envelope.sender_public_key,
            envelope.seq,
            &envelope.kind,
        );
        if verify_raw(&envelope.sender_public_key, &digest, &envelope.signature).is_err() {
            self.ban_list.ban_peer(
                peer_id,
                BanReason::ProtocolViolation("invalid_dos_ingress_signature".to_owned()),
                None,
            );
            return Err(P2pError::InvalidGossipSignature);
        }

        // 6. Equivocation detection for consensus proposals and votes.
        match &envelope.kind {
            IngressKind::Proposal {
                height,
                round,
                block_hash,
            } => {
                self.check_equivocation(
                    peer_id,
                    true,
                    *height,
                    *round,
                    *block_hash,
                    envelope.signature,
                )?;
            }
            IngressKind::Vote {
                height,
                round,
                block_hash,
            } => {
                self.check_equivocation(
                    peer_id,
                    false,
                    *height,
                    *round,
                    *block_hash,
                    envelope.signature,
                )?;
            }
            IngressKind::Transaction { .. } => {}
        }

        self.accepted_messages += 1;
        Ok(())
    }

    fn record_strike(&mut self, peer_id: PeerId, reason: &str) {
        let strikes = {
            let entry = self.peer_windows.entry(peer_id).or_default();
            entry.strikes += 1;
            entry.strikes
        };
        if strikes >= self.limits.violations_before_ban {
            self.ban_list.ban_peer(
                peer_id,
                BanReason::ProtocolViolation(reason.to_owned()),
                None,
            );
        }
    }

    fn check_equivocation(
        &mut self,
        peer_id: PeerId,
        is_proposal: bool,
        height: u64,
        round: u64,
        block_hash: [u8; 32],
        signature: [u8; 64],
    ) -> Result<(), P2pError> {
        let key = (peer_id, is_proposal, height, round);
        if let Some(&(first_hash, first_sig)) = self.consensus_records.get(&key) {
            if first_hash != block_hash {
                self.equivocation_evidence.push(EquivocationEvidence {
                    peer_id,
                    is_proposal,
                    height,
                    round,
                    first_block_hash: first_hash,
                    second_block_hash: block_hash,
                    first_signature: first_sig,
                    second_signature: signature,
                });
                self.ban_list.ban_peer(
                    peer_id,
                    BanReason::ProtocolViolation(format!("equivocation:h={height}:r={round}")),
                    None,
                );
                return Err(P2pError::EquivocationDetected {
                    peer_id,
                    height,
                    round,
                    first_hash,
                    second_hash: block_hash,
                });
            }
            return Ok(());
        }
        self.consensus_records.insert(key, (block_hash, signature));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = "auralite-n6-unit";
    const NOW_MS: u64 = 1_700_000_000_000;

    #[test]
    fn oversized_frame_is_rejected_before_signature_check() {
        let limits = DosDefenceLimits {
            max_frame_bytes: 1_024,
            ..DosDefenceLimits::default()
        };
        let mut engine = DosDefenceEngine::new(CHAIN, limits);
        let peer = PeerIdentity::from_seed([1u8; 32]);
        let mut env = IngressEnvelope::sign(
            &peer,
            CHAIN,
            1,
            IngressKind::Transaction {
                payload: vec![0xAA; 2_048],
            },
        );
        env.raw_size_bytes = 2_200;

        assert_eq!(
            engine.ingest(env, NOW_MS),
            Err(P2pError::DosFrameSizeExceeded {
                size: 2_200,
                max: 1_024,
            })
        );
        assert_eq!(
            engine.sig_verifications_performed(),
            0,
            "oversized frame must be dropped before running Ed25519 signature verification"
        );
    }

    #[test]
    fn message_rate_limit_throttles_and_bans_flooding_peer() {
        let limits = DosDefenceLimits {
            max_messages_per_window: 5,
            max_sig_verifications_per_window: 100,
            violations_before_ban: 2,
            ..DosDefenceLimits::default()
        };
        let mut engine = DosDefenceEngine::new(CHAIN, limits);
        let flooder = PeerIdentity::from_seed([2u8; 32]);

        for seq in 1..=5 {
            let env = IngressEnvelope::sign(
                &flooder,
                CHAIN,
                seq,
                IngressKind::Transaction {
                    payload: vec![seq as u8; 16],
                },
            );
            engine.ingest(env, NOW_MS).unwrap();
        }

        // Messages 6 and 7 exceed max_messages_per_window (2 strikes -> banned).
        for seq in 6..=7 {
            let env = IngressEnvelope::sign(
                &flooder,
                CHAIN,
                seq,
                IngressKind::Transaction {
                    payload: vec![seq as u8; 16],
                },
            );
            assert!(matches!(
                engine.ingest(env, NOW_MS),
                Err(P2pError::DosMessageRateExceeded { .. })
            ));
        }
        assert!(
            engine
                .ban_list()
                .check_peer(&flooder.peer_id(), NOW_MS)
                .is_err()
        );
    }

    #[test]
    fn signature_rate_limit_prevents_signature_storm() {
        let limits = DosDefenceLimits {
            max_messages_per_window: 100,
            max_sig_verifications_per_window: 3,
            violations_before_ban: 2,
            ..DosDefenceLimits::default()
        };
        let mut engine = DosDefenceEngine::new(CHAIN, limits);
        let storm_peer = PeerIdentity::from_seed([3u8; 32]);

        for seq in 1..=3 {
            let env = IngressEnvelope::sign(
                &storm_peer,
                CHAIN,
                seq,
                IngressKind::Transaction {
                    payload: vec![seq as u8; 8],
                },
            );
            engine.ingest(env, NOW_MS).unwrap();
        }
        assert_eq!(engine.sig_verifications_performed(), 3);

        // 4th message within the same window hits the signature verification budget BEFORE verify_raw.
        let env4 = IngressEnvelope::sign(
            &storm_peer,
            CHAIN,
            4,
            IngressKind::Transaction {
                payload: vec![4u8; 8],
            },
        );
        assert_eq!(
            engine.ingest(env4, NOW_MS),
            Err(P2pError::DosSignatureBudgetExceeded {
                peer_id: storm_peer.peer_id(),
                count: 4,
                max: 3,
            })
        );
        assert_eq!(
            engine.sig_verifications_performed(),
            3,
            "signature budget cap must prevent extra Ed25519 verifications"
        );
    }

    #[test]
    fn forged_ingress_signature_is_rejected_and_banned() {
        let mut engine = DosDefenceEngine::new(CHAIN, DosDefenceLimits::default());
        let forger = PeerIdentity::from_seed([4u8; 32]);
        let mut env = IngressEnvelope::sign(
            &forger,
            CHAIN,
            1,
            IngressKind::Transaction {
                payload: b"forged".to_vec(),
            },
        );
        env.signature[0] ^= 0xFF;

        assert_eq!(
            engine.ingest(env, NOW_MS),
            Err(P2pError::InvalidGossipSignature)
        );
        assert!(
            engine
                .ban_list()
                .check_peer(&forger.peer_id(), NOW_MS)
                .is_err()
        );
    }

    #[test]
    fn equivocation_detector_catches_conflicting_vote_and_bans_peer() {
        let mut engine = DosDefenceEngine::new(CHAIN, DosDefenceLimits::default());
        let byzantine = PeerIdentity::from_seed([5u8; 32]);

        let vote_a = IngressEnvelope::sign(
            &byzantine,
            CHAIN,
            1,
            IngressKind::Vote {
                height: 42,
                round: 0,
                block_hash: [0xAA; 32],
            },
        );
        let vote_b = IngressEnvelope::sign(
            &byzantine,
            CHAIN,
            2,
            IngressKind::Vote {
                height: 42,
                round: 0,
                block_hash: [0xBB; 32],
            },
        );

        engine.ingest(vote_a, NOW_MS).unwrap();
        assert_eq!(
            engine.ingest(vote_b, NOW_MS),
            Err(P2pError::EquivocationDetected {
                peer_id: byzantine.peer_id(),
                height: 42,
                round: 0,
                first_hash: [0xAA; 32],
                second_hash: [0xBB; 32],
            })
        );
        assert_eq!(engine.equivocation_evidence().len(), 1);
        assert!(
            engine
                .ban_list()
                .check_peer(&byzantine.peer_id(), NOW_MS)
                .is_err()
        );
    }

    #[test]
    fn ten_thousand_msg_per_sec_attacker_is_isolated_while_validators_finalize() {
        let limits = DosDefenceLimits {
            max_frame_bytes: 64 * 1024,
            max_messages_per_window: 50,
            max_sig_verifications_per_window: 50,
            window_ms: 1_000,
            violations_before_ban: 3,
        };
        let mut engine = DosDefenceEngine::new(CHAIN, limits);
        let attacker = PeerIdentity::from_seed([0x99; 32]);
        let validators: Vec<PeerIdentity> = (1..=4u8)
            .map(|i| PeerIdentity::from_seed([i; 32]))
            .collect();

        // Pre-sign 1 attacker flood template and send 10,000 messages across two windows
        // (5,000 at NOW_MS and 5,000 at NOW_MS + 1_000 after the attacker is banned).
        let flood_env = IngressEnvelope::sign(
            &attacker,
            CHAIN,
            1,
            IngressKind::Transaction {
                payload: vec![0x77; 64],
            },
        );
        for i in 0..10_000 {
            let ts = if i < 5_000 { NOW_MS } else { NOW_MS + 1_000 };
            let _ = engine.ingest(flood_env.clone(), ts);
        }

        // Attacker must be banned after 50 allowed + 3 rate strikes in the first window,
        // so the second window's 5,000 messages are rejected immediately by BanList (0 extra sigs).
        assert!(
            engine
                .ban_list()
                .check_peer(&attacker.peer_id(), NOW_MS + 1_000)
                .is_err()
        );
        assert_eq!(engine.sig_verifications_performed(), 50);
        assert_eq!(engine.dropped_pre_crypto(), 9_950);

        // Honest validators finalize blocks unhindered during the exact same window.
        let mut finalized_heights = 0u64;
        for height in 1..=5u64 {
            let block_hash = [height as u8; 32];
            let prop = IngressEnvelope::sign(
                &validators[0],
                CHAIN,
                height,
                IngressKind::Proposal {
                    height,
                    round: 0,
                    block_hash,
                },
            );
            engine.ingest(prop, NOW_MS).unwrap();
            for val in &validators[..3] {
                let vote = IngressEnvelope::sign(
                    val,
                    CHAIN,
                    height,
                    IngressKind::Vote {
                        height,
                        round: 0,
                        block_hash,
                    },
                );
                engine.ingest(vote, NOW_MS).unwrap();
            }
            finalized_heights += 1;
        }
        assert_eq!(finalized_heights, 5);
    }
}
