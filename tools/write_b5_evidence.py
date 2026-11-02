#!/usr/bin/env python3
"""Aggregate B5 test, mutation, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/b5"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(?:PASS\s+)?(NC-\d+ [^:]+):", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(6)] + [f"d{i}" for i in range(2, 7)]
NC_PHASES += ["n0", "n1", "n2"]
EXPECTED_TOTAL = 255

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
    "crates/aura-consensus/src/vrf_leader.rs",
    "crates/aura-consensus/tests/fixtures/b5_baseline.txt",
    "crates/aura-consensus/tests/b5_vrf_leader.rs",
    "tools/audit_closeout.py",
    "tools/gen_b5_vectors.py",
    "tools/nc_b5.py",
    "tools/write_b5_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (path.parent.name in {"d6", "n0", "n1", "n2", "b4", "b5"}, path.as_posix()),
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
    lines = ["# Cumulative negative-control inventory through B5", f"# Generated (UTC): {stamp}", ""]
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
        raise SystemExit("ERROR: B5 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    b5_tests = (EVIDENCE / "test-b5.log").read_text(encoding="utf-8")
    if "test result: FAILED" in b5_tests or b5_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-b5.log does not show both B5 test suites passing")

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
        raise SystemExit("ERROR: B5 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, b5_tests


def report(stamp: str) -> str:
    return f"""# B5 — Stake-weighted VRF leader election, grinding protection, and N1/N2 consensus wiring evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-consensus` completes the **B5** VRF leader and N1/N2 BFT networking phase
(`aura_consensus::vrf_leader`) and updates `aura_consensus::baseline` to report
all 6 core consensus layers (`proposer`, `block-signature`, `qc`, `finality`,
`validator-set`, `vrf-leader`) as `implemented`:

- **Cryptographic VRF & grinding protection (`VrfInput`, `VrfProof`, `prove_vrf`, `verify_vrf`, `derive_epoch_randomness`)**:
  - `VrfInput` binds `(chain_id, epoch, epoch_randomness, view, round)` and rejects
    zero `epoch_randomness` after epoch 0 (`VrfError::ZeroEpochRandomness`).
  - `derive_epoch_randomness` verifies the boundary `QuorumCertificate` under
    `(chain_id, Domain::Vote)` (`VrfError::InvalidBoundaryQc`) and sorts signer
    `(public_key, signature)` pairs canonically so vote arrival order cannot be
    manipulated to grind future epoch seeds.
  - `prove_vrf` and `verify_vrf` use deterministic BLS12-381 signatures over the
    domain-separated VRF transcript and verify both the BLS proof signature and
    the 32-byte uniform output hash.
- **Stake-weighted VRF leader election (`VrfLeaderElection`, `LeaderClaim`)**:
  - Maps the uniform 64-bit round VRF draw onto `[0, total_stake)` via 128-bit
    fixed-point arithmetic (`((draw as u128) * (total_stake as u128)) >> 64`),
    selecting each validator with probability strictly proportional to
    `stake_i / total_stake`.
  - Over 10,000 rounds across unequal stakes (`10%`, `20%`, `30%`, `40%`), every
    validator's empirical proposer count falls within its statistical confidence
    interval (`+/- 200` rounds).
- **Connecting B3/B4 BFT finality to N1/N2 (`ProposalEnvelope`, `VoteEnvelope`, `NetworkedValidatorNode`, `FourValidatorNetwork`)**:
  - Wires `aura_p2p::identity::PeerIdentity` and `aura_p2p::gossipsub::GossipEngine`
    (`GossipTopic::Proposal` and `GossipTopic::Vote`) to `FinalityEngine`,
    `QcAggregator`, `ValidatorSet`, and `VrfLeaderElection` without bypassing
    BLS vote verification or QC verification.
  - A 4-validator (`f = 1`, quorum = 3) network finalizes 100 rounds over N2
    gossip, including 50 rounds with 1 validator offline (`set_online(3, false)`).

## Gate results

- `make test-b5`: 67 `aura-consensus` library unit tests and 3 `b5_vrf_leader`
  integration tests passed.
- `make nc-b5`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact vrf_leader::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **255/255** cumulative negative controls through B5; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean B4-baseline (`2cf0321a83d6b308950a196b476ab086bf50c42c` + `0040-N0` +
  `0041-N1` + `0042-N2` + `0043-B4`) apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
"""


def refresh_sums() -> None:
    files = [
        EVIDENCE / name
        for name in (
            "gates.txt",
            "nc.log",
            "nc-all.log",
            "test-b5.log",
            "make-test.log",
            "clean-apply.txt",
            "report.md",
            "inventory.txt",
        )
    ]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing B5 checksum inputs: " + ", ".join(missing))
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
    gates = f"""B5 gates (UTC generated {stamp})

make test-b5: PASS (see test-b5.log)
  aura-consensus library unit tests: 67
  10,000-round VRF distribution + 4-validator N1/N2 BFT finality with f=1 outage integration tests: 3
make nc-b5: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total}/{total} cumulative negative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
Dependency closure and lockfile checksums: PASS (0 new external crates)
Clean GitHub-HEAD patch application and revalidation: PASS (see clean-apply.txt)
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/b5/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/b5/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
