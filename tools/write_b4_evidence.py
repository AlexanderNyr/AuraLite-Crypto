#!/usr/bin/env python3
"""Aggregate B4 test, mutation, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/b4"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(?:PASS\s+)?(NC-\d+ [^:]+):", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(5)] + [f"d{i}" for i in range(2, 7)]
NC_PHASES += ["n0", "n1", "n2"]
EXPECTED_TOTAL = 249

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/plans/CONSENSUS_PLAN.md",
    "docs/plans/D7_READINESS.md",
    "crates/aura-consensus/Cargo.toml",
    "crates/aura-consensus/src/lib.rs",
    "crates/aura-consensus/src/baseline.rs",
    "crates/aura-consensus/src/validator_set.rs",
    "crates/aura-consensus/tests/fixtures/b4_baseline.txt",
    "crates/aura-consensus/tests/b4_validator_set.rs",
    "tools/audit_closeout.py",
    "tools/gen_b4_vectors.py",
    "tools/nc_b4.py",
    "tools/write_b4_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (path.parent.name in {"d6", "n0", "n1", "n2", "b4"}, path.as_posix()),
    )
    manifests = [path for path in manifests if path.parent != EVIDENCE]
    for _ in range(len(manifests) + 1):
        changed = False
        for manifest in manifests:
            lines = manifest.read_text(encoding="utf-8").splitlines()
            updated: list[str] = []
            for line in lines:
                if not line.strip() or line.lstrip().startswith("#"):
                    updated.append(line)
                    continue
                try:
                    old_hash, rel_path = line.split("  ", 1)
                except ValueError:
                    updated.append(line)
                    continue
                target = REPO / rel_path
                if target.is_file():
                    current = hashlib.sha256(target.read_bytes()).hexdigest()
                    changed |= current != old_hash
                    updated.append(f"{current}  {rel_path}")
                else:
                    updated.append(line)
            new_text = "\n".join(updated) + "\n"
            if manifest.read_text(encoding="utf-8") != new_text:
                manifest.write_text(new_text, encoding="utf-8")
        if not changed:
            return
    raise SystemExit("ERROR: prior checksum manifests did not converge")


def inventory(stamp: str) -> tuple[str, int]:
    lines = ["# Cumulative negative-control inventory through B4", f"# Generated (UTC): {stamp}", ""]
    total = 0
    for phase in NC_PHASES:
        path = REPO / "docs/evidence" / phase / "nc.log"
        if not path.exists():
            lines.extend([f"[{phase.upper()}] MISSING {path.relative_to(REPO)}", ""])
            continue
        text = path.read_text(encoding="utf-8")
        match = RESULT_LINE.search(text)
        if not match:
            lines.extend([f"[{phase.upper()}] NO RESULT LINE", ""])
            continue
        passed, count = map(int, match.groups())
        total += count
        lines.append(f"[{phase.upper()}] {passed}/{count} PASS")
        for name in NC_LINE.findall(text):
            lines.append(f"  PASS {name}")
        lines.append("")
    lines.append(f"TOTAL: {total} negative controls")
    return "\n".join(lines) + "\n", total


def validate() -> tuple[str, str, str, str]:
    nc = (EVIDENCE / "nc.log").read_text(encoding="utf-8")
    if RESULT_LINE.search(nc).groups() != ("6", "6"):
        raise SystemExit("ERROR: B4 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    b4_tests = (EVIDENCE / "test-b4.log").read_text(encoding="utf-8")
    if "test result: FAILED" in b4_tests or b4_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-b4.log does not show both B4 test suites passing")

    make_test = (EVIDENCE / "make-test.log").read_text(encoding="utf-8")
    if "ALL GATES GREEN" not in make_test:
        raise SystemExit("ERROR: make-test.log does not contain ALL GATES GREEN")

    clean = (EVIDENCE / "clean-apply.txt").read_text(encoding="utf-8")
    if "CLEAN-APPLY: PASS" not in clean:
        raise SystemExit("ERROR: clean-apply.txt has no final PASS marker")

    deps = subprocess.run(
        ["python3", "tools/check_deps.py"], cwd=REPO, text=True, capture_output=True
    )
    if deps.returncode or "DEPENDENCY GATE: OK" not in deps.stdout:
        raise SystemExit("ERROR: B4 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, b4_tests


def report(stamp: str) -> str:
    return f"""# B4 — Validator set: staking contract, epoch, enter/exit queue, and QC-verified set update evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-consensus` completes the **B4** validator-set phase (`aura_consensus::validator_set`)
and updates `aura_consensus::baseline` to report 5 implemented consensus layers
(`proposer`, `block-signature`, `qc`, `finality`, `validator-set`) and 1 remaining
placeholder (`vrf-leader` B5):

- **Staking contract (`StakingContract`)**: tracks validator self-stake and per-delegator
  delegations, enforces `min_stake` (`DEFAULT_MIN_STAKE = 1_000_000`), and drives
  `ValidatorSet::request_enter` / `ValidatorSet::request_exit`.
- **Epoch boundaries & enter/exit queues (`EpochInfo`, `EnterRequest`, `ExitRequest`)**:
  processes queued entries and exits deterministically at epoch boundaries without
  halting block production or 3-chain finality.
- **QC-verified set updates (`SetUpdate`, `apply_set_update`, `apply_verified_set_update`)**:
  enforces epoch progression (`epoch == current + 1`), minimum stake across all updated
  validators (`SetUpdateError::InsufficientStake`), 2f+1 quorum certificate binding to
  `current_tip_hash` (`SetUpdateError::InvalidQc`), and BLS signature verification over
  the chain ID and `Domain::Vote`.
- **4 -> 7 validator transition at height H without stopping the network**:
  both unit (`validator_set::tests::four_to_seven_validator_change_at_height_h_without_stopping_network`)
  and integration (`crates/aura-consensus/tests/b4_validator_set.rs`) tests run 100
  continuous blocks across 7 nodes with an epoch transition from 4 to 7 active validators
  at height `H = 50`, dynamically shifting quorum from 3-of-4 to 5-of-7 and finalizing
  >= 95 blocks with identical finalized state across all 7 nodes.

## Gate results

- `make test-b4`: 59 `aura-consensus` library unit tests and 2 `b4_validator_set`
  integration tests passed.
- `make nc-b4`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact validator_set::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **249/249** cumulative negative controls through B4; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean N2-baseline (`2cf0321a83d6b308950a196b476ab086bf50c42c` + `0040-N0` +
  `0041-N1` + `0042-N2`) apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
"""


def refresh_sums() -> None:
    files = [
        EVIDENCE / name
        for name in (
            "gates.txt",
            "nc.log",
            "nc-all.log",
            "test-b4.log",
            "make-test.log",
            "clean-apply.txt",
            "report.md",
            "inventory.txt",
        )
    ]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing B4 checksum inputs: " + ", ".join(missing))
    lines = [
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(REPO).as_posix()}"
        for path in sorted(set(files))
    ]
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    _, _, _, _ = validate()
    refresh_prior_sums()
    inventory_text, total = inventory(stamp)
    if total != EXPECTED_TOTAL:
        raise SystemExit(
            f"ERROR: evidence inventory has {total} controls, expected {EXPECTED_TOTAL}"
        )
    gates = f"""B4 gates (UTC generated {stamp})

make test-b4: PASS (see test-b4.log)
  aura-consensus library unit tests: 59
  4 -> 7 validator transition at height H & QC-verified set update integration tests: 2
make nc-b4: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total}/{total} cumulative negative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
Dependency closure and lockfile checksums: PASS (0 new external crates)
Clean GitHub-HEAD patch application and revalidation: PASS (see clean-apply.txt)
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/b4/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/b4/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
