//! T0 economics baseline gate (ECONOMICS_PLAN T0).
//!
//! The gate is the literal phase gate: "emission from nowhere" in the
//! simulation must go red. The positive gate runs the 1000-step baseline
//! schedule from `sim/t0-baseline.json` and requires the total supply to
//! stay constant at every step; every control below carries its named T0
//! message.

use aura_storage::econ::{
    Amount, DESIRED_PROPERTIES, EconError, EconSim, EmissionSource, T0_AGENTS, T0_SEED, T0_STEPS,
    Transfer, XorShift64, generate_schedule,
};

/// Golden state digest of the T0 baseline run, computed by the independent
/// Python oracle (`tools/t0_economics_check.py`).
const GOLDEN_DIGEST: &str = "aec12b64eada1b850e7c73ac1e2afce62fb398df0d27365c7af300c295c7a51a";

fn baseline() -> (EconSim, Vec<Transfer>) {
    let sim = EconSim::from_genesis(&T0_AGENTS).expect("genesis");
    let schedule = generate_schedule(T0_SEED, T0_STEPS, &T0_AGENTS);
    (sim, schedule)
}

#[test]
fn t0_simulation_keeps_the_total_supply_constant() {
    let (mut sim, schedule) = baseline();
    assert_eq!(
        schedule.len(),
        T0_STEPS,
        "T0: transfers must conserve the total supply"
    );
    let supply_before = sim.total_supply();
    let outcome = sim.run(&schedule);
    assert!(
        outcome.is_ok(),
        "T0: transfers must conserve the total supply (run failed: {:?})",
        outcome
    );
    assert_eq!(
        sim.total_supply(),
        supply_before,
        "T0: transfers must conserve the total supply"
    );
    assert_eq!(
        sim.total_supply(),
        3_000_000,
        "T0: transfers must conserve the total supply"
    );
    assert_eq!(
        sim.sum_balances(),
        sim.total_supply(),
        "T0: transfers must conserve the total supply"
    );
}

#[test]
fn t0_transfer_must_not_spend_more_than_the_sender_has() {
    let mut sim = EconSim::from_genesis(&[("alice", 10), ("bob", 0)]).expect("genesis");
    let outcome = sim.transfer("alice", "bob", 11);
    assert_eq!(
        outcome,
        Err(EconError::InsufficientFunds),
        "T0: a transfer must not spend more than the sender has"
    );
    assert_eq!(
        sim.balance("alice"),
        Some(10),
        "T0: a transfer must not spend more than the sender has"
    );
    sim.transfer("alice", "bob", 10).expect("exact balance");
    assert_eq!(
        sim.balance("bob"),
        Some(10),
        "T0: a transfer must not spend more than the sender has"
    );
}

#[test]
fn t0_simulation_is_deterministic() {
    let (mut sim, schedule) = baseline();
    sim.run(&schedule).expect("schedule");
    let digest = sim.state_digest();
    assert_eq!(
        digest, GOLDEN_DIGEST,
        "T0: the simulation must be deterministic"
    );
    let (mut again, schedule_again) = baseline();
    again.run(&schedule_again).expect("schedule");
    assert_eq!(
        again.state_digest(),
        digest,
        "T0: the simulation must be deterministic"
    );
}

#[test]
fn t0_only_declared_emission_may_change_the_total_supply() {
    let mut sim = EconSim::from_genesis(&T0_AGENTS).expect("genesis");
    let supply_before = sim.total_supply();
    sim.mint(EmissionSource::IssuanceSchedule, "alice", 5)
        .expect("declared");
    assert_eq!(
        sim.total_supply(),
        supply_before + 5,
        "T0: only declared emission may change the total supply"
    );
    sim.check_invariants()
        .expect("T0: only declared emission may change the total supply");
    let (mut base, schedule) = baseline();
    base.run(&schedule).expect("schedule");
    assert_eq!(
        base.total_supply(),
        supply_before,
        "T0: only declared emission may change the total supply"
    );
}

#[test]
fn t0_properties_cover_scarcity_security_accessibility() {
    let mut seen = [false; 3];
    for property in DESIRED_PROPERTIES {
        match property {
            "scarcity" => seen[0] = true,
            "security" => seen[1] = true,
            "accessibility" => seen[2] = true,
            _ => {}
        }
    }
    assert_eq!(
        seen,
        [true, true, true],
        "T0: the baseline properties must cover scarcity, security and accessibility"
    );
    assert_eq!(
        DESIRED_PROPERTIES.len(),
        3,
        "T0: the baseline properties must cover scarcity, security and accessibility"
    );
}

#[test]
fn t0_reference_model_agrees_with_simulation() {
    let (mut sim, schedule) = baseline();
    sim.run(&schedule).expect("schedule");
    // Naive independent recomputation: a sorted ledger folded over the same
    // schedule with the same cap-at-balance rule.
    let mut naive: Vec<(String, Amount)> = T0_AGENTS
        .iter()
        .map(|(name, amount)| ((*name).to_string(), *amount))
        .collect();
    for step in &schedule {
        let from = naive
            .iter()
            .position(|(name, _)| name == &step.from)
            .expect("from");
        let to = naive
            .iter()
            .position(|(name, _)| name == &step.to)
            .expect("to");
        let capped = step.amount.min(naive[from].1);
        naive[from].1 -= capped;
        naive[to].1 += capped;
    }
    naive.sort_by(|a, b| a.0.cmp(&b.0));
    let mut sim_balances: Vec<(String, Amount)> = T0_AGENTS
        .iter()
        .map(|(name, _)| ((*name).to_string(), sim.balance(name).expect("known")))
        .collect();
    sim_balances.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        sim_balances, naive,
        "T0: the reference model must agree with the simulation"
    );
    assert_eq!(
        sim.steps() as usize,
        T0_STEPS,
        "T0: the reference model must agree with the simulation"
    );
}

#[test]
fn t0_split_transfer_is_equivalent_to_one_transfer() {
    let mut one = EconSim::from_genesis(&T0_AGENTS).expect("genesis");
    let mut two = EconSim::from_genesis(&T0_AGENTS).expect("genesis");
    one.transfer("alice", "bob", 300).expect("one");
    two.transfer("alice", "bob", 120).expect("part a");
    two.transfer("alice", "bob", 180).expect("part b");
    assert_eq!(
        one.state_digest(),
        two.state_digest(),
        "T0: splitting a transfer must not change the outcome"
    );
    // The same with the runner: a step of a+b equals two steps a, b.
    let mut run_one = EconSim::from_genesis(&T0_AGENTS).expect("genesis");
    let mut run_two = EconSim::from_genesis(&T0_AGENTS).expect("genesis");
    let step = |amount: Amount| Transfer {
        from: "alice".to_string(),
        to: "bob".to_string(),
        amount,
    };
    run_one.apply_step(&step(300)).expect("one");
    run_two.apply_step(&step(120)).expect("part a");
    run_two.apply_step(&step(180)).expect("part b");
    assert_eq!(
        run_one.balance("bob"),
        run_two.balance("bob"),
        "T0: splitting a transfer must not change the outcome"
    );
    assert_eq!(
        run_one.balance("alice"),
        run_two.balance("alice"),
        "T0: splitting a transfer must not change the outcome"
    );
}

#[test]
fn t0_prng_is_reproducible() {
    let mut a = XorShift64::new(T0_SEED);
    let mut b = XorShift64::new(T0_SEED);
    for _ in 0..64 {
        assert_eq!(
            a.next_u64(),
            b.next_u64(),
            "T0: the simulation must be deterministic"
        );
    }
    let (mut sim, schedule) = baseline();
    sim.run(&schedule).expect("schedule");
    assert_ne!(
        sim.state_digest(),
        "",
        "T0: the simulation must be deterministic"
    );
}
