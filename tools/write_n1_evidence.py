#!/usr/bin/env python3
"""Historical N1 closeout writer, frozen after the phase receipt was published.

This generator rewrites old N1 evidence and prior SHA256SUMS manifests. The
post-closeout security review must not regenerate historical receipts.
"""
from __future__ import annotations

import hashlib
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/n1"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(4)] + [f"d{i}" for i in range(2, 7)]
NC_PHASES += ["n0", "n1"]
EXPECTED_TOTAL = 237

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/plans/NET_PLAN.md",
    "docs/plans/D7_READINESS.md",
    "docs/evidence/n0/SHA256SUMS",
    "crates/aura-p2p/Cargo.toml",
    "crates/aura-p2p/src/lib.rs",
    "crates/aura-p2p/src/baseline.rs",
    "crates/aura-p2p/src/error.rs",
    "crates/aura-p2p/src/identity.rs",
    "crates/aura-p2p/src/ban.rs",
    "crates/aura-p2p/src/transport.rs",
    "crates/aura-p2p/src/bin/main.rs",
    "crates/aura-p2p/tests/fixtures/n0_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n1_baseline.txt",
    "crates/aura-p2p/tests/n1_transport.rs",
    "crates/aura-node/src/baseline.rs",
    "crates/aura-node/tests/d0_baseline.rs",
    "crates/aura-node/tests/fixtures/d0_baseline.txt",
    "tools/audit_closeout.py",
    "tools/gen_n1_vectors.py",
    "tools/nc_n1.py",
    "tools/write_n1_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (path.parent.name in {"d6", "n0", "n1"}, path.as_posix()),
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
    lines = ["# Cumulative negative-control inventory through N1", f"# Generated (UTC): {stamp}", ""]
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
        raise SystemExit("ERROR: N1 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    n1_tests = (EVIDENCE / "test-n1.log").read_text(encoding="utf-8")
    if "test result: FAILED" in n1_tests or n1_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-n1.log does not show both N1 test suites passing")

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
        raise SystemExit("ERROR: N1 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, n1_tests


def report(stamp: str) -> str:
    return f"""# N1 — P2P transport, identity, handshake, and ban list evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-p2p` implements the **N1** transport layer (`aura_p2p::identity`,
`aura_p2p::ban`, `aura_p2p::transport`, `aura_p2p::error`) and updates
`aura_p2p::baseline` to report 1 implemented network component (`transport`)
and 7 remaining placeholders (`gossipsub` N2 .. `chaos-harness` N8):

- **Cryptographic peer identity (`D-N1`)**: `PeerId = BLAKE3(ed25519_public_key)`
  with constant-time verification (`PeerId::verify_public_key`) and
  domain-separated Ed25519 handshake transcript signing/verification
  (`AURA_P2P_HANDSHAKE_V1\\0` binding `protocol_version`, `chain_id`,
  initiator/responder role, local & remote challenges, and public key).
- **Ban list**: deterministic `BanList` tracking banned `PeerId`s and `IpAddr`s
  with permanent or expiring bans (`BanEntry`, `BanReason`).
- **Noise-inspired authenticated transport & TCP swarm**:
  - `HandshakeHello` challenge-response exchange verifying `protocol_version`,
    `chain_id`, `PeerId == BLAKE3(public_key)`, expected remote `PeerId` (when
    dialing a known peer), and mutual Ed25519 transcript signatures.
  - `SessionCipher` directional BLAKE3-keyed stream + MAC session framing with
    monotonic sequence numbers and `MAX_FRAME_BYTES` cap.
  - `TransportSwarm` live TCP listener + dialer connecting 10 independent nodes,
    rejecting and auto-banning identity substitution (`P2pError::IdentitySubstitution`),
    rejecting self-connections, and enforcing peer/IP bans.

N1 does not implement gossipsub (N2), Kademlia discovery (N3), block/state sync
(N4–N5), or multi-validator devnet (D7).

## Gate results

- `make test-n1`: 18 `aura-p2p` library unit tests and 2 live 10-node TCP
  `TransportSwarm` integration tests passed.
- `make nc-n1`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact ...`); restored library tests passed.
- `make nc-all`: **237/237** cumulative negative controls through N1; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD (`2cf0321a83d6b308950a196b476ab086bf50c42c` + `0040-N0`)
  apply and revalidation: [`clean-apply.txt`](clean-apply.txt).
"""


def refresh_sums() -> None:
    files = [
        EVIDENCE / name
        for name in (
            "gates.txt",
            "nc.log",
            "nc-all.log",
            "test-n1.log",
            "make-test.log",
            "clean-apply.txt",
            "report.md",
            "inventory.txt",
        )
    ]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing N1 checksum inputs: " + ", ".join(missing))
    lines = [
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(REPO).as_posix()}"
        for path in sorted(set(files))
    ]
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    print(
        "REFUSED: N1 phase-close evidence is historical and frozen. "
        "This legacy generator rewrites receipts and prior checksum manifests; "
        "see the N1 security-review follow-up in docs/plans/NET_PLAN.md.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
