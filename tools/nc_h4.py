#!/usr/bin/env python3
"""H4 negative controls (docs/h4-property-plan.md §5).

Each control injects a bug (one source mutation at a unique anchor,
restored afterwards) and requires the H4 property tests to go red with the
named message. NC-1 is the named control (`injected bug -> property finds
it`).
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECT = {
    "H4-NC1": "H4: balance conservation violated",
    "H4-NC2": "H4: canonical roundtrip violated",
    "H4-NC3": "H4: signature property violated",
    "H4-NC4": "H4: model/VM divergence",
    "H4-NC5": "H4: tree differential mismatch",
    "H4-NC6": "H4: credit/debit algebra violated",
}

CONTROLS = [
    dict(
        id="H4-NC1",
        label="injected bug -> property finds it",
        path="crates/aura-state/src/account.rs",
        anchor="    pub fn debit(&mut self, amount: Amount) -> Result<(), StateError> {",
        inject='\n        let amount = if amount == Amount::from_units(7) { Amount::from_units(6) } else { amount };',
        cmd=[
            "cargo", "test", "--locked", "-p", "aura-testkit", "--test", "h4_properties",
            "h4_balance_conservation", "--", "--exact",
        ],
    ),
    dict(
        id="H4-NC2",
        label="canonicality guard must be enforced",
        path="crates/aura-codec/src/codec.rs",
        anchor="            if varint_is_nonminimal(count, byte) {",
        replace="            if false && varint_is_nonminimal(count, byte) {",
        cmd=[
            "cargo", "test", "--locked", "-p", "aura-testkit", "--test", "h4_properties",
            "h4_canonical_roundtrip", "--", "--exact",
        ],
    ),
    dict(
        id="H4-NC3",
        label="expiry must be signed",
        path="crates/aura-tx/src/tx.rs",
        anchor="        writer.push_bytes(SIGNING_CONTEXT);\n        self.encode(&mut writer);",
        replace=(
            "        writer.push_bytes(SIGNING_CONTEXT);\n"
            "        { let mut unsigned = self.clone();"
            " unsigned.expiry = aura_types::Height::new(0); unsigned.encode(&mut writer); }"
        ),
        cmd=[
            "cargo", "test", "--locked", "-p", "aura-testkit", "--test", "h4_properties",
            "h4_signature_properties", "--", "--exact",
        ],
    ),
    dict(
        id="H4-NC4",
        label="model must predict the VM",
        path="crates/aura-exec/src/detcheck.rs",
        anchor="    fn transfer(&mut self, index: usize, from: [u8; 20], to: [u8; 20], amount: u128) {",
        inject=(
            "\n        if amount == 7 {\n"
            "            self.refused += 1;\n"
            "            self.bill(index, X7_TRANSFER_GAS, &[]);\n"
            "            self.digest = fold(\n"
            "                self.digest,\n"
            "                index,\n"
            "                2,\n"
            "                -1,\n"
            "                X7_TRANSFER_GAS,\n"
            "                &self.tree.state_root(),\n"
            "            );\n"
            "            return;\n"
            "        }"
        ),
        cmd=[
            "cargo", "test", "--locked", "-p", "aura-exec", "--test", "h4_model_vm",
            "h4_model_vm", "--", "--exact",
        ],
    ),
    dict(
        id="H4-NC5",
        label="merkle node order must hold",
        path="crates/aura-state/src/tree.rs",
        anchor="    input.extend_from_slice(left.as_bytes());\n    input.extend_from_slice(right.as_bytes());",
        replace="    input.extend_from_slice(right.as_bytes());\n    input.extend_from_slice(left.as_bytes());",
        cmd=[
            "cargo", "test", "--locked", "-p", "aura-state", "--test", "h4_tree",
            "h4_tree_differential", "--", "--exact",
        ],
    ),
    dict(
        id="H4-NC6",
        label="credit must add exactly",
        path="crates/aura-state/src/account.rs",
        anchor="    pub fn credit(&mut self, amount: Amount) -> Result<(), StateError> {",
        inject='\n        let amount = if amount == Amount::from_units(7) { Amount::from_units(8) } else { amount };',
        cmd=[
            "cargo", "test", "--locked", "-p", "aura-testkit", "--test", "h4_properties",
            "h4_credit_debit_algebra", "--", "--exact",
        ],
    ),
]


def main() -> int:
    failures = 0
    for control in CONTROLS:
        target = ROOT / control["path"]
        original = target.read_text()
        anchor = control["anchor"]
        count = original.count(anchor)
        if count != 1:
            print(f"{control['id']} SETUP-ERROR anchor-count={count} in {control['path']}")
            return 2
        if "replace" in control:
            mutated = original.replace(anchor, control["replace"], 1)
        else:
            mutated = original.replace(anchor, anchor + control["inject"], 1)
        target.write_text(mutated)
        try:
            result = subprocess.run(
                control["cmd"], cwd=ROOT, capture_output=True, text=True
            )
        finally:
            target.write_text(original)
        output = result.stdout + result.stderr
        expected = EXPECT[control["id"]]
        red = result.returncode != 0 and expected in output
        restored = target.read_text() == original
        ok = red and restored
        if not ok:
            failures += 1
        print(
            f"{control['id']} {'PASS' if ok else 'FAIL'} "
            f"(red={red} restored={restored}) [{control['label']}]"
        )
    print(f"H4 controls: {len(CONTROLS) - failures}/{len(CONTROLS)} PASS")
    if failures == 0:
        print(f"RESULT: {len(CONTROLS)}/{len(CONTROLS)} negative controls PASS")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
