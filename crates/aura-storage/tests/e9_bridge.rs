//! E9 gate (ECOSYSTEM_PLAN E9, decision D-E4):
//! **MITM forged header → reject**; **OS client checks finalized
//! checkpoint**; **emulator lane green** (the pinned scenario lane; the
//! `aurad` CI lane runs via `make emulator-lane`).
//!
//! The step log of [`run_e9_scenario`] is pinned line by line and the
//! independent Python model `tools/e9_bridge_check.py` must reproduce
//! the same digest.

use aura_crypto::sign_raw;
use aura_storage::bridge::{
    Bridge, Commit, E9_DAILY_LIMIT, E9_PER_TX_LIMIT, Header, Kind, Msg, OsClient, Validator,
    attacker_seed, required_power, run_e9_scenario, scenario_digest, total_power, validator_seed,
    verify_commit,
};

const SCENARIO_DIGEST: &str = "039d498ba6fcd17e7640094492283a94be2c3447f8ebec89f9306eb438339580";
const AGGREGATE_DIGEST: &str = "4f8d9af363ab88fb7a6ce0ef6af52facb886f5e123ca0a128d1693e06d575e4f";

const GOLDEN_STEPS: [&str; 25] = [
    "header|1|ok",
    "header|2|ok",
    "header|3|ok",
    "header|4|ok",
    "header|5|ok",
    "header|5|err|E9: header not newer (5/5)",
    "header|6|err|E9: commit lacks quorum (20/27)",
    "forged|6|err|E9: forged header signature (99)",
    "forged|6|err|E9: forged header signature (1)",
    "chain|7|err|E9: header does not chain (7)",
    "deposit|1|500000000000000000000|ok",
    "deposit|2|400000000000000000000|ok",
    "withdraw|3|100000000000000000000|ok",
    "deposit|1|500000000000000000000|err|E9: message already processed (1)",
    "deposit|4|5000000000000000000000|err|E9: transfer exceeds per-tx limit (5000000000000000000000/1000000000000000000000)",
    "deposit|5|400000000000000000000|err|E9: transfer exceeds daily limit (400000000000000000000/200000000000000000000)",
    "deposit|8|10000000000000000000|err|E9: message root mismatch (3)",
    "pause|ok",
    "deposit|6|10000000000000000000|err|E9: bridge is paused",
    "unpause|ok",
    "os|5|ok",
    "os|6|err|E9: checkpoint not finalized (6)",
    "day|2|ok",
    "withdraw|7|900000000000000000000|err|E9: withdraw exceeds reserve (900000000000000000000/800000000000000000000)",
    "invariant|locked=800000000000000000000|minted=800000000000000000000",
];

fn validators() -> Vec<Validator> {
    (0..4u32)
        .map(|id| Validator {
            id,
            power: 10,
            pubkey: aura_crypto::ed25519_public_key(&validator_seed(id)),
        })
        .collect()
}

fn header_at(height: u64, parent: [u8; 32]) -> Header {
    Header {
        height,
        parent,
        root: aura_state::sha256_bytes(format!("e9-root-{height}").as_bytes()).as_bytes()[..32]
            .try_into()
            .unwrap(),
        time: height * 10,
    }
}

fn commit_of(signers: &[u32], header: &Header) -> Commit {
    let hash = header.hash();
    let mut commit: Commit = signers
        .iter()
        .map(|id| (*id, sign_raw(&validator_seed(*id), &hash)))
        .collect();
    commit.sort_by_key(|(id, _)| *id);
    commit
}

// ------------------------------------------------------- DoD criteria

#[test]
fn e9_gate_mitm_forged_header_rejected() {
    let vs = validators();
    let genesis = header_at(0, [0u8; 32]);
    let h1 = header_at(1, genesis.hash());
    let h2 = header_at(2, h1.hash());

    // Attack A: the MITM appends its own signature to an honest commit.
    let mut forged = commit_of(&[0, 1, 2], &h1);
    forged.push((99, sign_raw(&attacker_seed(), &h1.hash())));
    assert_eq!(
        verify_commit(&vs, &h1, &forged).unwrap_err().to_string(),
        "E9: forged header signature (99)"
    );

    // Attack B: a bit-flipped signature of a real validator.
    let mut flipped = commit_of(&[0, 1, 2], &h2);
    flipped[1].1[0] ^= 0x01;
    assert_eq!(
        verify_commit(&vs, &h2, &flipped).unwrap_err().to_string(),
        "E9: forged header signature (1)"
    );

    // Attack C: a wholly attacker-signed header (all keys foreign).
    let attacker_commit: Commit = vec![(99, sign_raw(&attacker_seed(), &h2.hash()))];
    assert_eq!(
        verify_commit(&vs, &h2, &attacker_commit)
            .unwrap_err()
            .to_string(),
        "E9: forged header signature (99)"
    );

    // A duplicate signer is forgery too (double vote in one commit).
    let mut doubled = commit_of(&[0, 1, 2], &h2);
    doubled.push((1, sign_raw(&validator_seed(1), &h2.hash())));
    assert_eq!(
        verify_commit(&vs, &h2, &doubled).unwrap_err().to_string(),
        "E9: forged header signature (1)"
    );
}

#[test]
fn e9_gate_os_client_finalized_checkpoint() {
    let vs = validators();
    let genesis = header_at(0, [0u8; 32]);
    let mut bridge = Bridge::new(vs.clone(), genesis.clone(), E9_PER_TX_LIMIT, E9_DAILY_LIMIT);
    let mut parent = genesis.hash();
    let mut last = genesis.clone();
    for height in 1..=5u64 {
        let h = header_at(height, parent);
        bridge
            .update_header(&h, &commit_of(&[0, 1, 2], &h))
            .unwrap();
        parent = h.hash();
        last = h.clone();
    }

    // The OS client builds its checkpoint only from a verified commit.
    let os = bridge.os_client();
    assert_eq!(os.checkpoint_height, 5);
    OsClient::verify_checkpoint(&vs, &last, &commit_of(&[0, 1, 2], &last)).unwrap();
    // Forged checkpoint → reject at the OS boundary too.
    let mut forged = commit_of(&[0, 1, 2], &last);
    forged.push((99, sign_raw(&attacker_seed(), &last.hash())));
    assert_eq!(
        OsClient::verify_checkpoint(&vs, &last, &forged)
            .unwrap_err()
            .to_string(),
        "E9: forged header signature (99)"
    );

    // OS client checks the finalized checkpoint: 5 ok, 6 refused.
    os.accept(5).unwrap();
    assert_eq!(
        os.accept(6).unwrap_err().to_string(),
        "E9: checkpoint not finalized (6)"
    );
    // Root check at the checkpoint boundary.
    os.check_root(5, last.root).unwrap();
    assert_eq!(
        os.check_root(5, [9u8; 32]).unwrap_err().to_string(),
        "E9: message root mismatch (5)"
    );
}

#[test]
fn e9_gate_emulator_lane_green() {
    let run = run_e9_scenario();
    assert_eq!(run.steps, GOLDEN_STEPS, "exact scenario step log");
    assert_eq!(scenario_digest(&run.steps), SCENARIO_DIGEST);
    assert_eq!(run.forged_rejected, 2);
    assert_eq!(run.finalized_height, 5);
    assert_eq!(run.locked, 800_000_000_000_000_000_000);
    assert_eq!(run.minted, 800_000_000_000_000_000_000);
    assert_eq!(run.lane, "green");
    let agg = aura_state::sha256_bytes(
        format!(
            "E9|steps=25|scenario_digest={SCENARIO_DIGEST}|forged=2|\
             finalized=5|invariant=800000000000000000000|lane=green"
        )
        .as_bytes(),
    )
    .to_hex();
    assert_eq!(agg, AGGREGATE_DIGEST);
}

// -------------------------------------------------------- error literals

#[test]
fn e9_nc_literals() {
    // The six mutation-control targets, asserted verbatim.
    let run = run_e9_scenario();
    for expected in [
        "forged|6|err|E9: forged header signature (99)",
        "header|6|err|E9: commit lacks quorum (20/27)",
        "chain|7|err|E9: header does not chain (7)",
        "os|6|err|E9: checkpoint not finalized (6)",
        "deposit|1|500000000000000000000|err|E9: message already processed (1)",
        "deposit|6|10000000000000000000|err|E9: bridge is paused",
    ] {
        assert!(
            run.steps.iter().any(|s| s == expected),
            "missing {expected}"
        );
    }
}

#[test]
fn e9_named_messages() {
    let vs = validators();
    let genesis = header_at(0, [0u8; 32]);
    let mut bridge = Bridge::new(vs, genesis.clone(), E9_PER_TX_LIMIT, E9_DAILY_LIMIT);
    let h1 = header_at(1, genesis.hash());
    bridge
        .update_header(&h1, &commit_of(&[0, 1, 2], &h1))
        .unwrap();

    // Zero amount.
    let zero = Msg {
        id: 1,
        kind: Kind::Deposit,
        amount: 0,
    };
    assert_eq!(
        bridge.submit(&zero, 1, h1.root).unwrap_err().to_string(),
        "E9: amount must be positive"
    );
    // Not finalized (height beyond the checkpoint).
    let m = Msg {
        id: 2,
        kind: Kind::Deposit,
        amount: 1,
    };
    assert_eq!(
        bridge.submit(&m, 2, h1.root).unwrap_err().to_string(),
        "E9: checkpoint not finalized (2)"
    );
    // Not newer (replaying an old header).
    assert_eq!(
        bridge
            .update_header(&h1, &commit_of(&[0, 1, 2], &h1))
            .unwrap_err()
            .to_string(),
        "E9: header not newer (1/1)"
    );
}

// --------------------------------------------------------- metamorphisms

#[test]
fn e9_meta_commit_order_independent() {
    let vs = validators();
    let h = header_at(3, [3u8; 32]);
    let mut a = commit_of(&[0, 1, 2], &h);
    let mut b = a.clone();
    b.reverse();
    assert_eq!(verify_commit(&vs, &h, &a), Ok(()));
    assert_eq!(verify_commit(&vs, &h, &b), Ok(()));
    a.push((1, sign_raw(&validator_seed(1), &h.hash())));
    assert_eq!(
        verify_commit(&vs, &h, &a).unwrap_err().to_string(),
        "E9: forged header signature (1)"
    );
}

#[test]
fn e9_meta_limit_boundaries() {
    let vs = validators();
    let genesis = header_at(0, [0u8; 32]);
    let mut bridge = Bridge::new(vs, genesis.clone(), E9_PER_TX_LIMIT, E9_DAILY_LIMIT);
    let h1 = header_at(1, genesis.hash());
    bridge
        .update_header(&h1, &commit_of(&[0, 1, 2], &h1))
        .unwrap();

    // Exactly at the per-tx limit passes; one wei more is refused.
    let exact = Msg {
        id: 1,
        kind: Kind::Deposit,
        amount: E9_PER_TX_LIMIT,
    };
    bridge.submit(&exact, 1, h1.root).unwrap();
    let over = Msg {
        id: 2,
        kind: Kind::Deposit,
        amount: E9_PER_TX_LIMIT + 1,
    };
    assert_eq!(
        bridge.submit(&over, 1, h1.root).unwrap_err().to_string(),
        format!(
            "E9: transfer exceeds per-tx limit ({}/{})",
            E9_PER_TX_LIMIT + 1,
            E9_PER_TX_LIMIT
        )
    );
    // The daily window: 1000 used, 200 remaining, 200 passes, 201 refused.
    let rest = Msg {
        id: 3,
        kind: Kind::Deposit,
        amount: 200_000_000_000_000_000_000,
    };
    bridge.submit(&rest, 1, h1.root).unwrap();
    let over_day = Msg {
        id: 4,
        kind: Kind::Deposit,
        amount: 1,
    };
    assert_eq!(
        bridge
            .submit(&over_day, 1, h1.root)
            .unwrap_err()
            .to_string(),
        "E9: transfer exceeds daily limit (1/0)"
    );
    // Day 2 resets the window.
    bridge.advance_day(2);
    let next = Msg {
        id: 5,
        kind: Kind::Deposit,
        amount: 1,
    };
    bridge.submit(&next, 1, h1.root).unwrap();
}

#[test]
fn e9_meta_reserve_invariant_and_determinism() {
    let a = run_e9_scenario();
    let b = run_e9_scenario();
    assert_eq!(a.steps, b.steps);
    assert_eq!(a.locked, a.minted, "locked == minted");
    assert_eq!(a.locked, b.locked);
    // Quorum maths: 4 validators x power 10 -> 27 of 40 required.
    let vs = validators();
    assert_eq!(total_power(&vs), 40);
    assert_eq!(required_power(&vs), 27);
}
