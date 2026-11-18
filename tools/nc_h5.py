#!/usr/bin/env python3
"""Mutation controls for H5 (docs/h5-consensus-plan.md §5).

Each control injects ONE bug (restored after) and requires the named H5
tests to go RED with the named message - "injected bug -> the gate catches
it", and NC-1 is literally the H5 gate: when the model stops finding the
counterexample, the model gate must fail.
"""

import hashlib
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAFETY = os.path.join(ROOT, "crates/aura-consensus/src/safety_model.rs")
CHAOS = os.path.join(ROOT, "crates/aura-p2p/src/chaos.rs")
TESTS = ["-p", "aura-consensus", "--test", "h5_safety_model"]

# (name, path, anchor, replacement, test cmd, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> model must find counterexample (checker always Safe)",
        SAFETY,
        "    pub fn check(config: &BftModelConfig) -> ModelCheckResult {\n",
        "    pub fn check(config: &BftModelConfig) -> ModelCheckResult {\n"
        "        let _ = config;\n"
        "        return ModelCheckResult::Safe { states_explored: 0, min_honest_quorum_overlap: 1 };\n",
        ["cargo", "test", "--locked"] + TESTS +
        ["h5_model_safety_and_counterexample", "--", "--exact"],
        "H5: model must find a counterexample when 2f+1 is removed",
    ),
    (
        "NC-2 required_quorum weakened to f+1",
        SAFETY,
        "            quorum_threshold(self.total_validators)\n",
        "            self.byzantine_validators + 1\n",
        ["cargo", "test", "--locked"] + TESTS +
        ["h5_model_safety_and_counterexample", "--", "--exact"],
        "H5: safety must hold for f < n/3",
    ),
    (
        "NC-3 accountable-safety extraction returns no offenders",
        SAFETY,
        "pub fn extract_conflicting_qc_offenders(quorum_a: &[u64], quorum_b: &[u64]) -> Vec<u64> {\n",
        "pub fn extract_conflicting_qc_offenders(quorum_a: &[u64], quorum_b: &[u64]) -> Vec<u64> {\n"
        "        return Vec::new();\n",
        ["cargo", "test", "--locked"] + TESTS +
        ["h5_model_safety_and_counterexample", "--", "--exact"],
        "H5: accountable safety extraction must name offenders",
    ),
    (
        "NC-4 TLA threshold changed to F + 1",
        SAFETY,
        "QuorumThreshold == 2 * F + 1\n",
        "QuorumThreshold == F + 1\n",
        ["cargo", "test", "--locked"] + TESTS +
        ["h5_tla_spec_integrity", "--", "--exact"],
        "H5: TLA spec drift",
    ),
    (
        "NC-5 divergence monitor never fires",
        CHAOS,
        ") -> Result<([u8; 32], [u8; 32], usize), P2pError> {\n",
        ") -> Result<([u8; 32], [u8; 32], usize), P2pError> {\n"
        "        return Ok(([0u8; 32], [0u8; 32], 0));\n",
        ["cargo", "test", "--locked", "-p", "aura-p2p", "--test",
         "h5_jepsen", "h5_divergence_monitor_fires", "--", "--exact"],
        "H5: divergence monitor must fire",
    ),
    (
        "NC-6 chaos campaign truncated below 20 scenarios",
        CHAOS,
        "pub fn canonical_20_scenarios() -> Vec<ChaosScenarioConfig> {\n",
        "pub fn canonical_20_scenarios() -> Vec<ChaosScenarioConfig> {\n"
        "        return Vec::new();\n",
        ["cargo", "test", "--locked", "-p", "aura-p2p", "--test",
         "h5_jepsen", "h5_chaos_campaign_zero_divergence", "--", "--exact"],
        "H5: chaos campaign must cover 20 scenarios",
    ),
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def run_control(name, path, anchor, replacement, cmd, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
    compile_failed = "error[" in output and "test result" not in output
    if compile_failed:
        return False, "mutation broke compilation"
    if "test result: ok" in output:
        return False, "mutated test still passes"
    if expected in output:
        return True, "red with named message"
    return False, "red without named message"


def main():
    passed = 0
    for name, path, anchor, replacement, cmd, expected in CONTROLS:
        ok, note = run_control(name, path, anchor, replacement, cmd, expected)
        line = f"[{'PASS' if ok else 'FAIL'}] {name}: {note}"
        print(line, flush=True)
        passed += bool(ok)
    print("RESULT: %d/%d negative controls PASS" % (passed, len(CONTROLS)),
          flush=True)
    print("control-anchors sha256: safety_model=%s chaos=%s"
          % (sha256_file(SAFETY)[:16], sha256_file(CHAOS)[:16]), flush=True)
    return 0 if passed == len(CONTROLS) else 1


if __name__ == "__main__":
    sys.exit(main())
