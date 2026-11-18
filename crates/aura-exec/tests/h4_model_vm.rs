//! H4 model-based VM tests (docs/h4-property-plan.md §2, D-H4-2).
//!
//! A reference model of the transfer semantics in `detcheck::run` predicts
//! committed/refused/gas for generated scenarios; the production runner must
//! match exactly. The node label must not influence execution at all.
//! `tools/nc_h4.py` NC-4 makes the transfer path refuse amount 7 and this
//! model must notice (`H4: model/VM divergence`).

use aura_exec::detcheck::{self, Op, Prng, Step, X7_ACCOUNTS, X7_START_BALANCE, X7_TRANSFER_GAS};

/// Transfer-only scenario: distinct accounts, amounts 1..=100_000, with a
/// guaranteed amount-7 step so the NC-4 mutation trigger fires.
fn transfer_scenario(seed: u64, steps: usize) -> Vec<Step> {
    let mut prng = Prng::new(seed);
    let mut out = Vec::with_capacity(steps);
    for index in 0..steps {
        let amount = if index == 0 {
            7u128
        } else {
            (1 + prng.below(100_000)) as u128
        };
        let from = prng.below(X7_ACCOUNTS as u64) as usize;
        let mut to = prng.below(X7_ACCOUNTS as u64) as usize;
        if to == from {
            to = (to + 1) % X7_ACCOUNTS;
        }
        out.push(Step {
            index,
            op: Op::Transfer {
                from: detcheck::account_address(from),
                to: detcheck::account_address(to),
                amount,
            },
        });
    }
    out
}

/// Independent model of `Node::transfer` bookkeeping: accept = debit+credit+
/// nonce bump commit together; overdraft = refused with gas still billed.
fn model(steps: &[Step]) -> (usize, usize, u64) {
    let mut balances = [X7_START_BALANCE; X7_ACCOUNTS];
    let mut committed = 0usize;
    let mut refused = 0usize;
    for step in steps {
        let (from, to, amount) = match step.op {
            Op::Transfer { from, to, amount } => (from, to, amount),
            _ => panic!("h4 model only covers transfer scenarios"),
        };
        let from_index = from[19] as usize;
        let to_index = to[19] as usize;
        if balances[from_index] >= amount {
            balances[from_index] -= amount;
            balances[to_index] += amount;
            committed += 1;
        } else {
            refused += 1;
        }
    }
    (committed, refused, (steps.len() as u64) * X7_TRANSFER_GAS)
}

#[test]
fn h4_model_vm() {
    for seed in [20261180u64, 7, 42, 0x9E37_79B9_7F4A_7C15] {
        let steps = transfer_scenario(seed, 64);
        let (committed, refused, gas) = model(&steps);
        assert_eq!(committed + refused, steps.len());
        let report = detcheck::run(&steps, 0);
        assert_eq!(
            report.committed, committed,
            "H4: model/VM divergence (seed {seed})"
        );
        assert_eq!(
            report.refused, refused,
            "H4: model/VM divergence (seed {seed})"
        );
        assert_eq!(
            report.gas_total, gas,
            "H4: model/VM divergence (seed {seed})"
        );
        assert_eq!(
            report.committed + report.refused + report.failed,
            steps.len(),
            "H4: model/VM divergence (seed {seed})"
        );

        // Node-label independence: the label is not an input.
        let other = detcheck::run(&steps, 5);
        assert_eq!(
            other.digest, report.digest,
            "H4: model/VM divergence (seed {seed})"
        );
        assert_eq!(
            other.state_root, report.state_root,
            "H4: model/VM divergence (seed {seed})"
        );
        assert_eq!(
            other.tree_root, report.tree_root,
            "H4: model/VM divergence (seed {seed})"
        );
        assert_eq!(
            other.gas_total, report.gas_total,
            "H4: model/VM divergence (seed {seed})"
        );
    }

    // A targeted scenario of one amount-7 transfer: the model predicts commit.
    let steps = vec![Step {
        index: 0,
        op: Op::Transfer {
            from: detcheck::account_address(0),
            to: detcheck::account_address(1),
            amount: 7,
        },
    }];
    let (committed, refused, _) = model(&steps);
    assert_eq!((committed, refused), (1, 0));
    let report = detcheck::run(&steps, 0);
    assert_eq!(report.committed, 1, "H4: model/VM divergence");
    assert_eq!(report.refused, 0, "H4: model/VM divergence");
}
