//! N7 light client gate: a light client verifies header+QC proofs, finalized
//! checkpoint progression, and simplified Data-Availability Sampling (DAS)
//! without a full node, rejects forged headers and corrupted DAS samples (and
//! bans the offending peer), and enforces strict proof byte-size and
//! verification-gas bounds.

use aura_p2p::block_sync::{SyncQc, SyncValidatorSet, SyncedHeader};
use aura_p2p::error::P2pError;
use aura_p2p::identity::PeerIdentity;
use aura_p2p::light_client::{
    DEFAULT_MAX_PROOF_BYTES, DEFAULT_MAX_VERIFICATION_GAS, FinalizedCheckpoint, LightClientBounds,
    LightClientProof, LightClientVerifier, build_das_tree, sample_das_chunks,
};

const CHAIN: &str = "auralite-devnet-1";
const NOW_MS: u64 = 1_700_007_000_000;

#[test]
fn light_client_rejects_forged_headers_and_verifies_checkpoints_with_bounded_proof_gas_and_size() {
    let validators: Vec<PeerIdentity> = (1..=4u8)
        .map(|seed| PeerIdentity::from_seed([seed; 32]))
        .collect();
    let val_entries = validators
        .iter()
        .map(|id| (id.peer_id(), *id.public_key(), 100u64))
        .collect();
    let val_set = SyncValidatorSet::from_validators(val_entries).unwrap();

    let genesis_header = SyncedHeader::new(
        CHAIN,
        1_000,
        [0x10; 32],
        [0x20; 32],
        [0x30; 32],
        NOW_MS,
        validators[0].peer_id(),
    );
    let initial_checkpoint = FinalizedCheckpoint::from_header(&genesis_header);
    let bounds = LightClientBounds::default();
    let mut light_client = LightClientVerifier::new(initial_checkpoint, val_set, bounds);

    let light_peer = PeerIdentity::from_seed([0x77; 32]).peer_id();
    let honest_relayer = PeerIdentity::from_seed([0xA1; 32]).peer_id();
    let forged_header_peer = PeerIdentity::from_seed([0xB2; 32]).peer_id();
    let forged_qc_peer = PeerIdentity::from_seed([0xC3; 32]).peer_id();
    let withheld_das_peer = PeerIdentity::from_seed([0xD4; 32]).peer_id();

    // Build a 16-chunk DAS tree (256 bytes per chunk) for height 1,001.
    let chunks_1001: Vec<Vec<u8>> = (0..16u8).map(|i| vec![0x40 + i; 256]).collect();
    let (tx_root_1001, all_samples_1001) = build_das_tree(&chunks_1001).unwrap();
    let header_1001 = SyncedHeader::new(
        CHAIN,
        1_001,
        genesis_header.block_hash,
        tx_root_1001,
        [0xAA; 32],
        NOW_MS + 1_000,
        validators[1].peer_id(),
    );
    let signers = [&validators[0], &validators[1], &validators[2]];
    let qc_1001 = SyncQc::sign(CHAIN, 1_001, 1, header_1001.block_hash, &signers);
    let samples_1001 =
        sample_das_chunks(&all_samples_1001, &header_1001.block_hash, &light_peer, 4);

    let valid_proof_1001 = LightClientProof {
        checkpoint_height: 1_000,
        checkpoint_hash: genesis_header.block_hash,
        header: header_1001.clone(),
        qc: qc_1001,
        das_samples: samples_1001,
    };

    // 1. Forged header (attacker tampers with `state_root` to steal funds) -> rejected & peer banned.
    let mut forged_header_proof = valid_proof_1001.clone();
    forged_header_proof.header.state_root = [0xFF; 32];
    assert_eq!(
        light_client.verify_and_advance(forged_header_peer, &forged_header_proof, NOW_MS + 1_000),
        Err(P2pError::BlockSyncHeaderHashMismatch { height: 1_001 }),
        "light client must reject a forged header whose state_root was altered"
    );
    assert!(
        light_client
            .ban_list()
            .check_peer(&forged_header_peer, NOW_MS + 1_000)
            .is_err(),
        "peer serving a forged header must be banned"
    );
    assert_eq!(
        light_client.checkpoint().height,
        1_000,
        "checkpoint must not advance on a forged header"
    );

    // 2. Forged header where attacker recomputes `block_hash` over fake `state_root`
    //    but cannot forge the 2f+1 validator QC -> rejected & peer banned.
    let recomputed_fake_header = SyncedHeader::new(
        CHAIN,
        1_001,
        genesis_header.block_hash,
        tx_root_1001,
        [0xEE; 32],
        NOW_MS + 1_000,
        validators[1].peer_id(),
    );
    let mut forged_qc_proof = valid_proof_1001.clone();
    forged_qc_proof.header = recomputed_fake_header;
    assert!(
        matches!(
            light_client.verify_and_advance(forged_qc_peer, &forged_qc_proof, NOW_MS + 1_000),
            Err(P2pError::BlockSyncHeaderHashMismatch { height: 1_001 })
        ),
        "light client must reject a header whose hash does not match the 2f+1 QC"
    );
    assert!(
        light_client
            .ban_list()
            .check_peer(&forged_qc_peer, NOW_MS + 1_000)
            .is_err()
    );

    // 3. Data-availability withholding / corrupted DAS sample -> rejected & peer banned.
    let mut bad_das_proof = valid_proof_1001.clone();
    bad_das_proof.das_samples[2].chunk_data[0] ^= 0x80;
    assert!(
        matches!(
            light_client.verify_and_advance(withheld_das_peer, &bad_das_proof, NOW_MS + 1_000),
            Err(P2pError::LightClientDasSampleInvalid { .. })
        ),
        "light client must reject a proof with a corrupted DAS sample"
    );
    assert!(
        light_client
            .ban_list()
            .check_peer(&withheld_das_peer, NOW_MS + 1_000)
            .is_err()
    );

    // 4. Honest relayer submits the valid proof for height 1,001 -> accepted, checkpoint advances,
    //    and both proof byte size and verification gas are strictly bounded.
    let receipt_1001 = light_client
        .verify_and_advance(honest_relayer, &valid_proof_1001, NOW_MS + 1_000)
        .expect("valid header+QC+DAS proof advances the light client checkpoint");
    assert_eq!(receipt_1001.height, 1_001);
    assert_eq!(receipt_1001.block_hash, header_1001.block_hash);
    assert_eq!(receipt_1001.state_root, [0xAA; 32]);
    assert_eq!(receipt_1001.das_samples_verified, 4);
    assert!(
        receipt_1001.proof_bytes > 0 && receipt_1001.proof_bytes <= DEFAULT_MAX_PROOF_BYTES,
        "proof size {} bytes must be compact and within max bound {}",
        receipt_1001.proof_bytes,
        DEFAULT_MAX_PROOF_BYTES
    );
    assert!(
        receipt_1001.verification_gas > 0
            && receipt_1001.verification_gas <= DEFAULT_MAX_VERIFICATION_GAS,
        "verification gas {} must be bounded within {}",
        receipt_1001.verification_gas,
        DEFAULT_MAX_VERIFICATION_GAS
    );
    assert_eq!(light_client.checkpoint().height, 1_001);
    assert_eq!(light_client.checkpoint().state_root, [0xAA; 32]);

    // 5. Light client can also verify a multi-block finalized checkpoint jump (e.g., 1,001 -> 1,500)
    //    without downloading intermediate blocks.
    let chunks_1500: Vec<Vec<u8>> = (0..16u8).map(|i| vec![0x80 + i; 256]).collect();
    let (tx_root_1500, all_samples_1500) = build_das_tree(&chunks_1500).unwrap();
    let header_1500 = SyncedHeader::new(
        CHAIN,
        1_500,
        [0x77; 32],
        tx_root_1500,
        [0xBB; 32],
        NOW_MS + 500_000,
        validators[2].peer_id(),
    );
    let qc_1500 = SyncQc::sign(CHAIN, 1_500, 1, header_1500.block_hash, &signers);
    let samples_1500 =
        sample_das_chunks(&all_samples_1500, &header_1500.block_hash, &light_peer, 4);
    let proof_1500 = LightClientProof {
        checkpoint_height: 1_001,
        checkpoint_hash: header_1001.block_hash,
        header: header_1500.clone(),
        qc: qc_1500,
        das_samples: samples_1500,
    };

    let receipt_1500 = light_client
        .verify_and_advance(honest_relayer, &proof_1500, NOW_MS + 500_000)
        .expect("multi-block checkpoint proof verifies in O(1) header+QC+DAS cost");
    assert_eq!(receipt_1500.height, 1_500);
    assert_eq!(receipt_1500.state_root, [0xBB; 32]);
    assert_eq!(receipt_1500.proof_bytes, receipt_1001.proof_bytes);
    assert_eq!(receipt_1500.verification_gas, receipt_1001.verification_gas);
}
