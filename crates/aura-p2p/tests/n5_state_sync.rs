//! N5 state sync gate: a new node syncs an S4 state snapshot within measured
//! time, verifies the manifest, chunk digests, and S1 `state_root`, and
//! automatically rolls back, bans the offending peer, and re-requests when
//! served a corrupt chunk.

use aura_p2p::block_sync::{SyncQc, SyncValidatorSet, SyncedHeader};
use aura_p2p::identity::PeerIdentity;
use aura_p2p::state_sync::{StateSyncSession, build_s4_snapshot_bundle};
use aura_state::{Account, StateEntries, StateKey, state_root};
use aura_types::{Address, Amount, Nonce};

const CHAIN: &str = "auralite-devnet-1";
const SNAPSHOT_HEIGHT: u64 = 5_000;
const NOW_MS: u64 = 1_700_005_000_000;

fn generate_state_entries(count: usize) -> StateEntries {
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let mut addr = [0u8; 20];
        addr[0] = 0x42;
        addr[12..20].copy_from_slice(&(i as u64 + 1).to_be_bytes());
        let key = StateKey::account(&Address::from_bytes(addr));
        let account = Account::new(
            Nonce::new(i as u64 % 50),
            Amount::from_units((i as u128 + 1) * 10_000),
            Account::NO_CODE,
            Account::EMPTY_STORAGE_ROOT,
        );
        entries.push((key.as_bytes().to_vec(), account.to_bytes()));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn build_certified_header(state_root_bytes: [u8; 32]) -> (SyncValidatorSet, SyncedHeader, SyncQc) {
    let validators: Vec<PeerIdentity> = (1..=4u8)
        .map(|i| PeerIdentity::from_seed([i; 32]))
        .collect();
    let val_entries = validators
        .iter()
        .map(|id| (id.peer_id(), *id.public_key(), 100u64))
        .collect();
    let val_set = SyncValidatorSet::from_validators(val_entries).unwrap();
    let header = SyncedHeader::new(
        CHAIN,
        SNAPSHOT_HEIGHT,
        [0x55; 32],
        [0x66; 32],
        state_root_bytes,
        NOW_MS,
        validators[0].peer_id(),
    );
    let signers = [&validators[0], &validators[1], &validators[2]];
    let qc = SyncQc::sign(CHAIN, SNAPSHOT_HEIGHT, 1, header.block_hash, &signers);
    (val_set, header, qc)
}

#[test]
fn new_node_syncs_s4_state_within_measured_time_and_recovers_from_corrupt_chunks() {
    // Build a 1,000-account S4 state snapshot split into 10 chunks of 100 keys each.
    let entries = generate_state_entries(1_000);
    let (raw_manifest, honest_chunks, expected_root) =
        build_s4_snapshot_bundle(CHAIN, SNAPSHOT_HEIGHT, &entries, 100, 4).unwrap();
    assert_eq!(honest_chunks.len(), 10);

    let (val_set, header, qc) = build_certified_header(expected_root);
    let mut new_node = StateSyncSession::from_certified_header(&header, &qc, &val_set)
        .expect("certified header initializes state sync session");

    let honest_peer_a = PeerIdentity::from_seed([0xA1; 32]).peer_id();
    let corrupt_peer = PeerIdentity::from_seed([0xB2; 32]).peer_id();
    let honest_peer_b = PeerIdentity::from_seed([0xC3; 32]).peer_id();

    let manifest = new_node
        .accept_manifest(honest_peer_a, &raw_manifest, NOW_MS)
        .expect("S4 manifest verifies against QC-certified header");
    assert_eq!(manifest.chunks.len(), 10);
    assert_eq!(manifest.total_keys, 1_000);

    // Peer `corrupt_peer` corrupts chunk #1 (first chunk assigned to it in round-robin),
    // triggering rollback, peer ban, and automatic re-request from `honest_peer_b`.
    let mut corrupt_chunks = honest_chunks.clone();
    corrupt_chunks[1][0] ^= 0xAA;

    let report = new_node
        .sync_from_peers(
            &[
                (honest_peer_a, honest_chunks.clone()),
                (corrupt_peer, corrupt_chunks),
                (honest_peer_b, honest_chunks),
            ],
            4,
            NOW_MS,
        )
        .expect("state sync completes after rollback and re-request");

    assert_eq!(report.height, SNAPSHOT_HEIGHT);
    assert_eq!(report.total_keys, 1_000);
    assert_eq!(report.chunks_verified, 10);
    assert_eq!(report.rollbacks_triggered, 1);
    assert_eq!(report.re_requested_chunks, vec![1]);
    assert_eq!(report.banned_peers, vec![corrupt_peer]);
    assert_eq!(report.verified_state_root, expected_root);
    assert!(
        report.elapsed_micros < 5_000_000,
        "1,000-key S4 state sync took {} us (>= 5s)",
        report.elapsed_micros
    );

    // Verify the committed state entries match the original state and recompute the exact S1 root.
    let committed = new_node.committed_entries().expect("entries committed");
    assert_eq!(committed, &entries);
    assert_eq!(
        *state_root(committed).unwrap().as_bytes(),
        expected_root,
        "recomputed S1 Merkle root matches certified header state_root"
    );
}
