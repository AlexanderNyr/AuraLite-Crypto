//! N4 block sync gate: a lagging node catches up 10,000 blocks via header-first
//! verification (`SyncedHeader` + `SyncQc`) and parallel body download across
//! multiple peers, reaching the exact same head hash and state root.

use aura_crypto::blake3_hash;
use aura_p2p::block_sync::{
    BlockSyncEngine, SyncQc, SyncValidatorSet, SyncedBlockBody, SyncedHeader,
};
use aura_p2p::error::P2pError;
use aura_p2p::identity::PeerIdentity;

const CHAIN: &str = "auralite-devnet-1";
const GENESIS_HASH: [u8; 32] = [0x10; 32];
const GENESIS_STATE: [u8; 32] = [0x20; 32];
const GENESIS_TS_MS: u64 = 1_700_000_000_000;

type CanonicalChain = (
    Vec<PeerIdentity>,
    SyncValidatorSet,
    Vec<(SyncedHeader, SyncQc)>,
    Vec<SyncedBlockBody>,
);

fn build_canonical_chain(block_count: u64) -> CanonicalChain {
    let validators: Vec<PeerIdentity> = (1..=4u8)
        .map(|i| PeerIdentity::from_seed([i; 32]))
        .collect();
    let entries = validators
        .iter()
        .map(|id| (id.peer_id(), *id.public_key(), 100u64))
        .collect();
    let val_set = SyncValidatorSet::from_validators(entries).unwrap();

    // First build the sequential headers and bodies (fast BLAKE3 hashing).
    let mut headers_only = Vec::with_capacity(block_count as usize);
    let mut bodies = Vec::with_capacity(block_count as usize);
    let mut parent_hash = GENESIS_HASH;
    let mut state_root = GENESIS_STATE;

    for height in 1..=block_count {
        let tx = format!("tx-height-{height}").into_bytes();
        let txs = vec![tx];
        let tx_root = SyncedBlockBody::compute_tx_root(&txs);

        let mut state_buf = [0u8; 64];
        state_buf[..32].copy_from_slice(&state_root);
        state_buf[32..].copy_from_slice(&tx_root);
        state_root = *blake3_hash(&state_buf).as_bytes();

        let proposer = validators[(height as usize) % validators.len()].peer_id();
        let header = SyncedHeader::new(
            CHAIN,
            height,
            parent_hash,
            tx_root,
            state_root,
            GENESIS_TS_MS + height * 1_000,
            proposer,
        );
        parent_hash = header.block_hash;
        bodies.push(SyncedBlockBody::new(height, header.block_hash, txs));
        headers_only.push(header);
    }

    // Sign the 3-of-4 Quorum Certificates in parallel across 4 threads.
    let chunk_size = headers_only.len().div_ceil(4).max(1);
    let headers_and_qcs: Vec<(SyncedHeader, SyncQc)> = std::thread::scope(|s| {
        let handles: Vec<_> = headers_only
            .chunks(chunk_size)
            .map(|slice| {
                let vals = &validators;
                s.spawn(move || {
                    let signers = [&vals[0], &vals[1], &vals[2]];
                    slice
                        .iter()
                        .map(|h| {
                            let qc = SyncQc::sign(CHAIN, h.height, 1, h.block_hash, &signers);
                            (h.clone(), qc)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });

    (validators, val_set, headers_and_qcs, bodies)
}

#[test]
fn lagging_node_catches_up_ten_thousand_blocks_and_reaches_same_head() {
    const TOTAL_BLOCKS: u64 = 10_000;
    let (_, val_set, headers_and_qcs, bodies) = build_canonical_chain(TOTAL_BLOCKS);

    let canonical_head_hash = headers_and_qcs.last().unwrap().0.block_hash;
    let canonical_state_root = headers_and_qcs.last().unwrap().0.state_root;

    let mut lagging_node =
        BlockSyncEngine::new(CHAIN, val_set, GENESIS_HASH, GENESIS_STATE, GENESIS_TS_MS);
    assert_eq!(lagging_node.synced_head_height(), 0);

    let header_peer = PeerIdentity::from_seed([0xA0; 32]).peer_id();
    let now_ms = GENESIS_TS_MS + 20_000_000;

    // Phase 1 (header-first): ingest all 10,000 headers + QCs in 4 batches of 2,500.
    for batch in headers_and_qcs.chunks(2_500) {
        let count = lagging_node
            .ingest_header_batch(header_peer, batch.to_vec(), now_ms)
            .expect("header+QC batch verifies");
        assert_eq!(count, 2_500);
    }
    assert_eq!(lagging_node.header_tip_height(), TOTAL_BLOCKS);
    assert_eq!(
        lagging_node.synced_head_height(),
        0,
        "synced head does not advance until bodies are downloaded and verified"
    );

    // Phase 2 (parallel body download): 4 peers serve 2,500 bodies each in
    // reverse chunk order (out-of-order completion: 7501..10000 first, 1..2500 last).
    let worker_peers: Vec<_> = (0..4u8)
        .map(|i| PeerIdentity::from_seed([0xB0 + i; 32]).peer_id())
        .collect();
    let mut parallel_chunks: Vec<_> = bodies
        .chunks(2_500)
        .enumerate()
        .map(|(idx, slice)| (worker_peers[idx], slice.to_vec()))
        .collect();
    parallel_chunks.reverse();

    let downloaded = lagging_node
        .download_bodies_parallel(parallel_chunks, now_ms)
        .expect("parallel body download succeeds");
    assert_eq!(downloaded, TOTAL_BLOCKS as usize);

    // Lagging node must reach height 10,000 with the exact canonical head hash and state root.
    assert_eq!(lagging_node.synced_head_height(), TOTAL_BLOCKS);
    assert_eq!(lagging_node.synced_head_hash(), canonical_head_hash);
    assert_eq!(lagging_node.synced_state_root(), canonical_state_root);
}

#[test]
fn corrupted_body_peer_is_banned_and_lagging_node_recovers_from_honest_peer() {
    let (_, val_set, headers_and_qcs, bodies) = build_canonical_chain(20);
    let mut node = BlockSyncEngine::new(CHAIN, val_set, GENESIS_HASH, GENESIS_STATE, GENESIS_TS_MS);
    let honest_peer = PeerIdentity::from_seed([0xC1; 32]).peer_id();
    let malicious_peer = PeerIdentity::from_seed([0xC2; 32]).peer_id();
    let now_ms = GENESIS_TS_MS + 100_000;

    node.ingest_header_batch(honest_peer, headers_and_qcs.clone(), now_ms)
        .unwrap();

    // Malicious peer corrupts block 10's body in its parallel chunk.
    let mut bad_chunk = bodies[0..10].to_vec();
    bad_chunk[9].transactions = vec![b"corrupted-payload".to_vec()];
    let err = node
        .download_bodies_parallel(
            vec![
                (malicious_peer, bad_chunk),
                (honest_peer, bodies[10..20].to_vec()),
            ],
            now_ms,
        )
        .unwrap_err();
    assert_eq!(err, P2pError::BlockSyncTxRootMismatch { height: 10 });
    assert!(node.ban_list().check_peer(&malicious_peer, now_ms).is_err());

    // Re-requesting the chunks from the honest peer completes sync to height 20.
    node.download_bodies_parallel(vec![(honest_peer, bodies)], now_ms)
        .unwrap();
    assert_eq!(node.synced_head_height(), 20);
    assert_eq!(
        node.synced_head_hash(),
        headers_and_qcs.last().unwrap().0.block_hash
    );
}
