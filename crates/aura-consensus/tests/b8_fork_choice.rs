//! B8 integration tests — Fork choice, finalized checkpoints, consensus
//! light-client 3-chain proofs, deep-reorg rejection with cryptographic
//! [`SlashingEvidence`] emission (`D-B3`, `D-B4`), and 64-case vector fixture
//! verification.

use aura_consensus::bls::{BlsSecretKey, address_from_public_key};
use aura_consensus::finality::FinalityBlock;
use aura_consensus::fork_choice::{
    ConsensusLightClientProof, FinalizedCheckpoint, ForkChoiceError, ForkChoiceTree,
    SignedForkBlock,
};
use aura_consensus::qc::{QcAggregator, QuorumCertificate, Vote, VoteType};
use aura_consensus::signature::BlockSignature;
use aura_consensus::slashing::{SlashingEngine, SlashingParams, ValidatorSlashingRecord};
use aura_crypto::ed25519_public_key;
use aura_types::{ChainId, Domain, Hash};

const FIXTURE: &str = include_str!("fixtures/b8_fork_choice.txt");

struct TestVal {
    ed_seed: [u8; 32],
    bls_sk: BlsSecretKey,
    record: ValidatorSlashingRecord,
}

fn make_validators(count: usize) -> Vec<TestVal> {
    (0..count)
        .map(|i| {
            let seed = [(i as u8) + 1; 32];
            let bls_sk = BlsSecretKey::from_seed(&seed);
            let bls_pk = bls_sk.public_key();
            let address = address_from_public_key(&bls_pk);
            let ed_pk = ed25519_public_key(&seed);
            TestVal {
                ed_seed: seed,
                bls_sk,
                record: ValidatorSlashingRecord {
                    address,
                    ed25519_pubkey: ed_pk,
                    bls_pubkey: bls_pk,
                    current_stake: 10_000_000,
                    jailed: false,
                    tombstoned: false,
                    slash_count: 0,
                },
            }
        })
        .collect()
}

fn build_qc(
    vals: &[TestVal],
    chain_id: &ChainId,
    view: u64,
    round: u64,
    block_hash: Hash,
    signers: usize,
) -> QuorumCertificate {
    let mut agg = QcAggregator::with_signers(
        chain_id.as_str(),
        &vals[0..signers]
            .iter()
            .map(|v| v.record.bls_pubkey.clone())
            .collect::<Vec<_>>(),
    );
    let mut out = None;
    for v in &vals[0..signers] {
        let mut msg = Vec::with_capacity(49);
        msg.extend_from_slice(&view.to_le_bytes());
        msg.extend_from_slice(&round.to_le_bytes());
        msg.extend_from_slice(block_hash.as_bytes());
        msg.push(VoteType::Precommit as u8);
        let sig = v.bls_sk.sign(chain_id.as_str(), Domain::Vote, &msg);
        let vote = Vote {
            view,
            round,
            block_hash,
            validator: v.record.address,
            vote_type: VoteType::Precommit,
            signature: sig,
        };
        if let Some(qc) = agg.add_vote(v.record.bls_pubkey.clone(), vote) {
            out = Some(qc);
        }
    }
    out.unwrap()
}

#[allow(clippy::too_many_arguments)]
fn make_signed_block(
    val: &TestVal,
    chain_id: &ChainId,
    height: u64,
    view: u64,
    round: u64,
    parent: &FinalityBlock,
    justified_qc_view: u64,
    tx_tag: &[u8],
) -> SignedForkBlock {
    let block = if height == 0 {
        FinalityBlock {
            view: 0,
            round: 0,
            parent_hash: Hash::ZERO,
            timestamp_ms: 1_000,
            block_hash: Hash::from_bytes([0x01; 32]),
            qc: None,
            transactions: Vec::new(),
        }
    } else {
        FinalityBlock::propose(
            view,
            round,
            parent,
            1_000 + height * 1_000,
            vec![tx_tag.to_vec()],
        )
    };
    let proposer_sig = BlockSignature::sign_block(
        &val.ed_seed,
        val.record.address,
        block.block_hash,
        chain_id.as_str(),
    );
    SignedForkBlock {
        height,
        block,
        justified_qc_view,
        proposer_sig,
        attester_vote: None,
    }
}

#[test]
fn b8_deep_reorg_past_finalized_checkpoint_is_rejected_and_slashes_offender() {
    let chain_id = ChainId::new("auralite-b8-integration").unwrap();
    let vals = make_validators(4);
    let dummy = FinalityBlock {
        view: 0,
        round: 0,
        parent_hash: Hash::ZERO,
        timestamp_ms: 0,
        block_hash: Hash::ZERO,
        qc: None,
        transactions: Vec::new(),
    };
    let genesis = make_signed_block(&vals[0], &chain_id, 0, 0, 0, &dummy, 0, b"genesis");
    let genesis_qc = build_qc(&vals, &chain_id, 0, 0, genesis.block.block_hash, 3);
    let mut tree = ForkChoiceTree::new(
        chain_id.clone(),
        vals.iter().map(|v| v.record.address).collect(),
        genesis.clone(),
        FinalizedCheckpoint {
            height: 0,
            view: 0,
            block_hash: genesis.block.block_hash,
            state_root: Hash::from_bytes([0x10; 32]),
            commit_qc: genesis_qc,
        },
    );

    // Build a 3-chain on view 1 and finalize b1 via ConsensusLightClientProof + FinalizedCheckpoint.
    let b1 = make_signed_block(&vals[1], &chain_id, 1, 1, 0, &genesis.block, 1, b"canon-b1");
    let b2 = make_signed_block(&vals[2], &chain_id, 2, 1, 1, &b1.block, 1, b"canon-b2");
    let b3 = make_signed_block(&vals[3], &chain_id, 3, 1, 2, &b2.block, 1, b"canon-b3");
    tree.insert_block(b1.clone()).unwrap();
    tree.insert_block(b2.clone()).unwrap();
    tree.insert_block(b3.clone()).unwrap();

    let qc1 = build_qc(&vals, &chain_id, 1, 0, b1.block.block_hash, 3);
    let qc2 = build_qc(&vals, &chain_id, 1, 1, b2.block.block_hash, 3);
    let qc3 = build_qc(&vals, &chain_id, 1, 2, b3.block.block_hash, 3);

    let lc_proof = ConsensusLightClientProof {
        target_block: b1.block.clone(),
        state_root: Hash::from_bytes([0x20; 32]),
        child_block: b2.block.clone(),
        grandchild_block: b3.block.clone(),
        target_qc: qc1.clone(),
        child_qc: qc2.clone(),
        grandchild_qc: qc3,
    };
    lc_proof
        .verify(
            &chain_id,
            &vals.iter().map(|v| v.record.address).collect(),
            lc_proof.state_root,
        )
        .unwrap();

    let trusted_root = Hash::from_bytes([0x30; 32]);
    let proof_child = FinalityBlock::propose(1, 2, &b2.block, 4_000, vec![]);
    let proof_grandchild = FinalityBlock::propose(1, 3, &proof_child, 5_000, vec![]);
    let checkpoint_proof = ConsensusLightClientProof {
        state_root: trusted_root,
        target_block: b2.block.clone(),
        child_qc: build_qc(&vals, &chain_id, 1, 2, proof_child.block_hash, 3),
        grandchild_qc: build_qc(&vals, &chain_id, 1, 3, proof_grandchild.block_hash, 3),
        target_qc: qc2.clone(),
        child_block: proof_child,
        grandchild_block: proof_grandchild,
    };
    tree.update_finalized_checkpoint(
        FinalizedCheckpoint {
            height: 2,
            view: 1,
            block_hash: b2.block.block_hash,
            state_root: Hash::from_bytes([0x30; 32]),
            commit_qc: qc2,
        },
        &checkpoint_proof,
        trusted_root,
        3,
    )
    .unwrap();

    // Validator 1 attempts a deep reorg at finalized height 1 -> rejected + SlashingEvidence emitted!
    let adv_b1 = make_signed_block(
        &vals[1],
        &chain_id,
        1,
        1,
        0,
        &genesis.block,
        50,
        b"evil-reorg",
    );
    let Err(Err(reorg_rejection)) = tree.insert_block(adv_b1) else {
        panic!("expected deep reorg to be rejected with SlashingEvidence");
    };
    assert!(matches!(
        reorg_rejection.error,
        ForkChoiceError::DeepReorgPastFinalized {
            finalized_height: 2,
            conflicting_height: 1,
            ..
        }
    ));
    assert_eq!(tree.select_canonical_head(), b3.block.block_hash);

    // Feed the emitted evidence into B6 SlashingEngine -> validator 1 is slashed 10% and tombstoned!
    let records = vals.iter().map(|v| v.record.clone()).collect();
    let mut slashing = SlashingEngine::new(chain_id, SlashingParams::default(), records);
    let receipt = slashing
        .submit_evidence(&reorg_rejection.evidence, vals[0].record.address, None)
        .unwrap();
    assert_eq!(receipt.offender, vals[1].record.address);
    assert_eq!(receipt.distribution.total_slashed, 1_000_000);
    assert!(receipt.tombstoned);
}

#[test]
fn b8_fork_choice_vector_fixture_matches_deterministic_rules() {
    let mut count = 0usize;
    for line in FIXTURE.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((_key, rest)) = line.split_once('=') else {
            continue;
        };
        let parts: Vec<&str> = rest.split('|').collect();
        assert_eq!(parts.len(), 7);

        let fin_h: u64 = parts[0].parse().unwrap();
        let cand_h: u64 = parts[1].parse().unwrap();
        let descends: bool = parts[2].parse().unwrap();
        let cand_qc: u64 = parts[3].parse().unwrap();
        let comp_qc: u64 = parts[4].parse().unwrap();
        let expected_outcome = parts[5];

        let computed = if cand_h <= fin_h || !descends {
            "REJECT_DEEP_REORG_WITH_EVIDENCE"
        } else if cand_qc > comp_qc {
            "SELECT_CANDIDATE"
        } else {
            "KEEP_COMPETING"
        };
        assert_eq!(computed, expected_outcome);
        count += 1;
    }
    assert_eq!(count, 64);
}
