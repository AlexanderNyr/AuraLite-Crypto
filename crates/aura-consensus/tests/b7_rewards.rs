//! B7 integration tests — Consensus rewards, transaction fee burn, stake
//! delegation, validator commission, 100,000-block conservation invariant
//! (`sum(balances) + burned == issued` within 1 unit), and 64-vector fixture
//! verification.

use std::collections::BTreeMap;

use aura_consensus::rewards::{
    DEFAULT_PROPOSER_BONUS_BPS, RewardParams, RewardsEngine, ValidatorRewardConfig,
};
use aura_types::Address;

const FIXTURE: &str = include_str!("fixtures/b7_rewards.txt");

#[test]
fn b7_hundred_thousand_blocks_conservation_invariant_within_one_unit() {
    let v0 = Address::from_bytes([0x01; 20]);
    let v1 = Address::from_bytes([0x02; 20]);
    let v2 = Address::from_bytes([0x03; 20]);
    let v3 = Address::from_bytes([0x04; 20]);
    let d0 = Address::from_bytes([0x21; 20]);
    let d1 = Address::from_bytes([0x22; 20]);
    let d2 = Address::from_bytes([0x23; 20]);
    let fee_payer = Address::from_bytes([0xFA; 20]);

    let mut cfg0 = ValidatorRewardConfig::new(v0, 15_000_000, 1_000).unwrap();
    cfg0.delegations.insert(d0, 5_000_000);
    cfg0.delegations.insert(d1, 10_000_000);

    let mut cfg1 = ValidatorRewardConfig::new(v1, 25_000_000, 750).unwrap();
    cfg1.delegations.insert(d1, 15_000_000);
    cfg1.delegations.insert(d2, 20_000_000);

    let mut cfg2 = ValidatorRewardConfig::new(v2, 35_000_000, 1_500).unwrap();
    cfg2.delegations.insert(d0, 12_000_000);

    let cfg3 = ValidatorRewardConfig::new(v3, 45_000_000, 500).unwrap();

    let mut extra = BTreeMap::new();
    extra.insert(fee_payer, 10_000_000_000);

    let validators = [v0, v1, v2, v3];
    let mut engine =
        RewardsEngine::new(RewardParams::default(), vec![cfg0, cfg1, cfg2, cfg3], extra).unwrap();

    for height in 1..=100_000u64 {
        let proposer = validators[(height as usize) % validators.len()];
        let tx_fees = 500 + (height % 1_234);
        engine
            .apply_block_rewards(height, proposer, tx_fees, Some(fee_payer))
            .unwrap();
    }

    assert_eq!(engine.blocks_processed(), 100_000);
    engine.verify_sum_invariant().unwrap();
    let accounted = engine.sum_balances() + engine.total_burned();
    let issued = engine.total_issued();
    assert!(
        accounted.abs_diff(issued) <= 1,
        "sum(balances) + burned ({accounted}) != issued ({issued})"
    );
    assert_eq!(accounted, issued);
}

#[test]
fn b7_rewards_vector_fixture_matches_engine() {
    let v0 = Address::from_bytes([0x0A; 20]);
    let d0 = Address::from_bytes([0x0B; 20]);
    let payer = Address::from_bytes([0x0C; 20]);

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

        let base_reward: u64 = parts[0].parse().unwrap();
        let tx_fees: u64 = parts[1].parse().unwrap();
        let fee_burn_bps: u16 = parts[2].parse().unwrap();
        let commission_bps: u16 = parts[3].parse().unwrap();
        let self_stake: u64 = parts[4].parse().unwrap();
        let delegated_stake: u64 = parts[5].parse().unwrap();
        let expected_burned: u64 = parts[6].parse().unwrap();
        let expected_distributable: u64 = parts[7].parse().unwrap();
        let expected_val_gain: u128 = parts[8].parse().unwrap();
        let expected_del_gain: u128 = parts[9].parse().unwrap();

        let params = RewardParams {
            base_block_reward: base_reward,
            fee_burn_bps,
            proposer_bonus_bps: DEFAULT_PROPOSER_BONUS_BPS,
        };
        let mut cfg = ValidatorRewardConfig::new(v0, self_stake, commission_bps).unwrap();
        cfg.delegations.insert(d0, delegated_stake);

        let mut extra = BTreeMap::new();
        extra.insert(payer, 1_000_000_000);

        let mut engine = RewardsEngine::new(params, vec![cfg], extra).unwrap();
        let receipt = engine
            .apply_block_rewards(1, v0, tx_fees, Some(payer))
            .unwrap();

        assert_eq!(receipt.burned_fees, expected_burned);
        assert_eq!(receipt.distributed_reward, expected_distributable);
        assert_eq!(
            engine.balance_of(&v0) - (self_stake as u128),
            expected_val_gain
        );
        assert_eq!(
            engine.balance_of(&d0) - (delegated_stake as u128),
            expected_del_gain
        );
        engine.verify_sum_invariant().unwrap();
        count += 1;
    }
    assert_eq!(count, 64);
}
