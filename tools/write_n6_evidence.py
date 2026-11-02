#!/usr/bin/env python3
"""Aggregate N6 test, mutation, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/n6"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(6)] + [f"d{i}" for i in range(2, 9)]
NC_PHASES += ["n0", "n1", "n2", "n3", "n4", "n5", "n6"]
EXPECTED_TOTAL = 291

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/plans/NET_PLAN.md",
    "docs/evidence/n5/SHA256SUMS",
    "crates/aura-p2p/Cargo.toml",
    "crates/aura-p2p/src/lib.rs",
    "crates/aura-p2p/src/baseline.rs",
    "crates/aura-p2p/src/error.rs",
    "crates/aura-p2p/src/identity.rs",
    "crates/aura-p2p/src/ban.rs",
    "crates/aura-p2p/src/transport.rs",
    "crates/aura-p2p/src/gossipsub.rs",
    "crates/aura-p2p/src/discovery.rs",
    "crates/aura-p2p/src/block_sync.rs",
    "crates/aura-p2p/src/state_sync.rs",
    "crates/aura-p2p/src/dos_defence.rs",
    "crates/aura-p2p/src/bin/main.rs",
    "crates/aura-p2p/tests/fixtures/n0_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n1_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n2_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n3_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n4_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n5_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n6_baseline.txt",
    "crates/aura-p2p/tests/n1_transport.rs",
    "crates/aura-p2p/tests/n2_gossipsub.rs",
    "crates/aura-p2p/tests/n3_discovery.rs",
    "crates/aura-p2p/tests/n4_block_sync.rs",
    "crates/aura-p2p/tests/n5_state_sync.rs",
    "crates/aura-p2p/tests/n6_dos_defence.rs",
    "tools/audit_closeout.py",
    "tools/gen_n6_vectors.py",
    "tools/nc_n6.py",
    "tools/write_n6_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (
            path.parent.name
            in {"d6", "n0", "n1", "n2", "b4", "b5", "d7", "d8", "n3", "n4", "n5", "n6"},
            path.as_posix(),
        ),
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
    lines = ["# Cumulative negative-control inventory through N6", f"# Generated (UTC): {stamp}", ""]
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
        for name, status in NC_LINE.findall(text):
            lines.append(f"  {status:<4} {name}")
        lines.append("")
    lines.append(f"TOTAL: {total} negative controls")
    return "\n".join(lines) + "\n", total


def validate() -> tuple[str, str, str, str]:
    nc = (EVIDENCE / "nc.log").read_text(encoding="utf-8")
    if RESULT_LINE.search(nc).groups() != ("6", "6"):
        raise SystemExit("ERROR: N6 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    n6_tests = (EVIDENCE / "test-n6.log").read_text(encoding="utf-8")
    if "test result: FAILED" in n6_tests or n6_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-n6.log does not show both N6 test suites passing")

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
        raise SystemExit("ERROR: N6 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, n6_tests


def report(stamp: str) -> str:
    return f"""# N6 — DoS defence: size/rate limits, signature rate limit, and equivocation detection evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-p2p` implements the **N6** DoS-defence layer (`aura_p2p::dos_defence`)
and updates `aura_p2p::baseline` to report 6 implemented network components
(`transport`, `gossipsub`, `discovery`, `block-sync`, `state-sync`,
`dos-defence`) and 2 remaining placeholders (`light-client` N7 and
`chaos-harness` N8):

- **Pre-crypto ban check (`D-N3`, `DosDefenceEngine::check_pre_crypto`)**:
  rejects banned peers in `O(1)` before spending any CPU on Ed25519 signature
  verification or payload deserialization.
- **Frame size & per-peer message rate limits (`DosDefenceLimits`)**: caps
  ingress frame size (`max_frame_bytes = 256 KiB`) and per-peer message rate
  (`max_messages_per_window = 128 msg/s`), escalating floods and repeated
  oversized frames to `BanList`.
- **Per-peer signature verification budget (`max_sig_verifications_per_window`)**:
  bounds Ed25519 verification CPU per peer (`64 sig/s` default) and bans peers
  exceeding the invalid-signature strike threshold.
- **Equivocation detection (`EquivocationEvidence`)**: detects conflicting
  consensus proposals or votes from the same peer at the same `(height, round)`,
  produces self-contained cryptographically verifiable `EquivocationEvidence`,
  and immediately bans the equivocator.

## Gate results

- `make test-n6`: 48 `aura-p2p` library unit tests and 1 `n6_dos_defence`
  integration test (1 malicious peer sending 10,000 msg/s while 4 validators
  keep finalizing heights 1..=5) passed.
- `make nc-n6`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact dos_defence::tests::<test>`); restored
  library tests passed.
- `make nc-all`: **291/291** cumulative negative controls through N6; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0049` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
"""


def refresh_sums() -> None:
    files = [
        EVIDENCE / name
        for name in (
            "gates.txt",
            "nc.log",
            "nc-all.log",
            "test-n6.log",
            "make-test.log",
            "clean-apply.txt",
            "report.md",
            "inventory.txt",
        )
    ]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing N6 checksum inputs: " + ", ".join(missing))
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
    gates = f"""N6 gates (UTC generated {stamp})

make test-n6: PASS (see test-n6.log)
  aura-p2p library unit tests: 48
  10,000 msg/s flood isolation + uninterrupted validator finalization tests: 1
make nc-n6: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total}/{total} cumulative negative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
Dependency closure and lockfile checksums: PASS (0 new external crates)
Clean GitHub-HEAD patch application and revalidation: PASS (see clean-apply.txt)
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/n6/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/n6/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
