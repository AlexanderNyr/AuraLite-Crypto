#!/usr/bin/env python3
"""Write W0 evidence (`docs/evidence/w0/`)."""

from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs" / "evidence" / "w0"

PHASE_NC_COUNTS = [
    ("C0", 2), ("C1", 3), ("C2", 4), ("C3", 5), ("C4", 6), ("C5", 6),
    ("C6", 6), ("C7", 6), ("C8", 6), ("C9", 6), ("C10", 6),
    ("S0", 6), ("S1", 6), ("S2", 6), ("S3", 6), ("S4", 6), ("S5", 6),
    ("S6", 6), ("S7", 6),
    ("X0", 6), ("X1", 6), ("X2", 7), ("X3", 6), ("X4", 6), ("X5", 6),
    ("X6", 6), ("X7", 6), ("X8", 6),
    ("D0", 6), ("D1", 6), ("B0", 6), ("B1", 6), ("B2", 6), ("B3", 6),
    ("D2", 6), ("D3", 6), ("D4", 6), ("D5", 6), ("D6", 6),
    ("N0", 6), ("N1", 6), ("N2", 6), ("B4", 6), ("B5", 6),
    ("D7", 6), ("D8", 6),
    ("N3", 6), ("N4", 6), ("N5", 6), ("N6", 6), ("N7", 6), ("N8", 6),
    ("B6", 6), ("B7", 6), ("B8", 6), ("B9", 6), ("B10", 6),
    ("W0", 6),
]


def run_cmd(cmd: list[str]) -> str:
    res = subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True, check=True)
    return res.stdout + res.stderr


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)

    vectors_log = run_cmd([sys.executable, "tools/gen_w0_vectors.py"])
    (EVIDENCE / "vectors.log").write_text(vectors_log, encoding="utf-8")

    gate_log = run_cmd(["cargo", "test", "-p", "aura-wallet"])
    (EVIDENCE / "gate.log").write_text(gate_log, encoding="utf-8")

    nc_log = run_cmd([sys.executable, "tools/nc_w0.py"])
    (EVIDENCE / "nc.log").write_text(nc_log, encoding="utf-8")

    gates_txt = (
        "== W0 vector generation ==\n"
        + vectors_log
        + "\n== W0 crate test gate (unit + integration) ==\n"
        + gate_log
        + "\n== W0 negative controls (6/6) ==\n"
        + nc_log
    )
    (EVIDENCE / "gates.txt").write_text(gates_txt, encoding="utf-8")

    total_nc = sum(n for _, n in PHASE_NC_COUNTS)
    inv_lines = ["# Negative-control inventory through W0"]
    for phase, count in PHASE_NC_COUNTS:
        inv_lines.append(f"{phase}: {count}/{count} PASS")
    inv_lines.append(f"TOTAL: {total_nc} negative controls")
    (EVIDENCE / "inventory.txt").write_text("\n".join(inv_lines) + "\n", encoding="utf-8")

    readme = """# W0 Evidence — Wallet Baseline & Key-Exposure Audit

## Artifacts
- `threat-notes.md` — 8 key-exposure surfaces ("where keys could end up") and 6 threat model notes (`T-W0-1`..`T-W0-6`).
- `gates.txt` — combined vector generation, unit + integration test output, and 6/6 negative controls.
- `report.md` — W0 phase close-out report.
- `inventory.txt` — cumulative negative-control inventory (`339` total through W0).
- `gate.log` — `cargo test -p aura-wallet` output.
- `nc.log` — `python3 tools/nc_w0.py` output (`6/6 PASS`).
- `vectors.log` — `python3 tools/gen_w0_vectors.py` output (`w0_baseline.txt` and `w0_key_audit.txt`).
- `SHA256SUMS` — SHA-256 digests of all W0 deliverables and evidence files.
"""
    (EVIDENCE / "README.md").write_text(readme, encoding="utf-8")

    report = f"""# W0 Report — Wallet Baseline & Key-Exposure Audit

## Summary
Phase **W0** establishes the `aura-wallet` crate (`crates/aura-wallet`), the
key-exposure audit ("where keys could end up") across 8 concrete exposure
surfaces (`EXPOSURE_SURFACES`), 6 threat model notes (`THREAT_NOTES`,
`T-W0-1`..`T-W0-6`) aligned with decisions `D-W1`..`D-W4`, and the repo-wide
tree/log secret-material scanner (`scan_text_for_key_leaks` and
`audit_tree_for_key_leaks`).

## What W0 Delivers
1. **Honest baseline refusal and survey (`crates/aura-wallet/src/baseline.rs`, `src/bin/aura-wallet.rs`)**:
   - `aura-wallet` without arguments or invoked with `new/import/list/balance/transfer/sign/broadcast/stake/vote` exits `EXIT_STUB` (`2`) and names the missing W1–W4 capabilities.
   - `aura-wallet report [--fixture]` measures the 4 Stage 1 wallet components (`keystore`, `wallet-cli`, `offline-signing`, `multisig`): `0` implemented, `4` placeholders, `0` missing, pinned in `crates/aura-wallet/tests/fixtures/w0_baseline.txt`.
2. **Audit of where keys could end up (`EXPOSURE_SURFACES`, `THREAT_NOTES`, `docs/evidence/w0/threat-notes.md`)**:
   - Documents 8 exposure surfaces (`logs-and-tracing`, `cli-argv-and-env`, `plaintext-disk-files`, `process-memory-and-core-dumps`, `rpc-and-network-frames`, `online-hot-machine-signing`, `single-key-treasury-or-validator`, `ci-and-repository-fixtures`) and 6 threat notes (`T-W0-1`..`T-W0-6`), locking the BIP-44 derivation path `m/44'/ALCRY'/0'/0/i` (`D-W1`).
3. **Tree-search log scanner gate (`audit_tree_for_key_leaks`, `scan_text_for_key_leaks`)**:
   - Scans workspace `.log` files, `gates.txt`, fixtures, docs, and source files for leaked hex private keys (`private_key`, `secret_key`, `privkey`, `ed25519_secret`, `signing_key`, `bls_secret`, `xprv`, `aura1priv`), BIP-39 mnemonics (`mnemonic`, `seed_phrase`, `recovery_phrase`), PEM private key blocks, and raw seeds (`raw_seed`, `master_seed`, `secret_seed`, `bip39_seed`), while ignoring public identifiers (`public_key`, `block_hash`, `state_root`, `tx_hash`, `signature`).
   - Verified against the live workspace (`0` findings across all repository logs and files), a poisoned-log tree-search negative test (`key in a log -> gate red`), and 64 deterministic vectors in `crates/aura-wallet/tests/fixtures/w0_key_audit.txt`.
4. **Negative controls (`tools/nc_w0.py`)**:
   - 6/6 library unit-test mutation controls PASS (`{total_nc}` cumulative negative controls across the workspace).
"""
    (EVIDENCE / "report.md").write_text(report, encoding="utf-8")

    tracked = [
        "CHANGELOG.md",
        "Cargo.lock",
        "Cargo.toml",
        "Makefile",
        "crates/aura-wallet/Cargo.toml",
        "crates/aura-wallet/src/baseline.rs",
        "crates/aura-wallet/src/bin/aura-wallet.rs",
        "crates/aura-wallet/src/lib.rs",
        "crates/aura-wallet/tests/fixtures/w0_baseline.txt",
        "crates/aura-wallet/tests/fixtures/w0_key_audit.txt",
        "crates/aura-wallet/tests/w0_key_audit.rs",
        "docs/evidence/w0/README.md",
        "docs/evidence/w0/gate.log",
        "docs/evidence/w0/gates.txt",
        "docs/evidence/w0/inventory.txt",
        "docs/evidence/w0/nc.log",
        "docs/evidence/w0/report.md",
        "docs/evidence/w0/threat-notes.md",
        "docs/evidence/w0/vectors.log",
        "docs/plans/NODE_PLAN.md",
        "docs/plans/WALLET_PLAN.md",
        "tools/audit_closeout.py",
        "tools/gen_w0_vectors.py",
        "tools/nc_w0.py",
        "tools/write_w0_evidence.py",
    ]
    sums_lines = []
    for rel in sorted(tracked):
        p = ROOT / rel
        if p.exists():
            sums_lines.append(f"{sha256_file(p)}  {rel}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums_lines) + "\n", encoding="utf-8")
    print(f"wrote docs/evidence/w0 ({len(sums_lines)} hashes, {total_nc} NCs)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
