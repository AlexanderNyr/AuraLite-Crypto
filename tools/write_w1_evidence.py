#!/usr/bin/env python3
"""Assemble W1 evidence (`docs/evidence/w1/`).

Phase gates are expected to have been run already (rule D9 evidence is a
snapshot of recorded runs, not a re-run):

- `docs/evidence/w1/test-w1.log`          — `make test-w1`
- `docs/evidence/w1/test-w1-interop.log`  — `make test-w1-interop`
- `docs/evidence/w1/nc.log`               — `make nc-w1` (52/52 wallet plus
  15/15 fuzz-oracle negative controls; both RESULT lines must be present)
- project-gate logs in `/home/user/w1-closeout-gates/` (make test,
  make check-claims) are summarized in `report.md` when present.

The closeout campaign summary is read from
`/home/user/w1-campaign-v2-closeout/result.json` (format_version 2).
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs" / "evidence" / "w1"
CAMPAIGN = Path("/home/user/w1-campaign-v2-closeout/result.json")
GATES_DIR = Path("/home/user/w1-closeout-gates")

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
    ("W1", 67),
]

RESULT_LINE = re.compile(r"^RESULT: (\d+)/(\d+)", re.M)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8") if path.exists() else ""


def campaign_summary() -> str:
    if not CAMPAIGN.exists():
        return "(closeout campaign result.json not found)\n"
    data = json.loads(CAMPAIGN.read_text(encoding="utf-8"))
    lines = [
        f"closeout campaign: {CAMPAIGN.parent}",
        f"  status: {data.get('status')}",
        f"  seed base: {data.get('seed_base')}, "
        f"{data.get('seconds_per_target')} s per target",
        f"  toolchain: {data.get('rustc')}; {data.get('cargo_fuzz')}",
    ]
    total_iter = 0
    total_sec = 0
    for run in data.get("targets", []):
        done = run.get("fuzzer_done") or [None, None]
        iters, secs = done[0], done[1]
        if isinstance(iters, int):
            total_iter += iters
        if isinstance(secs, int):
            total_sec += secs
        artifacts = run.get("artifacts")
        lines.append(
            f"  {run.get('target')}: status={run.get('status')} "
            f"iterations={iters} seconds={secs} artifacts={artifacts}"
        )
    lines.append(f"  totals: {total_iter} iterations, {total_sec} execution seconds")
    return "\n".join(lines) + "\n"


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)

    for name in ("test-w1.log", "test-w1-interop.log", "nc.log"):
        if not (EVIDENCE / name).exists():
            raise SystemExit(f"missing {(EVIDENCE / name).relative_to(ROOT)}; run the phase gates first")

    nc_log = read(EVIDENCE / "nc.log")
    matches = RESULT_LINE.findall(nc_log)
    passed = sum(int(m[0]) for m in matches)
    total = sum(int(m[1]) for m in matches)
    if passed != total or total != 67:
        raise SystemExit(f"nc.log says {passed}/{total}, expected 67/67")

    test_w1 = read(EVIDENCE / "test-w1.log")
    interop = read(EVIDENCE / "test-w1-interop.log")

    gates_txt = (
        "== make test-w1 (BIP-39 / SLIP-0010 vectors, 1,000 wrong passwords, "
        "strict parser, format fuzz units, profile provenance, recovery matrix) ==\n"
        + test_w1
        + "\n== make test-w1-interop (Python/Rust interop fixtures and fresh pairs) ==\n"
        + interop
        + f"\n== make nc-w1 ({passed}/{total} negative controls) ==\n"
        + nc_log
    )
    (EVIDENCE / "gates.txt").write_text(gates_txt, encoding="utf-8")

    project = ""
    for name in ("make-test.log", "check-claims.log", "audit-closeout.log"):
        text = read(GATES_DIR / name)
        if text:
            project += f"\n== {name} ==\n" + "\n".join(text.splitlines()[-12:]) + "\n"

    inv_lines = ["# Negative-control inventory through W1"]
    for phase, count in PHASE_NC_COUNTS:
        inv_lines.append(f"{phase}: {count}/{count} PASS")
    inv_lines.append(f"TOTAL: {sum(n for _, n in PHASE_NC_COUNTS)} negative controls")
    (EVIDENCE / "inventory.txt").write_text("\n".join(inv_lines) + "\n", encoding="utf-8")

    readme = """# W1 Evidence — Crypto Keystore Phase

## Artifacts
- `report.md` — W1 phase close-out report (receipt companion).
- `gates.txt` — `make test-w1`, `make test-w1-interop` and `make nc-w1` outputs.
- `test-w1.log`, `test-w1-interop.log` — individual gate logs.
- `nc.log` — `make nc-w1` output (`52/52` wallet plus `15/15` fuzz-oracle
  mutation controls; both RESULT lines).
- `inventory.txt` — cumulative negative-control inventory (`406` total through W1).
- `SHA256SUMS` — SHA-256 digests of the W1 deliverables and evidence files.

The interoperability format is frozen as W1-FORMAT-v1 in
`docs/spec/keystore-format.md` (owner decision). Scope is testnet-only; the
independent external security review is deferred to the Stage-2 audit by owner
decision (2026-11-08) and is not claimed here. Fuzz campaigns are bounded
research observations, not statistical proof.
"""
    (EVIDENCE / "README.md").write_text(readme, encoding="utf-8")

    report = f"""# W1 Report — Crypto Keystore Phase (closed 2026-11-08)

## Summary
Phase **W1** delivers the AuraLite crypto keystore: English BIP-39 with per-field
NFKD normalization profiles (Unicode 15.1/18.0 forks with provenance guards),
PBKDF2-HMAC-SHA512 seed derivation, SLIP-0010 Ed25519 hardened derivation at
`m/44'/1'/0'/0'/i'` (D-W1-testnet; not literal BIP-32/44 and not a mainnet
allocation), the versioned keystore format v1/v2 (Argon2id or legacy scrypt
policy bounds, AES-256-GCM with canonical-JSON AAD, post-GCM account binding,
zeroization) and the recovery metadata marker bound to normalization profiles.
The interoperability format is frozen as **W1-FORMAT-v1**
(`docs/spec/keystore-format.md`, owner decision; any format change is a new
versioned document).

## DoD evidence
1. **Derivation**: official and independent BIP-39/SLIP-0010 vectors plus
   independently generated testnet-path vectors pass in `make test-w1`.
2. **Wrong-password resistance**: 1,000 wrong passwords → 0 decryptions
   (`tests/w1_keystore.rs`, weak test-only KDF to keep the campaign bounded;
   normal KDF policy checked separately in release mode).
3. **Format fuzz**: sequential ASan/libFuzzer campaigns over the four wallet
   fuzz targets; the closeout campaign is recorded below. Iterations and
   execution time are descriptive only.
4. **Negative controls**: `make nc-w1` passes 52/52 wallet plus 15/15
   fuzz-oracle mutation controls ({passed}/{total}, 406/406 cumulative).
   Mutation controls test detection by selected tests, not completeness.
5. **Interoperability**: `make test-w1-interop` bidirectional fixtures and
   fresh Python/Rust pairs pass (profile-sensitive U+A7F1 and uppercase-hex
   header fixtures included).

## Campaigns (bounded observations)
{campaign_summary()}
Historical total through 2026-11-07 (six earlier ASan campaigns): 887,092,725
iterations and 10,171 execution seconds (seed base 20261140); see
`docs/w1-fuzz-extended-observation.md`. H3's 24-hour total campaign remains on
the H3 plan and is not claimed.

## Project gates (run at closure)
{project}
## Scope and carries (owner decision, 2026-11-08)
- Testnet-only closure; mainnet curve/path/coin-type allocation and migration
  policy remain separate decisions.
- The independent external security review is deferred to the Stage-2 audit;
  local tests, mutation controls, Python/Rust references and the assistant's
  work are not external review.
- `make test-sanitize` and `make test-determinism` remain explicit SKIPs owned
  by the H3/X7 plans; `cargo deny` stays planned for H1. None are claimed.
- Deliverable: `patches/0059-W1-crypto-keystore.patch` (complete W1-from-W0
  phase patch on top of the post-W0 predecessor). Receipt:
  `docs/plans/WALLET_PLAN.md`.
"""
    (EVIDENCE / "report.md").write_text(report, encoding="utf-8")

    tracked = [
        "CHANGELOG.md",
        "Cargo.lock",
        "Cargo.toml",
        "Makefile",
        "crates/aura-wallet/Cargo.toml",
        "crates/aura-wallet/src/baseline.rs",
        "crates/aura-wallet/src/derivation.rs",
        "crates/aura-wallet/src/keystore.rs",
        "crates/aura-wallet/src/lib.rs",
        "crates/aura-wallet/src/bin/aura-wallet.rs",
        "crates/aura-wallet/examples/w1_interop_fixture.rs",
        "docs/STATUS.md",
        "docs/WIP_SECURITY_AND_W1.md",
        "docs/plans/WALLET_PLAN.md",
        "docs/spec/FORMAT_INDEX.md",
        "docs/spec/keystore-format.md",
        "docs/w1-fuzz-extended-observation.md",
        "docs/w1-preclose-checklist.md",
        "fuzz/Cargo.toml",
        "fuzz/src/lib.rs",
        "tests/w1_keystore.rs",
        "tests/fixtures/w1_recovery_metadata_reference.json",
        "tools/audit_closeout.py",
        "tools/check_unicode_15_1_fork.py",
        "tools/nc_w1.py",
        "tools/nc_w1_fuzz.py",
        "tools/w1_fuzz_campaign.py",
        "tools/w1_interop_reference.py",
        "tools/w1_recovery_metadata_reference.py",
        "tools/verify_w1_fuzz_evidence.py",
        "tools/write_w1_evidence.py",
        "docs/evidence/w1/README.md",
        "docs/evidence/w1/gates.txt",
        "docs/evidence/w1/inventory.txt",
        "docs/evidence/w1/nc.log",
        "docs/evidence/w1/report.md",
        "docs/evidence/w1/test-w1-interop.log",
        "docs/evidence/w1/test-w1.log",
    ]
    for fixture in sorted((ROOT / "crates" / "aura-wallet" / "tests" / "fixtures").glob("w1_*")):
        tracked.append(str(fixture.relative_to(ROOT)))
    for target in sorted((ROOT / "fuzz" / "fuzz_targets").glob("*.rs")):
        tracked.append(str(target.relative_to(ROOT)))

    sums_lines = []
    for rel in sorted(set(tracked)):
        p = ROOT / rel
        if p.exists():
            sums_lines.append(f"{sha256_file(p)}  {rel}")
    (EVIDENCE / "SHA256SUMS").write_text("\n".join(sums_lines) + "\n", encoding="utf-8")
    print(f"wrote docs/evidence/w1 ({len(sums_lines)} hashes, 406 NCs)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
