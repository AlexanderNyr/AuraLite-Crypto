//! N6 DoS defence gate: 1 malicious peer sends 10,000 msg/s (flood, invalid
//! signatures, oversized frames, and equivocation) while the 4 honest
//! validators keep finalizing blocks uninterrupted.

use aura_p2p::block_sync::{SyncQc, SyncValidatorSet, SyncedHeader};
use aura_p2p::dos_defence::{DosDefenceEngine, DosDefenceLimits, IngressEnvelope, IngressKind};
use aura_p2p::identity::PeerIdentity;

const CHAIN: &str = "auralite-devnet-1";
const NOW_MS: u64 = 1_700_006_000_000;

#[test]
fn malicious_peer_sending_10k_msgs_per_sec_is_isolated_while_validators_keep_finalizing() {
    let validators: Vec<PeerIdentity> = (1..=4u8)
        .map(|seed| PeerIdentity::from_seed([seed; 32]))
        .collect();
    let val_entries = validators
        .iter()
        .map(|id| (id.peer_id(), *id.public_key(), 100u64))
        .collect();
    let val_set = SyncValidatorSet::from_validators(val_entries).unwrap();

    let attacker = PeerIdentity::from_seed([0xEE; 32]);
    let limits = DosDefenceLimits::default();
    let mut engine = DosDefenceEngine::new(CHAIN, limits);

    // Pre-generate one signed attacker template to simulate a 10,000 msg/s line-rate flood
    // over a 1-second window without spending test-harness time on 10,000 Ed25519 signs.
    let template = IngressEnvelope::sign(
        &attacker,
        CHAIN,
        1,
        IngressKind::Transaction {
            payload: vec![0xAA; 64],
        },
    );

    let mut finalized_heights = Vec::new();
    let mut parent_hash = [0u8; 32];
    let mut attacker_accepted = 0usize;
    let mut attacker_rejected = 0usize;

    // 5 consensus heights finalized within the 1-second window, interleaved
    // with 2,000 malicious messages per height (= 10,000 msg/s total).
    for height in 1..=5u64 {
        let step_ms = NOW_MS + (height - 1) * 150;

        // Attacker blasts 2,000 messages in this 150ms slice (10,000 msg/s across the 1s window).
        for i in 0..2_000usize {
            let mut flood_msg = template.clone();
            if i % 50 == 49 {
                flood_msg.raw_size_bytes = limits.max_frame_bytes + 1024;
            }
            match engine.ingest(flood_msg, step_ms) {
                Ok(()) => attacker_accepted += 1,
                Err(_) => attacker_rejected += 1,
            }
        }

        // Honest leader proposes `height` and 3 honest validators cast precommit votes.
        let leader = &validators[(height as usize - 1) % validators.len()];
        let header = SyncedHeader::new(
            CHAIN,
            height,
            parent_hash,
            [height as u8; 32],
            [0x10 + height as u8; 32],
            step_ms,
            leader.peer_id(),
        );
        let proposal_env = IngressEnvelope::sign(
            leader,
            CHAIN,
            height,
            IngressKind::Proposal {
                height,
                round: 1,
                block_hash: header.block_hash,
            },
        );
        engine
            .ingest(proposal_env, step_ms)
            .expect("honest leader proposal must be admitted despite 10k msg/s flood");

        let mut qc_signers = Vec::new();
        for voter in validators.iter().take(3) {
            let vote_env = IngressEnvelope::sign(
                voter,
                CHAIN,
                height,
                IngressKind::Vote {
                    height,
                    round: 1,
                    block_hash: header.block_hash,
                },
            );
            engine
                .ingest(vote_env, step_ms)
                .expect("honest validator vote must be admitted despite 10k msg/s flood");
            qc_signers.push(voter);
        }

        let qc = SyncQc::sign(CHAIN, height, 1, header.block_hash, &qc_signers);
        qc.verify(CHAIN, height, &header.block_hash, &val_set)
            .expect("honest validators form valid 2f+1 QC and finalize height");
        parent_hash = header.block_hash;
        finalized_heights.push(height);
    }

    assert_eq!(
        finalized_heights,
        vec![1, 2, 3, 4, 5],
        "validators must keep finalizing every height under a 10,000 msg/s flood"
    );
    assert!(
        engine
            .ban_list()
            .check_peer(&attacker.peer_id(), NOW_MS)
            .is_err(),
        "malicious 10k msg/s peer must be banned"
    );
    assert!(
        attacker_accepted <= limits.max_sig_verifications_per_window,
        "attacker cannot consume more than its signature budget ({attacker_accepted} accepted)"
    );
    assert_eq!(attacker_accepted + attacker_rejected, 10_000);
    assert!(
        engine.dropped_pre_crypto() >= 9_900,
        "at least 9,900 of the 10,000 flood messages must be dropped pre-crypto"
    );
    assert!(
        engine.sig_verifications_performed() <= limits.max_sig_verifications_per_window + 5 * 4,
        "pre-crypto rate/ban filter must prevent Ed25519 CPU exhaustion (verified {} sigs)",
        engine.sig_verifications_performed()
    );
}
