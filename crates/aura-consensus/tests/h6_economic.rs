//! H6 economic-attack simulations (docs/h6-economic-plan.md §2).
//!
//! Every property here is a deterministic simulation of one economic attack
//! vector from the SECURITY_PLAN H6 row: MEV/censorship bounds, stake
//! grinding bounds, 34% emission conservation, oracle/bridge modelled
//! surfaces, and the emergency-pause gate ("no pause -> red").
//! `tools/nc_h6.py` injects the bugs these tests must catch.

use std::collections::BTreeMap;

use aura_consensus::bls::{BlsPublicKey, BlsSecretKey, address_from_public_key};
use aura_consensus::finality::{FinalityBlock, FinalityEngine};
use aura_consensus::governance::{
    GovernanceEngine, GovernanceError, GovernanceParams, build_runtime_wasm_artifact,
};
use aura_consensus::proposer::Proposer;
use aura_consensus::qc::{QuorumCertificate, VoteType};
use aura_consensus::rewards::{RewardParams, RewardsEngine, ValidatorRewardConfig};
use aura_consensus::slashing::{
    SlashingAttackKind, SlashingEngine, SlashingEvidence, SlashingParams, ValidatorSlashingRecord,
};
use aura_consensus::validator_set::{Validator, ValidatorSet, ValidatorStatus};
use aura_consensus::vrf_leader::{
    LeaderClaim, VrfInput, VrfLeaderElection, VrfValidatorEntry, prove_vrf,
};
use aura_crypto::ed25519_public_key;
use aura_types::{Address, ChainId, Hash};

const CHAIN: &str = "auralite-h6-economic";

fn make_validator(seed: u8, stake: u64) -> (BlsSecretKey, VrfValidatorEntry) {
    let sk = BlsSecretKey::from_seed(&[seed; 32]);
    let pk = sk.public_key();
    let address = address_from_public_key(&pk);
    (
        sk,
        VrfValidatorEntry {
            address,
            public_key: pk,
            stake,
        },
    )
}

/// D-H6-3: stake grinding must not bias leader selection (limits bound),
/// and a forged leader claim must burn stake (slashing bound).
#[test]
fn h6_stake_grinding_bounded() {
    let (_sk1, v1) = make_validator(11, 1_000_000); // 10%
    let (_sk2, v2) = make_validator(22, 2_000_000); // 20%
    let (_sk3, v3) = make_validator(33, 3_000_000); // 30%
    let (_sk4, v4) = make_validator(44, 4_000_000); // 40%
    let a1 = v1.address;
    let a2 = v2.address;
    let a3 = v3.address;
    let a4 = v4.address;

    let entries = vec![v1, v2, v3, v4];
    let election = VrfLeaderElection::new(entries).expect("election table valid");
    assert_eq!(election.total_stake(), 10_000_000);

    // Limits bound: over deterministic rounds the selection share stays
    // proportional to stake — grinding cannot buy extra slots for free.
    const ROUNDS: u64 = 10_000;
    let epoch_rand = Hash::from_bytes([0x6A; 32]);
    let mut counts: BTreeMap<Address, u64> = BTreeMap::new();
    for round in 0..ROUNDS {
        let input = VrfInput::new(CHAIN, 1, epoch_rand, 0, round);
        let leader = election.select_leader(&input).expect("leader selected");
        *counts.entry(leader).or_insert(0) += 1;
    }
    let total = f64::from(u32::try_from(ROUNDS).unwrap());
    for (addr, stake) in [
        (a1, 1_000_000u64),
        (a2, 2_000_000),
        (a3, 3_000_000),
        (a4, 4_000_000),
    ] {
        let share = f64::from(u32::try_from(*counts.get(&addr).unwrap_or(&0)).unwrap()) / total;
        let expected = f64::from(u32::try_from(stake).unwrap()) / 10_000_000.0;
        assert!(
            (share - expected).abs() < 0.03,
            "H6: stake grinding must not bias leader selection ({addr:?} {share:.4} vs {expected:.4})"
        );
    }

    // Slashing bound: a forged (ground) leader claim — tampered VRF output —
    // is slashable evidence; the attacker's stake must burn.
    let chain_id = ChainId::new(CHAIN).expect("valid chain id");
    let vals: Vec<ValidatorSlashingRecord> = (11u8..15)
        .map(|seed| {
            let sk = BlsSecretKey::from_seed(&[seed; 32]);
            let pk = sk.public_key();
            ValidatorSlashingRecord {
                address: address_from_public_key(&pk),
                ed25519_pubkey: ed25519_public_key(&[seed; 32]),
                bls_pubkey: pk,
                current_stake: 10_000_000,
                jailed: false,
                tombstoned: false,
                slash_count: 0,
            }
        })
        .collect();
    let genesis: Vec<Validator> = vals
        .iter()
        .map(|r| Validator {
            address: r.address,
            stake: r.current_stake,
            status: ValidatorStatus::Active,
            epoch: 0,
        })
        .collect();
    let mut validator_set = ValidatorSet::new(genesis, 100);
    let mut engine = SlashingEngine::new(chain_id.clone(), SlashingParams::default(), vals.clone());

    let vrf_input = VrfInput::new(CHAIN, 1, Hash::from_bytes([0x09; 32]), 20, 0);
    let sk_offender = BlsSecretKey::from_seed(&[14; 32]); // vals[3]
    let mut proof = prove_vrf(&sk_offender, &vrf_input).expect("vrf proof");
    proof.output = Hash::from_bytes([0xEE; 32]); // forged output
    let evidence = SlashingEvidence::InvalidVrfLeaderClaim {
        vrf_input,
        claim: LeaderClaim { proof },
    };
    let reporter = Address::from_bytes([0x99; 20]);
    let receipt = engine
        .submit_evidence(&evidence, reporter, Some(&mut validator_set))
        .expect("forged leader claim is valid slash evidence");
    assert_eq!(
        receipt.attack_kind,
        SlashingAttackKind::InvalidVrfLeaderClaim
    );
    assert!(
        receipt.stake_after < receipt.stake_before,
        "H6: attacker damage must be bounded by slashing"
    );
}

/// D-H6-4: a 34% attacker driving proposer/fees cannot inflate emission.
#[test]
fn h6_emission_conserved_under_attack() {
    let mut configs = Vec::new();
    let mut addresses = Vec::new();
    for seed in 21u8..25 {
        let sk = BlsSecretKey::from_seed(&[seed; 32]);
        let addr = address_from_public_key(&sk.public_key());
        addresses.push(addr);
        let mut cfg = ValidatorRewardConfig::new(addr, 25_000_000, 1_000).expect("config");
        cfg.delegations
            .insert(Address::from_bytes([seed; 20]), 5_000_000);
        configs.push(cfg);
    }
    let fee_payer = Address::from_bytes([0x7E; 20]);
    let mut extra: BTreeMap<Address, u128> = BTreeMap::new();
    extra.insert(fee_payer, u128::MAX / 4);
    let mut engine =
        RewardsEngine::new(RewardParams::default(), configs, extra).expect("rewards engine");

    // Deterministic adversarial schedule: the 34% attacker is proposer for
    // most blocks and pushes extreme fees; conservation must survive.
    for step in 0..64u64 {
        let proposer = if step % 3 == 0 {
            addresses[0] // honest rotation
        } else {
            addresses[1] // the attacker's slot
        };
        let tx_fees: u64 = if step % 7 == 0 {
            step * 1_000_000
        } else if step % 5 == 0 {
            0
        } else {
            12_345
        };
        engine
            .apply_block_rewards(step, proposer, tx_fees, Some(fee_payer))
            .expect("reward application accepted");
        engine
            .verify_sum_invariant()
            .expect("H6: emission must stay conserved under attack");
    }
    // Unknown proposers and unknown delegators must not create value.
    assert!(
        engine
            .apply_block_rewards(64, Address::from_bytes([0x01; 20]), 1, Some(fee_payer))
            .is_err()
    );
    assert!(
        engine
            .delegate(
                Address::from_bytes([0x02; 20]),
                Address::from_bytes([0x03; 20]),
                1
            )
            .is_err()
    );
    engine
        .verify_sum_invariant()
        .expect("H6: emission must stay conserved under attack");
}

/// D-H6-5 (modelled surface, plan §3): stake-weighted median of reports
/// keeps the manipulated price inside the honest report range.
#[test]
fn h6_oracle_manipulation_bounded() {
    // (price, stake): 3 attacker reporters with extreme prices, 7 honest
    // reporters inside [99, 101].
    let mut reports: Vec<(u128, u64)> =
        vec![(1, 1_000_000), (10_000, 1_000_000), (1_000_000, 1_000_000)];
    for i in 0..7u64 {
        reports.push((99 + u128::from(i % 3), 1_000_000));
    }
    let median = oracle_median(&reports);
    assert!(
        (99..=101).contains(&median),
        "H6: oracle manipulation must stay within report bounds (median={median})"
    );
    // Metamorphism: report order must not matter.
    let mut shuffled = reports.clone();
    shuffled.reverse();
    assert_eq!(
        oracle_median(&shuffled),
        median,
        "H6: oracle manipulation must stay within report bounds"
    );
}

/// Stake-weighted median over (price, stake) reports.
fn oracle_median(reports: &[(u128, u64)]) -> u128 {
    let mut expanded: Vec<u128> = Vec::new();
    for (price, stake) in reports {
        for _ in 0..*stake / 1_000_000 {
            expanded.push(*price);
        }
    }
    expanded.sort_unstable();
    // H6: the stake-weighted median bounds manipulation.
    expanded[expanded.len() / 2]
}

/// D-H6-6 (modelled surface, plan §3): a peg with fixed reserves cannot be
/// drained — overdraw, replay and forged amounts are rejected.
#[test]
fn h6_bridge_drain_bounded() {
    let reserves: u128 = 1_000;
    // (amount, nonce): overdraw, legit, replay of the same nonce, forged
    // huge amount, then another legit withdrawal.
    let requests: [(u128, u64); 5] = [(1_500, 1), (400, 2), (400, 2), (u128::MAX, 3), (500, 4)];
    let mut state = BridgeState {
        reserves,
        withdrawn: 0,
        used_nonces: BTreeMap::new(),
    };
    for (amount, nonce) in requests {
        bridge_attempt(&mut state, amount, nonce);
        assert!(
            state.withdrawn <= state.reserves,
            "H6: bridge withdrawals must stay within reserves"
        );
    }
    // Only the two legitimate withdrawals (400 + 500) settle.
    assert_eq!(
        state.withdrawn, 900,
        "H6: bridge withdrawals must stay within reserves"
    );
}

struct BridgeState {
    reserves: u128,
    withdrawn: u128,
    used_nonces: BTreeMap<u64, ()>,
}

fn bridge_attempt(state: &mut BridgeState, amount: u128, nonce: u64) {
    if state.used_nonces.contains_key(&nonce) {
        return; // replay rejected
    }
    if state.withdrawn.saturating_add(amount) > state.reserves {
        return; // overdraw rejected by the reserve limit
    }
    state.used_nonces.insert(nonce, ());
    state.withdrawn += amount;
}

/// D-H6-7 (the named gate): while the emergency pause is active, governance
/// actions must be rejected; only a valid QC lifts it.
#[test]
fn h6_emergency_pause_halts_actions() {
    let chain_id = ChainId::new(CHAIN).expect("valid chain id");
    let keys: Vec<(Address, BlsSecretKey, BlsPublicKey)> = (31u8..35)
        .map(|seed| {
            let sk = BlsSecretKey::from_seed(&[seed; 32]);
            let pk = sk.public_key();
            (address_from_public_key(&pk), sk, pk)
        })
        .collect();
    let validator_pubkeys: BTreeMap<Address, BlsPublicKey> =
        keys.iter().map(|(a, _, pk)| (*a, pk.clone())).collect();
    let (genesis_wasm, _) = build_runtime_wasm_artifact(1, "h6-genesis");
    let mut gov = GovernanceEngine::new(
        chain_id.clone(),
        GovernanceParams::default(),
        validator_pubkeys,
        genesis_wasm,
    )
    .expect("governance engine");

    // Before the pause governance works.
    gov.submit_proposal(
        keys[0].0,
        100_000,
        1,
        aura_consensus::governance::ProposalPayload::ParameterChange {
            updated_params: GovernanceParams::default(),
        },
    )
    .expect("proposal accepted before pause");

    let qc = make_qc(&chain_id, &keys, 3, 1, 0, Hash::from_bytes([0x5A; 32]));
    gov.trigger_emergency_pause(2, "h6-drill".to_string(), &qc)
        .expect("2f+1 pause QC accepted");
    assert!(gov.emergency_pause().paused);

    // THE GATE: with the pause active, actions must fail.
    let blocked = gov.submit_proposal(
        keys[0].0,
        100_000,
        3,
        aura_consensus::governance::ProposalPayload::ParameterChange {
            updated_params: GovernanceParams::default(),
        },
    );
    assert!(
        matches!(blocked, Err(GovernanceError::EmergencyPaused { .. })),
        "H6: emergency pause must halt governance actions"
    );

    // Lift with a valid QC and governance resumes.
    let lift_qc = make_qc(&chain_id, &keys, 3, 2, 0, Hash::from_bytes([0x5B; 32]));
    gov.lift_emergency_pause(&lift_qc).expect("pause lifted");
    gov.submit_proposal(
        keys[0].0,
        100_000,
        4,
        aura_consensus::governance::ProposalPayload::ParameterChange {
            updated_params: GovernanceParams::default(),
        },
    )
    .expect("proposal accepted after lift");
}

fn make_qc(
    chain_id: &ChainId,
    keys: &[(Address, BlsSecretKey, BlsPublicKey)],
    signer_count: usize,
    view: u64,
    round: u64,
    block_hash: Hash,
) -> QuorumCertificate {
    let engine = FinalityEngine::new(chain_id.as_str(), view, block_hash);
    let dummy_block = FinalityBlock {
        view,
        round,
        parent_hash: block_hash,
        timestamp_ms: 0,
        block_hash,
        qc: None,
        transactions: Vec::new(),
    };
    let mut public_keys = Vec::new();
    let mut signatures = Vec::new();
    for (addr, sk, pk) in keys.iter().take(signer_count) {
        let vote = engine.vote_for(sk, *addr, &dummy_block, VoteType::Precommit);
        public_keys.push(pk.clone());
        signatures.push(vote.signature);
    }
    QuorumCertificate {
        view,
        round,
        block_hash,
        vote_type: VoteType::Precommit,
        public_keys,
        signatures,
    }
}

/// D-H6-2: a censoring proposer minority only delays inclusion — bounded by
/// round-robin rotation (`validators[round % len]`).
#[test]
fn h6_censorship_bounded_by_rotation() {
    let validators: Vec<Address> = (1..=4u8).map(|i| Address::from_bytes([i; 20])).collect();
    let attacker = validators[0];
    let scheduler = Proposer::new(validators.clone());
    let n = scheduler.validator_count();

    // One target tx enters the pool at round 0; the attacker censors during
    // its rounds, honest proposers include immediately.
    let mut pending: Vec<u64> = vec![42];
    let mut included_at: BTreeMap<u64, u64> = BTreeMap::new();
    for round in 0..(n as u64 * 3) {
        let proposer = scheduler.propose(round);
        assert!(
            scheduler.is_proposer(&proposer, round),
            "H6: censorship must be bounded by proposer rotation"
        );
        if proposer == attacker {
            continue; // censoring proposer includes nothing
        }
        for id in pending.drain(..) {
            included_at.insert(id, round);
        }
    }
    let round_included = *included_at
        .get(&42)
        .expect("H6: censorship must be bounded by proposer rotation");
    assert!(
        round_included < n as u64,
        "H6: censorship must be bounded by proposer rotation (delay {round_included})"
    );
}
