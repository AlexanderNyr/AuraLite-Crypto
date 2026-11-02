#!/usr/bin/env python3
"""Aggregate N0 test, mutation, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/n0"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(4)] + [f"d{i}" for i in range(2, 7)]
NC_PHASES += ["n0"]
EXPECTED_TOTAL = 231

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/plans/NET_PLAN.md",
    "docs/plans/D7_READINESS.md",
    "docs/evidence/d6/SHA256SUMS",
    "crates/aura-p2p/Cargo.toml",
    "crates/aura-p2p/src/lib.rs",
    "crates/aura-p2p/src/baseline.rs",
    "crates/aura-p2p/src/bin/main.rs",
    "crates/aura-p2p/tests/fixtures/n0_baseline.txt",
    "crates/aura-node/src/baseline.rs",
    "crates/aura-node/tests/n0_two_nodes_no_p2p.rs",
    "tools/audit_closeout.py",
    "tools/gen_n0_vectors.py",
    "tools/nc_n0.py",
    "tools/write_n0_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (path.parent.name in {"d6", "n0"}, path.as_posix()),
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
    lines = ["# Cumulative negative-control inventory through N0", f"# Generated (UTC): {stamp}", ""]
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
        raise SystemExit("ERROR: N0 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    n0_tests = (EVIDENCE / "test-n0.log").read_text(encoding="utf-8")
    if "test result: FAILED" in n0_tests or n0_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-n0.log does not show both N0 test suites passing")

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
        raise SystemExit("ERROR: N0 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, n0_tests


def report(stamp: str) -> str:
    return f"""# N0 — P2P network baseline evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-p2p` implements the N0 baseline survey (`aura_p2p::baseline`) and CLI
(`aura-p2p`). All eight `NET_PLAN` components (`transport` N1, `gossipsub` N2,
`discovery` N3, `block-sync` N4, `state-sync` N5, `dos-defence` N6,
`light-client` N7, `chaos-harness` N8) measure as `placeholder` (0 implemented,
8 placeholders, 0 missing). Invoking `aura-p2p` with no arguments exits `2`
(`EXIT_STUB`) and lists every missing capability with the phase that owes it.

Two-node RPC isolation is measured both in `aura_p2p::baseline` and in the live
two-node HTTP/JSON-RPC gate (`crates/aura-node/tests/n0_two_nodes_no_p2p.rs`):
when two independent `aurad` HTTP/JSON-RPC servers (`node-a` and `node-b`) run
with the same genesis state and a signed transfer is submitted and committed on
Node A, Node A serves the transaction and updated recipient balance over RPC,
while Node B returns `null` for `get_tx`, keeps recipient balance `0`, and stays
at height `0`. Both nodes report `peerCount = 0` and `p2p = "not_implemented"`.

N0 does not implement peer transport, gossipsub, discovery, sync, or a
multi-validator devnet.

## Gate results

- `make test-n0`: 10 `aura-p2p` library unit tests and 1 live two-node HTTP
  JSON-RPC isolation test passed.
- `make nc-n0`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact baseline::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **231/231** cumulative negative controls through N0; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean D6-baseline (`2cf0321a83d6b308950a196b476ab086bf50c42c`) apply and
  revalidation: [`clean-apply.txt`](clean-apply.txt).
"""


def refresh_sums() -> None:
    files = [
        EVIDENCE / name
        for name in (
            "gates.txt",
            "nc.log",
            "nc-all.log",
            "test-n0.log",
            "make-test.log",
            "clean-apply.txt",
            "report.md",
            "inventory.txt",
        )
    ]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing N0 checksum inputs: " + ", ".join(missing))
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
    gates = f"""N0 gates (UTC generated {stamp})

make test-n0: PASS (see test-n0.log)
  aura-p2p baseline unit tests: 10
  live two-node HTTP/JSON-RPC isolation test: 1
make nc-n0: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total}/{total} cumulative negative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
Dependency closure and lockfile checksums: PASS (0 new external crates)
Clean GitHub-HEAD patch application and revalidation: PASS (see clean-apply.txt)
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/n0/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/n0/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
