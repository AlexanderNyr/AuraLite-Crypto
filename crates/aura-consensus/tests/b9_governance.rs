//! B9 integration gate:
//! 1. First protocol upgrade in a 4-validator testnet (`v1 -> v2`) via
//!    proposal → vote → timelock → WASM runtime activation without any manual
//!    validator restart, plus emergency pause & resume.
//! 2. Verification of all 64 deterministic governance tally & timelock vectors
//!    in `tests/fixtures/b9_governance.txt`.

use std::collections::BTreeMap;

use aura_consensus::bls::{BlsPublicKey, BlsSecretKey, address_from_public_key};
use aura_consensus::finality::{FinalityBlock, FinalityEngine};
use aura_consensus::governance::{
    DEFAULT_MIN_PROPOSER_DEPOSIT, GovernanceEngine, GovernanceError, GovernanceParams,
    GovernanceVote, GovernanceVoteChoice, HotUpgradableTestnet, ProposalPayload, ProposalStatus,
    build_runtime_wasm_artifact,
};
use aura_consensus::qc::{QuorumCertificate, VoteType};
use aura_consensus::validator_set::{Validator, ValidatorSet, ValidatorStatus};
use aura_crypto::blake3_hash;
use aura_types::{Address, ChainId, Hash};

const GOV_VECTORS: &str = include_str!("fixtures/b9_governance.txt");

fn make_vote_qc(
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

#[test]
fn b9_first_protocol_upgrade_in_testnet_without_manual_validator_restart() {
    let chain_id = ChainId::new("aura-b9-testnet").expect("valid chain id");
    let (genesis_wasm, genesis_hash) = build_runtime_wasm_artifact(1, "testnet-genesis");
    let mut testnet =
        HotUpgradableTestnet::new(chain_id, GovernanceParams::default(), genesis_wasm)
            .expect("create 4-validator testnet");

    // 1. Run blocks 1..=5 on genesis protocol version 1.
    for _ in 0..5 {
        testnet.produce_block().expect("produce v1 block");
    }
    for node in &testnet.nodes {
        assert_eq!(node.governance.runtime().protocol_version, 1);
        assert_eq!(node.governance.runtime().wasm_code_hash, genesis_hash);
        assert_eq!(node.restart_count, 0);
    }

    // 2. Submit protocol upgrade proposal #1 (v1 -> v2) at height 5, scheduled for height 25.
    let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, "testnet-v2");
    let proposer = testnet.nodes[0].address;
    let prop_id = testnet
        .submit_proposal(
            proposer,
            DEFAULT_MIN_PROPOSER_DEPOSIT,
            ProposalPayload::RuntimeWasmUpgrade {
                from_version: 1,
                to_version: 2,
                wasm_bytecode: wasm_v2.clone(),
                wasm_code_hash: hash_v2,
                activation_height: 25,
            },
        )
        .expect("submit upgrade proposal");

    // 3. Validators vote during heights 6..=15.
    testnet.produce_block().expect("block 6");
    for idx in 0..4 {
        testnet
            .cast_validator_vote(idx, prop_id, GovernanceVoteChoice::Yes)
            .expect("cast Yes vote");
    }
    while testnet.height <= 15 {
        testnet
            .produce_block()
            .expect("advance through voting period");
    }

    // 4. Finalize voting at height 16 -> enters Timelocked(earliest_execution_height = 25).
    let status = testnet.finalize_voting(prop_id).expect("finalize voting");
    assert_eq!(
        status,
        ProposalStatus::Timelocked {
            earliest_execution_height: 25
        }
    );

    // 5. Blocks 17..=24 remain on v1 while in timelock; block 25 activates v2 across all 4 validators
    // without stopping or restarting any validator node!
    while testnet.height < 24 {
        testnet.produce_block().expect("timelock block");
    }
    for node in &testnet.nodes {
        assert_eq!(node.governance.runtime().protocol_version, 1);
    }

    while testnet.height < 35 {
        testnet.produce_block().expect("post-upgrade block");
    }
    for node in &testnet.nodes {
        assert_eq!(node.governance.runtime().protocol_version, 2);
        assert_eq!(node.governance.runtime().wasm_code_hash, hash_v2);
        assert_eq!(node.governance.runtime().wasm_bytecode, wasm_v2);
        assert_eq!(node.governance.runtime().activated_at_height, 25);
        assert_eq!(
            node.restart_count, 0,
            "validator must not be manually restarted during hot WASM upgrade"
        );
        assert!(node.finality.commit_rule.finalized_count() >= 30);
    }
}

#[test]
fn b9_governance_vector_fixture_matches_deterministic_rules() {
    let chain_id = ChainId::new("aura-b9-vectors").expect("valid chain id");
    let mut keys: Vec<(Address, BlsSecretKey, BlsPublicKey)> = Vec::new();
    let mut pubkeys = BTreeMap::new();
    let mut genesis_validators = Vec::new();

    for i in 0..4u8 {
        let mut seed = [0u8; 32];
        seed[0] = 0xB9;
        seed[1] = 0x50 + i;
        let sk = BlsSecretKey::from_seed(&seed);
        let pk = sk.public_key();
        let addr = address_from_public_key(&pk);
        pubkeys.insert(addr, pk.clone());
        genesis_validators.push(Validator {
            address: addr,
            stake: 10_000_000,
            status: ValidatorStatus::Active,
            epoch: 0,
        });
        keys.push((addr, sk, pk));
    }
    let vset = ValidatorSet::new(genesis_validators, 100);
    let (genesis_wasm, _) = build_runtime_wasm_artifact(1, "vectors-genesis");

    let mut count = 0usize;
    for raw in GOV_VECTORS.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = BTreeMap::new();
        for token in line.split_whitespace() {
            let (k, v) = token.split_once('=').expect("key=value");
            fields.insert(k, v);
        }

        let yes: u128 = fields["yes"].parse().unwrap();
        let no: u128 = fields["no"].parse().unwrap();
        let abstain: u128 = fields["abstain"].parse().unwrap();
        let veto: u128 = fields["veto"].parse().unwrap();
        let submit_h: u64 = fields["submit_h"].parse().unwrap();
        let tally_h: u64 = fields["tally_h"].parse().unwrap();
        let activation_h: u64 = fields["activation_h"].parse().unwrap();
        let paused: u8 = fields["paused"].parse().unwrap();
        let expected_status = fields["expected_status"];
        let earliest_exec_h: u64 = fields["earliest_exec_h"].parse().unwrap();
        let deposit_burned: u8 = fields["deposit_burned"].parse().unwrap();

        let mut gov = GovernanceEngine::new(
            chain_id.clone(),
            GovernanceParams::default(),
            pubkeys.clone(),
            genesis_wasm.clone(),
        )
        .expect("gov engine");

        let (wasm_v2, hash_v2) = build_runtime_wasm_artifact(2, fields["case_id"]);
        let prop_id = gov
            .submit_proposal(
                keys[0].0,
                DEFAULT_MIN_PROPOSER_DEPOSIT,
                submit_h,
                ProposalPayload::RuntimeWasmUpgrade {
                    from_version: 1,
                    to_version: 2,
                    wasm_bytecode: wasm_v2,
                    wasm_code_hash: hash_v2,
                    activation_height: activation_h,
                },
            )
            .expect("submit proposal");

        // Map the 10M-per-validator stakes to the 4 validators.
        let mut idx = 0usize;
        for (amount, choice) in [
            (yes, GovernanceVoteChoice::Yes),
            (no, GovernanceVoteChoice::No),
            (abstain, GovernanceVoteChoice::Abstain),
            (veto, GovernanceVoteChoice::NoWithVeto),
        ] {
            let num_vals = (amount / 10_000_000) as usize;
            for _ in 0..num_vals {
                let (addr, sk, _) = &keys[idx];
                let vote =
                    GovernanceVote::sign(sk, *addr, &chain_id, prop_id, choice, submit_h + 1);
                gov.cast_vote(vote, &vset).expect("cast vote");
                idx += 1;
            }
        }

        let status = gov
            .finalize_voting(prop_id, tally_h, &vset)
            .expect("finalize voting");
        assert_eq!(
            status.as_str(),
            expected_status,
            "case {}",
            fields["case_id"]
        );

        if let ProposalStatus::Timelocked {
            earliest_execution_height,
        } = status
        {
            assert_eq!(earliest_execution_height, earliest_exec_h);
            if paused == 1 {
                let pause_hash = blake3_hash(fields["case_id"].as_bytes());
                let qc = make_vote_qc(&chain_id, &keys, 3, 1, 1, pause_hash);
                gov.trigger_emergency_pause(earliest_exec_h, "vector-pause".to_string(), &qc)
                    .expect("pause");
                assert!(matches!(
                    gov.execute_proposal(prop_id, earliest_exec_h),
                    Err(GovernanceError::EmergencyPaused { .. })
                ));
            } else {
                gov.execute_proposal(prop_id, earliest_exec_h)
                    .expect("execute timelocked");
                assert_eq!(gov.runtime().protocol_version, 2);
            }
        }

        if deposit_burned == 1 {
            assert_eq!(gov.burned_deposits(), DEFAULT_MIN_PROPOSER_DEPOSIT);
        } else {
            assert_eq!(gov.burned_deposits(), 0);
        }

        count += 1;
    }

    assert_eq!(count, 64, "expected 64 B9 governance vectors");
}
