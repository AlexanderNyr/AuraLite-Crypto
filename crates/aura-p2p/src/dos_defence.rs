//! N6 DoS defence: pre-crypto frame size & message rate limits, per-authenticated-source
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

/// Default maximum messages per authenticated transport source per 1-second window.
pub const DEFAULT_MAX_MSGS_PER_SEC: usize = 200;

/// Default maximum Ed25519 signature verifications per authenticated source per 1-second window.
pub const DEFAULT_MAX_SIGS_PER_SEC: usize = 100;

/// Default number of rate/size strikes before automatic `BanList` isolation.
pub const DEFAULT_VIOLATIONS_BEFORE_BAN: usize = 3;

/// Per-authenticated-source and per-frame DoS defence limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DosDefenceLimits {
    /// Maximum allowed wire frame size in bytes (checked before signature verification).
    pub max_frame_bytes: usize,
    /// Maximum messages accepted for inspection per authenticated source per window.
    pub max_messages_per_window: usize,
    /// Maximum signature-verification attempts per authenticated source per window.
    pub max_sig_verifications_per_window: usize,
    /// Sliding window duration in milliseconds.
    pub window_ms: u64,
    /// Number of rate/size violations before the authenticated source is banned.
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
    /// Claimed signer `PeerId = BLAKE3(sender_public_key)`; untrusted until verified.
    /// This is the message origin, not the authenticated transport source.
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

/// N6 DoS defence engine enforcing pre-crypto size & rate limits, per-source
/// signature-verification-attempt budgets, and equivocation detection (`D-N3`).
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
    ///
    /// `authenticated_peer` must come from a completed, authenticated transport
    /// session; never populate it from `envelope.sender` or other wire data. All
    /// pre-crypto budgets and source-attributed strikes use this ID. The envelope
    /// signer is checked separately after its public key and signature validate.
    pub fn ingest(
        &mut self,
        authenticated_peer: PeerId,
        envelope: IngressEnvelope,
        now_ms: u64,
    ) -> Result<(), P2pError> {
        let signer_peer_id = envelope.sender;

        // 1. This ID is supplied by the authenticated transport, not the frame.
        if let Err(err) = self.ban_list.check_peer(&authenticated_peer, now_ms) {
            self.dropped_pre_crypto += 1;
            return Err(err);
        }

        // 2. O(1) frame-size check before any signature verification.
        if envelope.raw_size_bytes == 0 || envelope.raw_size_bytes > self.limits.max_frame_bytes {
            self.dropped_pre_crypto += 1;
            self.record_strike(authenticated_peer, "dos_frame_size_exceeded");
            return Err(P2pError::DosFrameSizeExceeded {
                size: envelope.raw_size_bytes,
                max: self.limits.max_frame_bytes,
            });
        }

        // Resource accounting is keyed by the authenticated transport source,
        // never by the message's unauthenticated claimed signer.
        let window_ms = self.limits.window_ms.max(1);
        let max_msgs = self.limits.max_messages_per_window;
        let max_sigs = self.limits.max_sig_verifications_per_window;
        let state = self.peer_windows.entry(authenticated_peer).or_default();
        if now_ms.saturating_sub(state.window_start_ms) >= window_ms {
            state.window_start_ms = now_ms;
            state.msg_count = 0;
            state.sig_count = 0;
        }
        state.msg_count += 1;
        let current_msgs = state.msg_count;

        // 3. O(1) per-source message rate limit before crypto.
        if current_msgs > max_msgs {
            self.dropped_pre_crypto += 1;
            self.record_strike(authenticated_peer, "dos_message_rate_exceeded");
            return Err(P2pError::DosMessageRateExceeded {
                peer_id: authenticated_peer,
                count: current_msgs,
                max: max_msgs,
            });
        }

        // 4. O(1) per-source signature verification CPU budget before Ed25519.
        let next_sigs = self
            .peer_windows
            .get(&authenticated_peer)
            .unwrap()
            .sig_count
            + 1;
        if next_sigs > max_sigs {
            self.dropped_pre_crypto += 1;
            self.record_strike(authenticated_peer, "dos_signature_budget_exceeded");
            return Err(P2pError::DosSignatureBudgetExceeded {
                peer_id: authenticated_peer,
                count: next_sigs,
                max: max_sigs,
            });
        }
        self.peer_windows
            .get_mut(&authenticated_peer)
            .unwrap()
            .sig_count = next_sigs;

        // A bad claimed origin is not evidence against that claimed PeerId.
        // Attribute repeated malformed input only to the authenticated source.
        if let Err(err) = signer_peer_id.verify_public_key(&envelope.sender_public_key) {
            self.record_strike(authenticated_peer, "dos_identity_mismatch");
            return Err(err);
        }

        // 5. Verify the domain-separated signature before applying identity-based
        // bans for signed protocol violations.
        self.sig_verifications_performed += 1;
        let digest = envelope_signing_digest(
            &envelope.chain_id,
            &signer_peer_id,
            &envelope.sender_public_key,
            envelope.seq,
            &envelope.kind,
        );
        if verify_raw(&envelope.sender_public_key, &digest, &envelope.signature).is_err() {
            // The transport identity is authenticated even though the envelope's
            // claimed signer is not. Never ban the unverified claimed signer.
            self.ban_list.ban_peer(
                authenticated_peer,
                BanReason::ProtocolViolation("invalid_dos_ingress_signature".to_owned()),
                None,
            );
            return Err(P2pError::InvalidGossipSignature);
        }

        // The signed envelope now proves control of the key for signer_peer_id.
        self.ban_list.check_peer(&signer_peer_id, now_ms)?;
        if envelope.chain_id != self.chain_id {
            self.ban_list.ban_peer(
                signer_peer_id,
                BanReason::ProtocolViolation("dos_chain_id_mismatch".to_owned()),
                None,
            );
            return Err(P2pError::ChainIdMismatch {
                expected: self.chain_id.clone(),
                actual: envelope.chain_id,
            });
        }

        // 6. Equivocation detection for consensus proposals and votes.
        match &envelope.kind {
            IngressKind::Proposal {
                height,
                round,
                block_hash,
            } => {
                self.check_equivocation(
                    signer_peer_id,
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
                    signer_peer_id,
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

    fn record_strike(&mut self, authenticated_peer: PeerId, reason: &str) {
        let strikes = {
            let entry = self.peer_windows.entry(authenticated_peer).or_default();
            entry.strikes += 1;
            entry.strikes
        };
        if strikes >= self.limits.violations_before_ban {
            self.ban_list.ban_peer(
                authenticated_peer,
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
            engine.ingest(peer.peer_id(), env, NOW_MS),
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
            engine.ingest(flooder.peer_id(), env, NOW_MS).unwrap();
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
                engine.ingest(flooder.peer_id(), env, NOW_MS),
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
            engine.ingest(storm_peer.peer_id(), env, NOW_MS).unwrap();
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
            engine.ingest(storm_peer.peer_id(), env4, NOW_MS),
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
    fn unauthenticated_sender_id_mismatch_cannot_ban_victim() {
        let limits = DosDefenceLimits {
            max_messages_per_window: 1,
            violations_before_ban: 1,
            ..DosDefenceLimits::default()
        };
        let mut engine = DosDefenceEngine::new(CHAIN, limits);
        let victim = PeerIdentity::from_seed([0x61; 32]);
        let attacker = PeerIdentity::from_seed([0x62; 32]);
        let victim_id = victim.peer_id();

        let mut spoofed = IngressEnvelope::sign(
            &attacker,
            CHAIN,
            1,
            IngressKind::Transaction {
                payload: b"spoofed sender".to_vec(),
            },
        );
        // `sender` is an unauthenticated wire claim, not proof of the signer.
        spoofed.sender = victim_id;
        assert!(matches!(
            engine.ingest(attacker.peer_id(), spoofed, NOW_MS),
            Err(P2pError::IdentityMismatch { .. })
        ));
        assert!(
            !engine.ban_list().is_peer_banned(&victim_id, NOW_MS),
            "a mismatching public key must not permanently ban the claimed PeerId"
        );
        assert!(
            engine
                .ban_list()
                .is_peer_banned(&attacker.peer_id(), NOW_MS),
            "source-attributed strikes may ban only the authenticated transport peer"
        );

        let honest = IngressEnvelope::sign(
            &victim,
            CHAIN,
            2,
            IngressKind::Transaction {
                payload: b"victim remains admissible".to_vec(),
            },
        );
        engine
            .ingest(victim_id, honest, NOW_MS)
            .expect("victim's valid signature still passes");
    }

    #[test]
    fn unauthenticated_sender_id_mismatch_cannot_poison_signature_budget() {
        let limits = DosDefenceLimits {
            max_messages_per_window: 10,
            max_sig_verifications_per_window: 1,
            violations_before_ban: 1,
            ..DosDefenceLimits::default()
        };
        let mut engine = DosDefenceEngine::new(CHAIN, limits);
        let victim = PeerIdentity::from_seed([0x67; 32]);
        let attacker = PeerIdentity::from_seed([0x68; 32]);
        let victim_id = victim.peer_id();
        let mut spoofed = IngressEnvelope::sign(
            &attacker,
            CHAIN,
            1,
            IngressKind::Transaction {
                payload: b"poison signature budget".to_vec(),
            },
        );
        spoofed.sender = victim_id;

        assert!(matches!(
            engine.ingest(attacker.peer_id(), spoofed, NOW_MS),
            Err(P2pError::IdentityMismatch { .. })
        ));
        assert!(!engine.ban_list().is_peer_banned(&victim_id, NOW_MS));
        assert!(
            engine
                .ban_list()
                .is_peer_banned(&attacker.peer_id(), NOW_MS)
        );

        let honest = IngressEnvelope::sign(
            &victim,
            CHAIN,
            2,
            IngressKind::Transaction {
                payload: b"victim still has its own signature budget".to_vec(),
            },
        );
        engine
            .ingest(victim_id, honest, NOW_MS)
            .expect("an unauthenticated claim must not consume the victim's signature budget");
    }

    #[test]
    fn invalid_signature_bans_authenticated_source_not_spoofed_signer() {
        let mut engine = DosDefenceEngine::new(CHAIN, DosDefenceLimits::default());
        let victim = PeerIdentity::from_seed([0x63; 32]);
        let attacker = PeerIdentity::from_seed([0x64; 32]);
        let victim_id = victim.peer_id();
        let mut forged = IngressEnvelope::sign(
            &victim,
            CHAIN,
            1,
            IngressKind::Transaction {
                payload: b"invalid signature with victim claim".to_vec(),
            },
        );
        forged.signature[0] ^= 0x80;

        assert_eq!(
            engine.ingest(attacker.peer_id(), forged, NOW_MS),
            Err(P2pError::InvalidGossipSignature)
        );
        assert!(!engine.ban_list().is_peer_banned(&victim_id, NOW_MS));
        assert!(
            engine
                .ban_list()
                .is_peer_banned(&attacker.peer_id(), NOW_MS)
        );
    }

    #[test]
    fn wrong_chain_is_checked_only_after_the_claimed_signer_signature() {
        let mut engine = DosDefenceEngine::new(CHAIN, DosDefenceLimits::default());
        let victim = PeerIdentity::from_seed([0x65; 32]);
        let attacker = PeerIdentity::from_seed([0x66; 32]);
        let victim_id = victim.peer_id();
        let mut forged = IngressEnvelope::sign(
            &attacker,
            "foreign-chain",
            1,
            IngressKind::Transaction {
                payload: b"spoofed foreign-chain sender".to_vec(),
            },
        );
        // The victim public key is public; possession is not proven by the claim.
        forged.sender = victim_id;
        forged.sender_public_key = *victim.public_key();

        assert_eq!(
            engine.ingest(attacker.peer_id(), forged, NOW_MS),
            Err(P2pError::InvalidGossipSignature)
        );
        assert!(!engine.ban_list().is_peer_banned(&victim_id, NOW_MS));
        assert!(
            engine
                .ban_list()
                .is_peer_banned(&attacker.peer_id(), NOW_MS)
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
            engine.ingest(forger.peer_id(), env, NOW_MS),
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

        engine.ingest(byzantine.peer_id(), vote_a, NOW_MS).unwrap();
        assert_eq!(
            engine.ingest(byzantine.peer_id(), vote_b, NOW_MS),
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
            let _ = engine.ingest(attacker.peer_id(), flood_env.clone(), ts);
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
            engine
                .ingest(validators[0].peer_id(), prop, NOW_MS)
                .unwrap();
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
                engine.ingest(val.peer_id(), vote, NOW_MS).unwrap();
            }
            finalized_heights += 1;
        }
        assert_eq!(finalized_heights, 5);
    }
}
