#!/usr/bin/env python3
"""Six B3 negative controls for finality, view changes, and nil blocks."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from nc_rust import run_controls

FINALITY = "crates/aura-consensus/src/finality.rs"
CASES = [
    (
        "NC-1 two QCs must not satisfy the 3-chain commit rule",
        FINALITY,
        '''            let Some(grandchildren) = self.children.get(child_hash) else {
                continue;
            };''',
        '''            let Some(grandchildren) = self.children.get(child_hash) else {
                return true;
            };''',
        "finality::tests::two_blocks_not_committed",
    ),
    (
        "NC-2 a same-view 3-chain must commit its first block",
        FINALITY,
        '''                if self.qcs.contains_key(grandchild_hash) && qc_b.view == qc_c.view {
                    return true;
                }''',
        '''                if self.qcs.contains_key(grandchild_hash) && qc_b.view == qc_c.view {
                    return false;
                }''',
        "finality::tests::three_chain_commit",
    ),
    (
        "NC-3 timeout view change must advance the view",
        FINALITY,
        "        self.view_change.advance_view();",
        "        let _ = &self.view_change;",
        "finality::tests::view_change_on_timeout",
    ),
    (
        "NC-4 round change must advance the round",
        FINALITY,
        "        self.view_change.advance_round();",
        "        let _ = &self.view_change;",
        "finality::tests::round_change",
    ),
    (
        "NC-5 block hashes must include transaction contents",
        FINALITY,
        "            buf.extend_from_slice(blake3_hash(tx).as_bytes());",
        "            let _ = tx;",
        "finality::tests::block_hash_changes_with_content",
    ),
    (
        "NC-6 empty-transaction blocks must be identified as nil",
        FINALITY,
        "        self.transactions.is_empty()",
        "        false",
        "finality::tests::nil_block_has_no_transactions",
    ),
]

if __name__ == "__main__":
    raise SystemExit(run_controls("B3", CASES))
