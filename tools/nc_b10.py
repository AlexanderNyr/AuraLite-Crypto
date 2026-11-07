#!/usr/bin/env python3
"""Six B10 negative controls against `crates/aura-consensus/src/safety_model.rs` library unit tests."""

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

MODEL = "crates/aura-consensus/src/safety_model.rs"

CASES = [
    (
        "NC-1 remove quorum check -> model finds a counterexample and reddens f < n/3 safety gate",
        MODEL,
        "        if self.enforce_quorum_check {\n            quorum_threshold(self.total_validators)\n        } else {\n            1\n        }",
        "        1",
        "safety_model::tests::model_proves_safety_for_f_less_than_n_over_3",
    ),
    (
        "NC-2 model checker must emit a counterexample when two conflicting branches both reach quorum",
        MODEL,
        "if votes_a >= q && votes_b >= q {",
        "if false {",
        "safety_model::tests::model_finds_counterexample_when_quorum_check_is_removed_or_f_reaches_one_third",
    ),
    (
        "NC-3 100-node simulation must reject adversarial forks below 2f+1 quorum",
        MODEL,
        "if adversarial_votes < q {",
        "if false {",
        "safety_model::tests::hundred_node_simulation_preserves_safety_with_33_byzantine_validators",
    ),
    (
        "NC-4 long-range analysis must enforce weak-subjectivity unbonding window",
        MODEL,
        "if age_blocks > scenario.unbonding_window_blocks {",
        "if false {",
        "safety_model::tests::long_range_posterior_corruption_is_rejected_by_weak_subjectivity_and_checkpoint",
    ),
    (
        "NC-5 TLA+/Apalache specification must declare AccountableSafetyInvariant",
        MODEL,
        "AccountableSafetyInvariant ==",
        "OmittedSafetyInvariant ==",
        "safety_model::tests::tla_apalache_spec_declares_invariants_and_matches_model_constants",
    ),
    (
        "NC-6 accountable safety must extract all double-signing validators in quorum intersection",
        MODEL,
        "        set_a.intersection(&set_b).copied().collect()\n    }",
        "        Vec::new()\n    }",
        "safety_model::tests::accountable_safety_identifies_at_least_f_plus_1_equivocators_on_conflicting_qcs",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B10", CASES))
