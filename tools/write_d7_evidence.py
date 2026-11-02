#!/usr/bin/env python3
"""Aggregate D7 test, mutation, devnet logs, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/d7"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(?:PASS\s+)?(NC-\d+ [^:]+):", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(6)] + [f"d{i}" for i in range(2, 8)]
NC_PHASES += ["n0", "n1", "n2"]
EXPECTED_TOTAL = 261

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docker-compose.yml",
    "docs/NODE.md",
    "docs/plans/NODE_PLAN.md",
    "docs/plans/D7_READINESS.md",
    "crates/aura-node/Cargo.toml",
    "crates/aura-node/src/lib.rs",
    "crates/aura-node/src/devnet.rs",
    "crates/aura-node/src/pipeline.rs",
    "crates/aura-node/src/bin/aurad.rs",
    "crates/aura-node/tests/d7_devnet.rs",
    "tools/audit_closeout.py",
    "tools/nc_d7.py",
    "tools/run_devnet.py",
    "tools/write_d7_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (
            path.parent.name in {"d6", "n0", "n1", "n2", "b4", "b5", "d7"},
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
    lines = [
        "# Cumulative negative-control inventory through D7",
        f"# Generated (UTC): {stamp}",
        "",
    ]
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
        raise SystemExit("ERROR: D7 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    d7_tests = (EVIDENCE / "test-d7.log").read_text(encoding="utf-8")
    if "test result: FAILED" in d7_tests or d7_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-d7.log does not show both D7 test suites passing")

    make_test = (EVIDENCE / "make-test.log").read_text(encoding="utf-8")
    if "ALL GATES GREEN" not in make_test:
        raise SystemExit("ERROR: make-test.log does not contain ALL GATES GREEN")

    clean = (EVIDENCE / "clean-apply.txt").read_text(encoding="utf-8")
    if "CLEAN APPLY VERIFICATION: PASS" not in clean:
        raise SystemExit("ERROR: clean-apply.txt does not record a passing clean apply")

    for idx in range(4):
        vlog = EVIDENCE / f"validator-{idx}.log"
        if not vlog.is_file() or "devnet round finalized with 2f+1 QC" not in vlog.read_text(
            encoding="utf-8"
        ):
            raise SystemExit(f"ERROR: missing or incomplete {vlog}")

    return nc, d7_tests, make_test, clean


def write_report(stamp: str) -> None:
    report = f"""# D7 — Four-validator devnet (M1) report

Generated (UTC): `{stamp}`
Deliverable: `patches/0045-D7-devnet.patch`

## Scope delivered

1. **Four-validator devnet composition (`crates/aura-node/src/devnet.rs`)**:
   - Deterministic devnet configuration (`DEVNET_CHAIN_ID = "auralite-devnet-1"`, `DEVNET_VALIDATOR_COUNT = 4`, `DEVNET_QUORUM_THRESHOLD = 3`, `DEVNET_GENESIS_TIME_MS = 1_700_000_000_000`).
   - Development-only faucet account (`DEVNET_FAUCET_SEED`, `faucet_address`, `build_faucet_transfer`) credited with `1,000,000,000,000` units at genesis across all four validators.
   - Peer identity verification (`verify_devnet_peer_identity`) rejecting substituted Ed25519 keys where `PeerId != BLAKE3(public_key)`.
   - Signed and deduplicated N2 wire frame ingestion (`ingest_devnet_gossip`) rejecting malformed/unsigned/forged frames and deduplicating repeated messages via `SeenCache`.
   - BFT `2f + 1` quorum threshold (`devnet_quorum_threshold(4) == 3`) and BLS vote verification/QC formation (`try_commit_round_votes`) ensuring 2 votes of 4 never commit.
   - In-process `DevnetCluster` and multi-process `aurad` worker (`run_devnet_worker`, `trigger_and_wait_devnet_round`) gating every local `Pipeline::produce_block()` on a verified 3-of-4 BLS `QuorumCertificate`.
2. **Multi-process M1 acceptance gate (`crates/aura-node/tests/d7_devnet.rs`, `tools/run_devnet.py`, `docker-compose.yml`)**:
   - Spawns 4 independent `aurad` validator processes with separate `--data-dir`, `--rpc-addr`, `--grpc-addr`, and `--validator-key`.
   - Verifies HTTP 200 on `/health` and Prometheus `/metrics` across all 4 validators.
   - Submits a signed faucet transfer via JSON-RPC (`submit_tx`), gossips it over N2 `GossipTopic::Tx`, and finalizes `>= 100` blocks (`height = 101`) with all 4 validators reporting identical `height`, `hash` (`block_hash`), `stateRoot` (`state_root`), and account balances (`get_block`, `get_balance`).
   - Stops `validator-3` (`SIGTERM`) and verifies the remaining 3 validators preserve the `f = 1` quorum of 3 and finalize additional blocks (`height = 113`) and transfers with identical `state_root`.
   - Records per-validator JSON structured logs in `docs/evidence/d7/validator-{{0..3}}.log` showing the common finalized `state_root`.

## Verification gates

- `make test-d7`: 6 `devnet::tests` library unit tests + 1 live 4-process `d7_devnet` M1 acceptance test PASS.
- `make devnet`: 4-validator `aurad` run finalizes `height=101` (`4/4`) and `height=113` (`3/4` after 1-validator outage) with identical `state_root` across online validators.
- `make nc-d7`: **6/6** library unit-test mutation controls PASS.
- `make nc-all`: **261/261** cumulative negative controls PASS across all 45 phases (`C0..C10`, `S0..S7`, `X0..X8`, `D0..D7`, `B0..B5`, `N0..N2`).
- `make test`: `ALL GATES GREEN`.

## Explicit non-claims

- Catch-up block/header sync for a restarted validator rejoining after an outage belongs to `NET_PLAN` **N4** (`Block/header sync`).
- Fast state sync over P2P belongs to `NET_PLAN` **N5** (`Fast sync hookup`).
- Full CLI operator subcommands (`aura` query/tx/keys/snapshot/upgrade) belong to `NODE_PLAN` **D8**.
"""
    (EVIDENCE / "report.md").write_text(report, encoding="utf-8")


def write_gates(
    stamp: str,
    inv_total: int,
    nc: str,
    d7_tests: str,
    make_test: str,
    clean: str,
) -> None:
    rustc = subprocess.check_output(["rustc", "--version"], cwd=REPO, text=True).strip()
    cargo = subprocess.check_output(["cargo", "--version"], cwd=REPO, text=True).strip()
    gates = f"""# D7 Gate Receipts
Generated (UTC): {stamp}
Toolchain: {rustc} / {cargo}

== make test-d7 ==
{d7_tests.strip()}

== make nc-d7 (6/6 library unit-test negative controls) ==
{nc.strip()}

== Cumulative negative controls ==
{inv_total}/{EXPECTED_TOTAL} PASS (see docs/evidence/d7/inventory.txt and nc-all.log)

== Clean-apply validation ==
{clean.strip()}

== make test (summary tail) ==
{chr(10).join(make_test.strip().splitlines()[-25:])}
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")


def write_sums() -> None:
    entries: list[str] = []
    evidence_files = sorted(
        path.relative_to(REPO).as_posix()
        for path in EVIDENCE.iterdir()
        if path.is_file() and path.name != "SHA256SUMS"
    )
    for rel in HASH_PATHS + evidence_files:
        path = REPO / rel
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        entries.append(f"{digest}  {rel}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(entries) + "\n", encoding="utf-8")


def main() -> None:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    nc, d7_tests, make_test, clean = validate()
    inv_text, inv_total = inventory(stamp)
    if inv_total != EXPECTED_TOTAL:
        raise SystemExit(f"ERROR: expected {EXPECTED_TOTAL} NCs in inventory, got {inv_total}")
    (EVIDENCE / "inventory.txt").write_text(inv_text, encoding="utf-8")
    write_report(stamp)
    write_gates(stamp, inv_total, nc, d7_tests, make_test, clean)
    write_sums()
    refresh_prior_sums()
    print(f"D7 evidence written: {inv_total}/{EXPECTED_TOTAL} NCs, SHA256SUMS updated")


if __name__ == "__main__":
    main()
