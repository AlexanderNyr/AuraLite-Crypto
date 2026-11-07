//! B6 integration tests — Slashing & Byzantine evidence (`D-B4`):
//! 6 attack types (`DoubleSignProposal`, `DoubleSignVote`, `SurroundVote`,
//! `AmnesiaVote`, `InvalidVrfLeaderClaim`, `ExtendedDowntime`), stake
//! penalties, validator jailing/tombstoning, reporter/treasury/burn
//! distribution, and pinned 64-case vector fixture verification.

use aura_consensus::bls::{BlsSecretKey, address_from_public_key};
use aura_consensus::qc::{QcAggregator, Vote, VoteType};
use aura_consensus::signature::BlockSignature;
use aura_consensus::slashing::{
    CheckpointAttestation, PenaltyDistribution, SlashingAttackKind, SlashingEngine, SlashingError,
    SlashingEvidence, SlashingParams, ValidatorSlashingRecord,
};
use aura_consensus::validator_set::{Validator, ValidatorSet, ValidatorStatus};
use aura_consensus::vrf_leader::{LeaderClaim, VrfInput, prove_vrf};
use aura_crypto::ed25519_public_key;
use aura_types::{Address, ChainId, Domain, Hash};

const FIXTURE: &str = include_str!("fixtures/b6_slashing.txt");

struct TestVal {
    ed_seed: [u8; 32],
    bls_sk: BlsSecretKey,
    record: ValidatorSlashingRecord,
}

fn make_validators(count: usize, initial_stake: u64) -> Vec<TestVal> {
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
                    current_stake: initial_stake,
                    jailed: false,
                    tombstoned: false,
                    slash_count: 0,
                },
            }
        })
        .collect()
}

fn sign_vote(
    val: &TestVal,
    chain_id: &ChainId,
    view: u64,
    round: u64,
    block_hash: Hash,
    vote_type: VoteType,
) -> Vote {
    let mut msg = Vec::with_capacity(49);
    msg.extend_from_slice(&view.to_le_bytes());
    msg.extend_from_slice(&round.to_le_bytes());
    msg.extend_from_slice(block_hash.as_bytes());
    msg.push(vote_type as u8);
    let signature = val.bls_sk.sign(chain_id.as_str(), Domain::Vote, &msg);
    Vote {
        view,
        round,
        block_hash,
        validator: val.record.address,
        vote_type,
        signature,
    }
}

fn sign_checkpoint(
    val: &TestVal,
    chain_id: &ChainId,
    source_epoch: u64,
    target_epoch: u64,
    target_hash: Hash,
) -> CheckpointAttestation {
    let msg = CheckpointAttestation::transcript(source_epoch, target_epoch, &target_hash);
    let signature = val.bls_sk.sign(chain_id.as_str(), Domain::Vote, &msg);
    CheckpointAttestation {
        validator: val.record.address,
        source_epoch,
        target_epoch,
        target_hash,
        signature,
    }
}

#[test]
fn b6_six_attack_types_slash_validator_set_and_distribute_penalties() {
    let chain_id = ChainId::new("auralite-b6-integration").unwrap();
    let vals = make_validators(6, 10_000_000);
    let genesis_validators: Vec<Validator> = vals
        .iter()
        .map(|v| Validator {
            address: v.record.address,
            stake: v.record.current_stake,
            status: ValidatorStatus::Active,
            epoch: 0,
        })
        .collect();
    let mut validator_set = ValidatorSet::new(genesis_validators, 100);
    assert_eq!(validator_set.len(), 6);
    assert_eq!(validator_set.total_stake(), 60_000_000);

    let records = vals.iter().map(|v| v.record.clone()).collect();
    let mut engine = SlashingEngine::new(chain_id.clone(), SlashingParams::default(), records);
    let reporter = Address::from_bytes([0x99; 20]);

    // 1. DoubleSignProposal (validator 0)
    let ev1 = SlashingEvidence::DoubleSignProposal {
        view: 5,
        round: 0,
        first: BlockSignature::sign_block(
            &vals[0].ed_seed,
            vals[0].record.address,
            Hash::from_bytes([0x01; 32]),
            chain_id.as_str(),
        ),
        second: BlockSignature::sign_block(
            &vals[0].ed_seed,
            vals[0].record.address,
            Hash::from_bytes([0x02; 32]),
            chain_id.as_str(),
        ),
    };

    // 2. DoubleSignVote (validator 1)
    let ev2 = SlashingEvidence::DoubleSignVote {
        first: sign_vote(
            &vals[1],
            &chain_id,
            6,
            0,
            Hash::from_bytes([0x03; 32]),
            VoteType::Precommit,
        ),
        second: sign_vote(
            &vals[1],
            &chain_id,
            6,
            0,
            Hash::from_bytes([0x04; 32]),
            VoteType::Precommit,
        ),
    };

    // 3. SurroundVote (validator 2)
    let ev3 = SlashingEvidence::SurroundVote {
        outer: sign_checkpoint(&vals[2], &chain_id, 2, 12, Hash::from_bytes([0x05; 32])),
        inner: sign_checkpoint(&vals[2], &chain_id, 4, 9, Hash::from_bytes([0x06; 32])),
    };

    // 4. AmnesiaVote (validator 3)
    let ev4 = SlashingEvidence::AmnesiaVote {
        earlier: sign_vote(
            &vals[3],
            &chain_id,
            10,
            0,
            Hash::from_bytes([0x07; 32]),
            VoteType::Precommit,
        ),
        later: sign_vote(
            &vals[3],
            &chain_id,
            14,
            0,
            Hash::from_bytes([0x08; 32]),
            VoteType::Precommit,
        ),
        highest_justify_view: 8,
    };

    // 5. InvalidVrfLeaderClaim (validator 4)
    let vrf_input = VrfInput::new(chain_id.as_str(), 1, Hash::from_bytes([0x09; 32]), 20, 0);
    let mut proof = prove_vrf(&vals[4].bls_sk, &vrf_input).unwrap();
    proof.output = Hash::from_bytes([0xEE; 32]);
    let ev5 = SlashingEvidence::InvalidVrfLeaderClaim {
        vrf_input,
        claim: LeaderClaim { proof },
    };

    // 6. ExtendedDowntime (validator 5)
    let qc_hash = Hash::from_bytes([0x0B; 32]);
    let mut agg = QcAggregator::with_signers(
        chain_id.as_str(),
        &vals[0..4]
            .iter()
            .map(|v| v.record.bls_pubkey.clone())
            .collect::<Vec<_>>(),
    );
    let mut window_qc = None;
    for signer in &vals[0..4] {
        let vote = sign_vote(signer, &chain_id, 30, 0, qc_hash, VoteType::Precommit);
        if let Some(qc) = agg.add_vote(signer.record.bls_pubkey.clone(), vote) {
            window_qc = Some(qc);
        }
    }
    let ev6 = SlashingEvidence::ExtendedDowntime {
        validator: vals[5].record.address,
        epoch: 3,
        missed_blocks: 55,
        window_blocks: 100,
        window_qc: window_qc.unwrap(),
    };

    for ev in [&ev1, &ev2, &ev3, &ev4, &ev5, &ev6] {
        let receipt = engine
            .submit_evidence(ev, reporter, Some(&mut validator_set))
            .unwrap();
        assert!(receipt.jailed);
        assert!(!validator_set.is_active(&receipt.offender));
    }

    assert_eq!(engine.receipts().len(), 6);
    assert_eq!(
        engine.reporter_balance(&reporter) + engine.treasury_balance() + engine.total_burned(),
        4_600_000
    );

    // Disabled slashing engine must reject evidence with SlashingDisabled.
    let disabled_params = SlashingParams {
        slashing_enabled: false,
        ..Default::default()
    };
    let mut disabled_engine = SlashingEngine::new(
        chain_id,
        disabled_params,
        vals.iter().map(|v| v.record.clone()).collect(),
    );
    assert_eq!(
        disabled_engine.submit_evidence(&ev1, reporter, None),
        Err(SlashingError::SlashingDisabled)
    );
}

#[test]
fn b6_slashing_vector_fixture_matches_deterministic_policy() {
    let params = SlashingParams::default();
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
        assert_eq!(parts.len(), 11);
        let kind = match parts[0] {
            "DoubleSignProposal" => SlashingAttackKind::DoubleSignProposal,
            "DoubleSignVote" => SlashingAttackKind::DoubleSignVote,
            "SurroundVote" => SlashingAttackKind::SurroundVote,
            "AmnesiaVote" => SlashingAttackKind::AmnesiaVote,
            "InvalidVrfLeaderClaim" => SlashingAttackKind::InvalidVrfLeaderClaim,
            "ExtendedDowntime" => SlashingAttackKind::ExtendedDowntime,
            other => panic!("unknown attack kind: {other}"),
        };
        let stake_before: u64 = parts[1].parse().unwrap();
        let bps: u16 = parts[2].parse().unwrap();
        let total_slashed: u64 = parts[3].parse().unwrap();
        let reporter_reward: u64 = parts[4].parse().unwrap();
        let treasury_share: u64 = parts[5].parse().unwrap();
        let burned_amount: u64 = parts[6].parse().unwrap();
        let stake_after: u64 = parts[7].parse().unwrap();
        let tombstoned: bool = parts[9].parse().unwrap();

        let (policy_bps, policy_tombstone) = params.policy_for(kind);
        assert_eq!(bps, policy_bps);
        assert_eq!(tombstoned, policy_tombstone);
        let dist = PenaltyDistribution::compute(total_slashed, &params);
        assert_eq!(dist.reporter_reward, reporter_reward);
        assert_eq!(dist.treasury_share, treasury_share);
        assert_eq!(dist.burned_amount, burned_amount);
        assert_eq!(stake_before - total_slashed, stake_after);
        count += 1;
    }
    assert_eq!(count, 64);
}
