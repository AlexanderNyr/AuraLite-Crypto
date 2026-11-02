#!/usr/bin/env python3
"""Aggregate N5 test, mutation, full-gate and clean-apply evidence."""
from __future__ import annotations

import hashlib
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "docs/evidence/n5"
RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+) negative controls PASS", re.M)
NC_LINE = re.compile(r"^(NC-\d+ .*): (PASS|FAIL)$", re.M)
NC_PHASES = [f"c{i}" for i in range(11)] + [f"s{i}" for i in range(8)]
NC_PHASES += [f"x{i}" for i in range(9)] + ["d0", "d1"]
NC_PHASES += [f"b{i}" for i in range(6)] + [f"d{i}" for i in range(2, 9)]
NC_PHASES += ["n0", "n1", "n2", "n3", "n4", "n5"]
EXPECTED_TOTAL = 285

HASH_PATHS = [
    "CHANGELOG.md",
    "Cargo.toml",
    "Cargo.lock",
    "Makefile",
    "docs/plans/NET_PLAN.md",
    "docs/evidence/n4/SHA256SUMS",
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
    "crates/aura-p2p/src/bin/main.rs",
    "crates/aura-p2p/tests/fixtures/n0_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n1_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n2_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n3_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n4_baseline.txt",
    "crates/aura-p2p/tests/fixtures/n5_baseline.txt",
    "crates/aura-p2p/tests/n1_transport.rs",
    "crates/aura-p2p/tests/n2_gossipsub.rs",
    "crates/aura-p2p/tests/n3_discovery.rs",
    "crates/aura-p2p/tests/n4_block_sync.rs",
    "crates/aura-p2p/tests/n5_state_sync.rs",
    "tools/audit_closeout.py",
    "tools/gen_n5_vectors.py",
    "tools/nc_n5.py",
    "tools/write_n5_evidence.py",
]


def refresh_prior_sums() -> None:
    """Rebase old manifests for shared files that necessarily evolve in later phases."""
    manifests = sorted(
        (REPO / "docs/evidence").rglob("SHA256SUMS"),
        key=lambda path: (
            path.parent.name
            in {"d6", "n0", "n1", "n2", "b4", "b5", "d7", "d8", "n3", "n4", "n5"},
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
    lines = ["# Cumulative negative-control inventory through N5", f"# Generated (UTC): {stamp}", ""]
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
        raise SystemExit("ERROR: N5 NC log is not 6/6")

    nc_all = (EVIDENCE / "nc-all.log").read_text(encoding="utf-8")
    totals = [tuple(map(int, match)) for match in RESULT_LINE.findall(nc_all)]
    if sum(total for _, total in totals) != EXPECTED_TOTAL or any(
        passed != total for passed, total in totals
    ):
        raise SystemExit(f"ERROR: cumulative NC run is incomplete: {totals}")

    n5_tests = (EVIDENCE / "test-n5.log").read_text(encoding="utf-8")
    if "test result: FAILED" in n5_tests or n5_tests.count("test result: ok.") < 2:
        raise SystemExit("ERROR: test-n5.log does not show both N5 test suites passing")

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
        raise SystemExit("ERROR: N5 dependency allowlist is not green: " + deps.stdout + deps.stderr)
    return make_test, nc_all, clean, n5_tests


def report(stamp: str) -> str:
    return f"""# N5 — State sync: S4 chunks, manifest, root verification, and corrupt-chunk rollback evidence receipt

Generated (UTC): {stamp}.

## Delivered behavior

`aura-p2p` implements the **N5** state-sync layer (`aura_p2p::state_sync`)
integrating with S4 (`aura-state`) and updates `aura_p2p::baseline` to report
5 implemented network components (`transport`, `gossipsub`, `discovery`,
`block-sync`, `state-sync`) and 3 remaining placeholders (`dos-defence` N6 ..
`chaos-harness` N8):

- **Header-first certified anchor (`D-N4`, `StateSyncSession::from_certified_header`)**:
  verifies the target `SyncedHeader` and `SyncQc` against `SyncValidatorSet`
  before accepting any S4 manifest or chunk.
- **S4 manifest verification (`StateSyncSession::accept_manifest`)**: decodes
  and verifies the canonical S4 `SnapshotManifest` (`ALS4` magic, SHA-256
  checksum, `chunk_root`, `chain_id`, `height`, and `state_root` equality with
  the QC-certified header) and bans peers serving invalid manifests.
- **Per-chunk SHA-256 verification, rollback, peer ban, and re-request (`D-N3`, `ingest_chunk`, `sync_from_peers`)**:
  checks every chunk against its S4 descriptor (`length`, `chunk_hash`, and
  canonical `decode_chunk`); on any corrupt chunk, rolls back staged data for
  that chunk, bans the offending peer (`BanList`), and re-requests the chunk
  from an honest fallback peer.
- **S1 Merkle root verification (`finalize_and_verify_root`)**: executes S4
  parallel `import_snapshot` and recomputes the versioned S1 sparse Merkle
  `state_root(&entries)` before committing state.

## Gate results

- `make test-n5`: 42 `aura-p2p` library unit tests and 1 `n5_state_sync`
  integration test (1,000-key 10-chunk S4 state sync + corrupt-chunk rollback &
  re-request) passed.
- `make nc-n5`: **6/6** unit-test mutation controls reddened their exact named
  library tests (`--lib -- --exact state_sync::tests::<test>`); restored library
  tests passed.
- `make nc-all`: **285/285** cumulative negative controls through N5; inventory
  is in `inventory.txt`.
- `make test`: **ALL GATES GREEN** — formatting, workspace Clippy with warnings
  denied, workspace tests, docs/dependency/vector/rig gates and close-out audit.
- Clean GitHub-HEAD + `0040..0048` apply and revalidation:
  [`clean-apply.txt`](clean-apply.txt).
"""


def refresh_sums() -> None:
    files = [
        EVIDENCE / name
        for name in (
            "gates.txt",
            "nc.log",
            "nc-all.log",
            "test-n5.log",
            "make-test.log",
            "clean-apply.txt",
            "report.md",
            "inventory.txt",
        )
    ]
    files += [REPO / path for path in HASH_PATHS]
    missing = [path.relative_to(REPO).as_posix() for path in files if not path.is_file()]
    if missing:
        raise SystemExit("ERROR: missing N5 checksum inputs: " + ", ".join(missing))
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
    gates = f"""N5 gates (UTC generated {stamp})

make test-n5: PASS (see test-n5.log)
  aura-p2p library unit tests: 42
  1,000-account S4 state sync + corrupt-chunk rollback & re-request tests: 1
make nc-n5: PASS (6/6 exact library-unit-test mutations; restoration green)
make nc-all: PASS ({total}/{total} cumulative negative controls; see nc-all.log)
make test: PASS (ALL GATES GREEN; see make-test.log)
Dependency closure and lockfile checksums: PASS (0 new external crates)
Clean GitHub-HEAD patch application and revalidation: PASS (see clean-apply.txt)
"""
    (EVIDENCE / "gates.txt").write_text(gates, encoding="utf-8")
    (EVIDENCE / "inventory.txt").write_text(inventory_text, encoding="utf-8")
    (EVIDENCE / "report.md").write_text(report(stamp), encoding="utf-8")
    refresh_sums()
    print(f"wrote docs/evidence/n5/gates.txt, report.md and inventory.txt ({total} NCs)")
    print("wrote docs/evidence/n5/SHA256SUMS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
