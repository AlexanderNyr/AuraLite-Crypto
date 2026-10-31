#!/usr/bin/env python3
"""Six D6 observability mutation controls, each targeting an exact library unit test."""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
OBS = REPO / "crates/aura-node/src/observability.rs"
LIFECYCLE = REPO / "crates/aura-node/src/lifecycle.rs"


def exact_test(name: str) -> list[str]:
    return ["cargo", "test", "-p", "aura-node", "--lib", name, "--", "--exact"]


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=REPO, text=True, capture_output=True, check=False)


def expect_red(label: str, path: Path, old: str, new: str, test_name: str) -> bool:
    original = path.read_text(encoding="utf-8")
    if old not in original:
        print(f"{label}: FAIL — mutation target not found in {path.relative_to(REPO)}")
        return False
    try:
        path.write_text(original.replace(old, new, 1), encoding="utf-8")
        result = run(exact_test(test_name))
        red = result.returncode != 0
        tail = "\n".join(
            line.rstrip()
            for line in (result.stdout + result.stderr).strip().splitlines()
            if line.strip()
        )[-1800:]
        print(f"{label}: {'PASS' if red else 'FAIL'}")
        print(f"      exact unit test {test_name} -> rc={result.returncode}")
        if tail:
            print("      " + tail.replace("\n", "\n      "))
        return red
    finally:
        path.write_text(original, encoding="utf-8")


def main() -> int:
    controls = [
        (
            "NC-1 /health refuses when the PID file is absent",
            OBS,
            "if !self.pid_file.is_file() {",
            "if false {",
            "observability::tests::health_endpoint_requires_pid_file",
        ),
        (
            "NC-2 finality lag metric remains exported",
            OBS,
            '"aurad_finality_lag_blocks",',
            '"aurad_finality_lag_removed",',
            "observability::tests::metrics_include_finality_gauge_and_dashboard_contract",
        ),
        (
            "NC-3 gas throughput derives from committed samples",
            OBS,
            "total as f64 / elapsed",
            "0.0",
            "observability::tests::gas_rate_counts_committed_samples",
        ),
        (
            "NC-4 peer count does not invent unavailable P2P peers",
            OBS,
            '"aurad_peer_count",\n            "Connected peers; zero because the N1 P2P layer is not implemented.",\n            "gauge",\n            0,',
            '"aurad_peer_count",\n            "Connected peers; zero because the N1 P2P layer is not implemented.",\n            "gauge",\n            1,',
            "observability::tests::peer_count_is_zero_until_p2p_exists",
        ),
        (
            "NC-5 structured logs remain JSON",
            LIFECYCLE,
            ".json()",
            ".compact()",
            "lifecycle::tests::structured_log_events_are_json",
        ),
        (
            "NC-6 OTLP layer records and exports spans",
            LIFECYCLE,
            "Sampler::AlwaysOn",
            "Sampler::AlwaysOff",
            "lifecycle::tests::otel_layer_exports_recorded_spans",
        ),
    ]
    results = [expect_red(*control) for control in controls]
    passed = sum(results)
    print(f"RESULT: {passed}/{len(controls)} negative controls PASS")
    restored = run(["cargo", "test", "-p", "aura-node", "--lib"])
    if restored.returncode != 0:
        print("restore check: aura-node library tests remain red")
        print((restored.stdout + restored.stderr)[-5000:])
        return 1
    print("restore check: cargo test -p aura-node --lib -> GREEN")
    return 0 if passed == len(controls) else 1


if __name__ == "__main__":
    sys.exit(main())
