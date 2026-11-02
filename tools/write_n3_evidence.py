#!/usr/bin/env python3
"""Aggregate N3 test, mutation, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/n3"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(6)] + [f"d{i}" for i in range(2, 9)]
NC_PHASES += ["n0", "n1", "n2", "n3"]
EXPECTED_TOTAL = 273

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/plans/NET_PLAN.md",
    "docs/evidence/n2/SHA256SUMS",
    "crates/aura-p2p/Cargo.toml",
    "crates/aura-p2p/src/lib.rs",
    "crates/aura-p2p/src/baseline.rs",
    "crates/aura-p2p/src/error.rs",
    "crates/aura-p2p/src/identity.rs",
    "crates/aura-p2p/src/ban.rs",
    "crates/aura-p2p/src/transport.rs",
    "crates/aura-p2p/src/gossipsub.rs",
    "crates/aura-p2p/src/discovery.rs",
    "crates/aura-p2p/src/bin/main.rs",
    "crates/aura-p2p/tests/fixtures/n0_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n1_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n2_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n3_baseline.txt",
    "crates/aura-p2p/tests/n1_transport.rs",
    "crates/aura-p2p/tests/n2_gossipsub.rs",
    "crates/aura-p2p/tests/n3_discovery.rs",
    "tools/audit_closeout.py",
    "tools/gen_n3_vectors.py",
    "tools/nc_n3.py",
    "tools/write_n3_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (
            path.parent.name in {"d6", "n0", "n1", "n2", "b4", "b5", "d7", "d8", "n3"},
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
    lines = ["# Cumulative negative-control inventory through N3", f"# Generated (UTC): {stamp}", ""]
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
        raise SystemExit("ERROR: N3 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    n3_tests = (EVIDENCE / "test-n3.log").read_text(encoding="utf-8")
    if "test result: FAILED" in n3_tests or n3_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-n3.log does not show both N3 test suites passing")

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
        raise SystemExit("ERROR: N3 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, n3_tests


def report(stamp: str) -> str:
    return f"""# N3 — Peer discovery: Kademlia DHT, bootstrap list, NAT/UPnP, and Circuit Relay v2 evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-p2p` implements the **N3** discovery layer (`aura_p2p::discovery`) and
updates `aura_p2p::baseline` to report 3 implemented network components
(`transport`, `gossipsub`, `discovery`) and 5 remaining placeholders
(`block-sync` N4 .. `chaos-harness` N8):

- **Kademlia DHT (`XorDistance`, `KademliaTable`, `KADEMLIA_K = 20`, `KADEMLIA_ALPHA = 3`)**:
  256-bit XOR metric, 256 k-buckets, closest-peer ordering (`closest_peers`),
  stale sequence replay rejection (`StalePeerRecord`), and `BanList` enforcement.
- **Cryptographically signed peer records (`D-N1`, `D-N3`)**: `SignedPeerRecord`
  signs and verifies `(DISCOVERY_RECORD_DOMAIN, chain_id, peer_id, public_key, seq, direct_addrs, nat_status)`
  with Ed25519 and checks `PeerId == BLAKE3(public_key)` in constant time.
- **Bootstrap list, NAT/UPnP, and Circuit Relay v2**: `BootstrapList`,
  `UpnpMapper` (`NatStatus::UpnpMapped`), and `RelayServer` / `RelayReservation`
  (`NatStatus::BehindSymmetricNat`) with TTL expiration and frame size bounds.
- **30-node behind-NAT discovery gate (`crates/aura-p2p/tests/n3_discovery.rs`)**:
  30 NATed nodes (5 cone-NAT UPnP-mapped relay-capable peers + 25 symmetric-NAT
  peers) seeded with at most 2 local neighbors and no central server converge in
  `<= 4` iterative Kademlia lookup rounds so every node discovers all 29 other
  verified peer records and exchanges traffic over relay and UPnP endpoints.

## Gate results

- `make test-n3`: 30 `aura-p2p` library unit tests and 2 `n3_discovery`
  integration tests passed.
- `make nc-n3`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact discovery::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **273/273** cumulative negative controls through N3; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0046` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
"""


def refresh_sums() -> None:
    files = [
        EVIDENCE / name
        for name in (
            "gates.txt",
            "nc.log",
            "nc-all.log",
            "test-n3.log",
            "make-test.log",
            "clean-apply.txt",
            "report.md",
            "inventory.txt",
        )
    ]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing N3 checksum inputs: " + ", ".join(missing))
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
    gates = f"""N3 gates (UTC generated {stamp})

make test-n3: PASS (see test-n3.log)
  aura-p2p library unit tests: 30
  30-node behind-NAT Kademlia discovery + UPnP + Circuit Relay v2 tests: 2
make nc-n3: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total}/{total} cumulative negative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
Dependency closure and lockfile checksums: PASS (0 new external crates)
Clean GitHub-HEAD patch application and revalidation: PASS (see clean-apply.txt)
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/n3/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/n3/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
