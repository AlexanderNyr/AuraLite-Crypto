//! B5 VRF leader election + N1/N2 consensus wiring integration gate.

use std::collections::BTreeMap;

use aura_consensus::bls::{BlsSecretKey, address_from_public_key};
use aura_consensus::qc::{Vote, VoteType};
use aura_consensus::vrf_leader::{
    FourValidatorNetwork, ProposalEnvelope, VoteEnvelope, VrfError, VrfInput, VrfLeaderElection,
    VrfValidatorEntry, prove_vrf,
};
use aura_p2p::gossipsub::GossipTopic;
use aura_types::{Address, Domain, Hash};

const CHAIN: &str = "auralite-devnet-1";

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

#[test]
fn ten_thousand_rounds_stake_weighted_vrf_distribution_and_public_verification() {
    let (sk1, v1) = make_validator(11, 1_000_000); // 10%
    let (sk2, v2) = make_validator(22, 2_000_000); // 20%
    let (sk3, v3) = make_validator(33, 3_000_000); // 30%
    let (sk4, v4) = make_validator(44, 4_000_000); // 40%

    let mut keys_by_addr = BTreeMap::new();
    keys_by_addr.insert(v1.address, sk1);
    keys_by_addr.insert(v2.address, sk2);
    keys_by_addr.insert(v3.address, sk3);
    keys_by_addr.insert(v4.address, sk4);

    let a1 = v1.address;
    let a2 = v2.address;
    let a3 = v3.address;
    let a4 = v4.address;

    let election = VrfLeaderElection::new(vec![v1, v2, v3, v4]).expect("election table valid");
    assert_eq!(election.total_stake(), 10_000_000);

    const ROUNDS: u64 = 10_000;
    let epoch_rand = Hash::from_bytes([0x3C; 32]);
    let mut counts: BTreeMap<Address, u64> = BTreeMap::new();

    for round in 0..ROUNDS {
        let input = VrfInput::new(CHAIN, 1, epoch_rand, 0, round);
        let leader = election.select_leader(&input).expect("leader selected");
        *counts.entry(leader).or_insert(0) += 1;

        // Publicly prove & verify every 200th round's VRF leader claim.
        if round % 200 == 0 {
            let sk = keys_by_addr.get(&leader).expect("leader key exists");
            let claim = election
                .prove_leader_claim(sk, &input)
                .expect("leader claim valid");
            election
                .verify_leader_claim(&claim, &input)
                .expect("public verification succeeds");
        }
    }

    let c1 = *counts.get(&a1).unwrap_or(&0) as i64;
    let c2 = *counts.get(&a2).unwrap_or(&0) as i64;
    let c3 = *counts.get(&a3).unwrap_or(&0) as i64;
    let c4 = *counts.get(&a4).unwrap_or(&0) as i64;
    assert_eq!(c1 + c2 + c3 + c4, ROUNDS as i64);
    assert!(
        (c1 - 1_000).abs() <= 200,
        "10% validator count {c1} out of CI"
    );
    assert!(
        (c2 - 2_000).abs() <= 200,
        "20% validator count {c2} out of CI"
    );
    assert!(
        (c3 - 3_000).abs() <= 200,
        "30% validator count {c3} out of CI"
    );
    assert!(
        (c4 - 4_000).abs() <= 200,
        "40% validator count {c4} out of CI"
    );
}

#[test]
fn four_validators_over_n1_n2_finalize_100_blocks_and_survive_one_validator_outage() {
    let mut net = FourValidatorNetwork::new(CHAIN, 100);

    // First 50 rounds with all 4 validators online; next 50 rounds with validator 3 down.
    for round in 0..100u64 {
        if round == 50 {
            net.set_online(3, false);
        }
        let qc = net
            .run_round(
                1_700_000_000_000 + round * 100,
                vec![format!("b5-tx-{round}").into_bytes()],
            )
            .expect("3-of-4 quorum reached over N2 gossip");
        assert!(qc.signatures.len() >= 3);
        qc.verify(CHAIN, Domain::Vote)
            .expect("emitted QC verifies cryptographically");
    }

    let finalized_0 = net.nodes[0].finality.commit_rule.finalized_count();
    let tip_0 = net.nodes[0].finality.tip.block_hash;
    assert!(
        finalized_0 >= 95,
        "expected >= 95 finalized blocks across 100 rounds, got {finalized_0}"
    );
    for idx in 0..3 {
        assert_eq!(
            net.nodes[idx].finality.commit_rule.finalized_count(),
            finalized_0,
            "validator {idx} finalized count matches validator 0"
        );
        assert_eq!(
            net.nodes[idx].finality.tip.block_hash, tip_0,
            "validator {idx} tip block hash matches validator 0"
        );
    }
}

#[test]
fn networked_validator_rejects_forged_vrf_leader_and_forged_bls_vote() {
    let mut net = FourValidatorNetwork::new(CHAIN, 100);
    let input = net.nodes[0].current_vrf_input();
    let elected = net.nodes[0]
        .leader_election
        .select_leader(&input)
        .expect("leader selected");

    // Pick a validator that is NOT the VRF leader for round 0.
    let impostor_idx = (0..4)
        .find(|&i| net.nodes[i].address != elected)
        .expect("non-leader exists");
    let receiver_idx = (0..4)
        .find(|&i| i != impostor_idx)
        .expect("receiver exists");

    let impostor_peer = net.nodes[impostor_idx].peer_id();
    let impostor_proof =
        prove_vrf(&net.nodes[impostor_idx].bls_secret, &input).expect("impostor VRF proof");
    let fake_block = net.nodes[impostor_idx]
        .finality
        .propose_at(1_700_000_000_100, vec![b"forged-proposal".to_vec()]);
    let bad_proposal = ProposalEnvelope {
        epoch: input.epoch,
        epoch_randomness: input.epoch_randomness,
        block: fake_block.clone(),
        vrf_proof: impostor_proof,
    };
    let (proposal_msg, _) = net.nodes[impostor_idx]
        .gossip
        .publish(GossipTopic::Proposal, bad_proposal.to_bytes())
        .expect("gossip publish");

    let res =
        net.nodes[receiver_idx].on_proposal_gossip(impostor_peer, &proposal_msg, 1_700_000_000_100);
    assert!(matches!(res, Err(VrfError::NotElectedLeader { .. })));

    // Also test a forged BLS vote (signed with impostor's BLS key for wrong block hash).
    let bad_sig =
        net.nodes[impostor_idx]
            .bls_secret
            .sign(CHAIN, Domain::Vote, b"not-a-valid-vote-payload");
    let bad_vote_env = VoteEnvelope {
        public_key: net.nodes[impostor_idx].bls_public.clone(),
        vote: Vote {
            view: 0,
            round: 0,
            block_hash: fake_block.block_hash,
            validator: net.nodes[impostor_idx].address,
            vote_type: VoteType::Precommit,
            signature: bad_sig,
        },
    };
    let (vote_msg, _) = net.nodes[impostor_idx]
        .gossip
        .publish(GossipTopic::Vote, bad_vote_env.to_bytes())
        .expect("gossip publish vote");
    let vote_res =
        net.nodes[receiver_idx].on_vote_gossip(impostor_peer, &vote_msg, 1_700_000_000_200);
    assert_eq!(vote_res, Err(VrfError::InvalidConsensusVote));
}
