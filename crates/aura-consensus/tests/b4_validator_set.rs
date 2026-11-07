//! B4 validator-set gate: 4 -> 7 validator transition at height H without
//! stopping the network, plus QC-verified set updates.

use aura_consensus::bls::{BlsSecretKey, address_from_public_key};
use aura_consensus::finality::FinalityEngine;
use aura_consensus::proposer::Proposer;
use aura_consensus::qc::{QcAggregator, Vote, VoteType, quorum_threshold};
use aura_consensus::validator_set::{
    DEFAULT_MIN_STAKE, SetUpdate, SetUpdateError, StakingContract, Validator, ValidatorSet,
    ValidatorStatus,
};
use aura_types::{Address, Domain, Hash};

const CHAIN: &str = "auralite-devnet-1";

#[test]
fn four_to_seven_validators_transition_at_height_h_across_all_nodes() {
    const HEIGHT_H: u64 = 50;
    const TOTAL_BLOCKS: u64 = 100;

    let keys: Vec<BlsSecretKey> = (0..7u8)
        .map(|i| BlsSecretKey::from_seed(&[i + 1; 32]))
        .collect();
    let addrs: Vec<Address> = keys
        .iter()
        .map(|k| address_from_public_key(&k.public_key()))
        .collect();

    let genesis_validators: Vec<Validator> = (0..4)
        .map(|i| Validator {
            address: addrs[i],
            stake: DEFAULT_MIN_STAKE,
            status: ValidatorStatus::Active,
            epoch: 0,
        })
        .collect();

    let genesis_hash = Hash::from_bytes([0u8; 32]);
    let mut nodes: Vec<(StakingContract, FinalityEngine)> = (0..7)
        .map(|_| {
            let mut staking = StakingContract::new(genesis_validators.clone(), HEIGHT_H);
            for addr in &addrs[4..7] {
                staking
                    .stake_and_enter(*addr, DEFAULT_MIN_STAKE * 2)
                    .expect("stake and enter succeeds");
            }
            let engine = FinalityEngine::new(CHAIN, 0, genesis_hash);
            (staking, engine)
        })
        .collect();

    for height in 1..=TOTAL_BLOCKS {
        let active_addrs: Vec<Address> = nodes[0]
            .0
            .validator_set()
            .active_addresses()
            .into_iter()
            .collect();
        let n_active = active_addrs.len();
        if height <= HEIGHT_H {
            assert_eq!(
                n_active, 4,
                "before/at height H there are 4 active validators"
            );
        } else {
            assert_eq!(n_active, 7, "after height H there are 7 active validators");
        }

        let proposer = Proposer::new(active_addrs);
        let _leader = proposer.propose(height - 1);
        let timestamp_ms = 1_700_000_000_000 + height * 100;
        let txs = vec![format!("tx-{height}").into_bytes()];

        for (staking, engine) in &mut nodes {
            let block = engine.propose_at(timestamp_ms, txs.clone());
            let mut agg = QcAggregator::new(CHAIN, staking.validator_set().active_addresses());
            let mut qc_opt = None;
            for (idx, addr) in addrs.iter().enumerate() {
                if !staking.validator_set().is_active(addr) {
                    continue;
                }
                let vote = engine.vote_for(&keys[idx], *addr, &block, VoteType::Precommit);
                let (maybe_qc, _) =
                    engine.aggregate_and_commit(&mut agg, keys[idx].public_key(), vote);
                if let Some(qc) = maybe_qc {
                    assert_eq!(
                        qc.signatures.len(),
                        quorum_threshold(n_active as u64) as usize
                    );
                    qc_opt = Some(qc);
                    break;
                }
            }
            let qc = qc_opt.expect("quorum reached");
            qc.verify(CHAIN, Domain::Vote).expect("QC verifies");
            engine.change_round();
            let crossed = staking.validator_set_mut().advance_block();
            if height == HEIGHT_H {
                assert_eq!(crossed.expect("epoch transition at H").number, 1);
                assert_eq!(staking.validator_set().len(), 7);
            }
        }
    }

    let expected_count = nodes[0].1.commit_rule.finalized_count();
    let expected_tip = nodes[0].1.tip.block_hash;
    assert!(expected_count >= 95);
    for (idx, (staking, engine)) in nodes.iter().enumerate() {
        assert_eq!(
            staking.validator_set().len(),
            7,
            "node {idx} has 7 active validators"
        );
        assert_eq!(
            engine.commit_rule.finalized_count(),
            expected_count,
            "node {idx} finalized count matches node 0"
        );
        assert_eq!(
            engine.tip.block_hash, expected_tip,
            "node {idx} finalized tip matches node 0"
        );
    }
}

#[test]
fn qc_authorized_set_update_replaces_validator_set_and_rejects_bad_updates() {
    let keys: Vec<BlsSecretKey> = (0..7u8)
        .map(|i| BlsSecretKey::from_seed(&[i + 11; 32]))
        .collect();
    let addrs: Vec<Address> = keys
        .iter()
        .map(|k| address_from_public_key(&k.public_key()))
        .collect();

    let genesis: Vec<Validator> = (0..4)
        .map(|i| Validator {
            address: addrs[i],
            stake: DEFAULT_MIN_STAKE,
            status: ValidatorStatus::Active,
            epoch: 0,
        })
        .collect();
    let mut set = ValidatorSet::new(genesis, 100);
    let tip = Hash::from_bytes([0xAB; 32]);

    // Build a valid 3-of-4 BLS QC over `tip`.
    let mut agg = QcAggregator::with_signers(
        CHAIN,
        &keys[..4].iter().map(|k| k.public_key()).collect::<Vec<_>>(),
    );
    let mut qc_opt = None;
    for i in 0..3 {
        let payload: Vec<u8> = [
            &0u64.to_le_bytes()[..],
            &10u64.to_le_bytes()[..],
            tip.as_bytes(),
            &[VoteType::Precommit as u8],
        ]
        .concat();
        let sig = keys[i].sign(CHAIN, Domain::Vote, &payload);
        let vote = Vote {
            view: 0,
            round: 10,
            block_hash: tip,
            validator: addrs[i],
            vote_type: VoteType::Precommit,
            signature: sig,
        };
        if let Some(qc) = agg.add_vote(keys[i].public_key(), vote) {
            qc_opt = Some(qc);
        }
    }
    let valid_qc = qc_opt.expect("3-of-4 QC formed");

    let seven_validators: Vec<Validator> = (0..7)
        .map(|i| Validator {
            address: addrs[i],
            stake: DEFAULT_MIN_STAKE * 2,
            status: ValidatorStatus::Active,
            epoch: 1,
        })
        .collect();

    // Wrong chain_id fails cryptographic QC verification.
    let bad_chain = set.apply_verified_set_update(
        SetUpdate {
            epoch: 1,
            validators: seven_validators.clone(),
            qc: valid_qc.clone(),
        },
        &tip,
        "wrong-chain-id",
    );
    assert_eq!(bad_chain, Err(SetUpdateError::InvalidQc));

    // Valid QC under CHAIN updates 4 -> 7 validators at epoch 1.
    set.apply_verified_set_update(
        SetUpdate {
            epoch: 1,
            validators: seven_validators,
            qc: valid_qc,
        },
        &tip,
        CHAIN,
    )
    .expect("verified set update succeeds");
    assert_eq!(set.len(), 7);
    assert_eq!(set.epoch(), 1);
    assert_eq!(set.total_stake(), DEFAULT_MIN_STAKE * 14);
}
