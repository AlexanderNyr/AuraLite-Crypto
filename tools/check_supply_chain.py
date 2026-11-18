#!/usr/bin/env python3
"""H1 supply-chain gate: deterministic, offline (`make check-h1`).

Sub-checks (docs/h1-supply-chain-plan.md D-H1-1..6):
  1. toolchain pin   — rust-toolchain.toml channel must match the oracle
  2. lockfile pin    — Cargo.lock digest + no unpinned git sources
  3. advisory deny   — lockfile must not resolve a denied (crate, version)
  4. SBOM            — sbom.cdx.json digest must match the oracle
  5. secrets         — tools/scan_secrets.py over the tree
  6. signatures      — Ed25519 demo-key verification (fixtures + release
                       manifest docs/evidence/h1/signed-artifacts.txt)

`--selftest` runs the pinned verdict vectors (tools/fixtures/
h1_supply_chain_vectors.json), including hostile inputs that must be
DENIED (the "CI fails on a known vulnerable version" property). The 6
mutation controls in `tools/nc_h1.py` weaken exactly these denials and
require the matching self-test case to go red. Exit 0/1.
"""
from __future__ import annotations

import base64
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from tools import sign_release as sign_mod  # noqa: E402
from tools import scan_secrets as secrets_mod  # noqa: E402

FIX = ROOT / "tools" / "fixtures"
VECTORS = FIX / "h1_supply_chain_vectors.json"
ADVISORIES = FIX / "h1_advisories.json"
LOCK = ROOT / "Cargo.lock"
TOOLCHAIN = ROOT / "rust-toolchain.toml"
SBOM = ROOT / "sbom.cdx.json"
MANIFEST = ROOT / "docs" / "evidence" / "h1" / "signed-artifacts.txt"


def err(msg: str) -> str:
    return f"error: supply-chain: {msg}"


def parse_packages(lock_text: str) -> list[dict]:
    pkgs = []
    for block in lock_text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        if name and version:
            pkgs.append({"name": name.group(1), "version": version.group(1)})
    return pkgs


def _patched(version: str, spec: str) -> bool:
    m = re.match(r">=(\d+)\.(\d+)\.(\d+)$", spec)
    if not m:
        return False
    want = tuple(int(x) for x in m.groups())
    try:
        have = tuple(int(x) for x in version.split(".")[:3])
    except ValueError:
        return False
    return have >= want


def check_toolchain(toml_text: str, expected_channel: str) -> tuple[bool, str]:
    m = re.search(r'channel\s*=\s*"([^"]+)"', toml_text)
    channel = m.group(1) if m else ""
    if channel != expected_channel:
        return False, err(f"toolchain not pinned (channel={channel or 'missing'})")
    return True, ""


def check_lock_digest(lock_text: str, vectors: dict) -> tuple[bool, str]:
    digest = hashlib.sha256(lock_text.encode("utf-8")).hexdigest()
    if digest != vectors["lock"]["sha256"]:
        return False, err(f"lockfile digest mismatch ({digest[:16]}...)")
    git_sources = lock_text.count('source = "git')
    if git_sources != 0:
        return False, err(f"git dependency without a pinned rev ({git_sources})")
    return True, ""


def deny_lock(lock_text: str, advisories: dict) -> list[str]:
    findings = []
    for pkg in parse_packages(lock_text):
        for adv in advisories["advisories"]:
            if pkg["name"] == adv["crate"] and not _patched(pkg["version"], adv["patched"]):
                findings.append(err(f"denied {adv['id']} ({pkg['name']} v{pkg['version']})"))
    return findings


def check_sbom_text(sbom_text: str, vectors: dict) -> tuple[bool, str]:
    digest = hashlib.sha256(sbom_text.encode("utf-8")).hexdigest()
    if digest != vectors["sbom"]["sha256"]:
        return False, err(f"sbom drift ({digest[:16]}...)")
    return True, ""


def check_signatures(vectors: dict) -> list[str]:
    problems = []
    demo = FIX / "h1_release_demo.bin"
    sig = base64.b64decode((FIX / "h1_release_demo.bin.sig").read_text().strip())
    if not sign_mod.verify(demo, sig):
        problems.append(err("bad signature (h1_release_demo.bin)"))
    if MANIFEST.exists():
        for line in MANIFEST.read_text(encoding="utf-8").splitlines():
            if not line.strip() or line.startswith("#"):
                continue
            name, digest_hex, sig_b64 = line.split()
            sig_bytes = base64.b64decode(sig_b64)
            if not sign_mod.verify_digest(bytes.fromhex(digest_hex), sig_bytes):
                problems.append(err(f"bad signature ({name})"))
    return problems


def repo_check(vectors: dict, advisories: dict) -> list[str]:
    problems: list[str] = []
    ok, msg = check_toolchain(TOOLCHAIN.read_text(encoding="utf-8"),
                              vectors["toolchain"]["channel"])
    if not ok:
        problems.append(msg)
    ok, msg = check_lock_digest(LOCK.read_text(encoding="utf-8"), vectors)
    if not ok:
        problems.append(msg)
    problems.extend(deny_lock(LOCK.read_text(encoding="utf-8"), advisories))
    ok, msg = check_sbom_text(SBOM.read_text(encoding="utf-8"), vectors)
    if not ok:
        problems.append(msg)
    for rid, path, lineno in secrets_mod.scan_tree():
        problems.append(err(f"secret rule {rid} matched {path}:{lineno}"))
    problems.extend(check_signatures(vectors))
    return problems


def selftest(vectors: dict, advisories: dict) -> tuple[list[tuple[str, bool, str]], int]:
    results: list[tuple[str, bool, str]] = []
    good_sig = base64.b64decode(vectors["demo_sig"]["sig_b64"])
    forged = base64.b64encode(base64.b64decode(vectors["demo_sig"]["sig_b64"])[:-1]
                              + b"\x00").decode()
    clean_lock = LOCK.read_text(encoding="utf-8")
    vuln_lock = (FIX / "h1_vulnerable.lock").read_text(encoding="utf-8")
    secret_text = (FIX / "h1_secret_leak.txt").read_text(encoding="utf-8")
    sbom_text = SBOM.read_text(encoding="utf-8")

    def run(case: str, expect: str, needle: str, verdict: tuple[bool, str]):
        ok, msg = verdict
        allowed = ok
        if expect == "allow":
            passed = allowed
        else:
            passed = (not allowed) and (needle in msg)
        results.append((case, passed, msg))

    run("toolchain_pinned", "allow", "",
        check_toolchain('channel = "1.99.1"\n', vectors["toolchain"]["channel"]))
    run("toolchain_unpinned_denied", "deny", "toolchain not pinned",
        check_toolchain('channel = "stable"\n', vectors["toolchain"]["channel"]))
    run("clean_lock_allowed", "allow", "",
        (not deny_lock(clean_lock, advisories), ""))
    run("vulnerable_lock_denied", "deny",
        "denied RUSTSEC-TEST-0001 (demo-vulnerable v0.9.0)",
        (not deny_lock(vuln_lock, advisories),
         "; ".join(deny_lock(vuln_lock, advisories))))
    run("lock_digest_mismatch_denied", "deny", "lockfile digest mismatch",
        check_lock_digest(clean_lock + "\n", vectors))
    run("sbom_matches", "allow", "",
        check_sbom_text(sbom_text, vectors))
    run("sbom_drift_denied", "deny", "sbom drift",
        check_sbom_text(sbom_text + " ", vectors))
    planted = secrets_mod.scan_text("h1_secret_leak.txt", secret_text)
    pw = next((f for f in planted if f[0] == "password-assignment"), None)
    run("planted_secret_detected", "deny",
        "secret rule password-assignment matched",
        (not planted,
         err(f"secret rule {pw[0]} matched {pw[1]}:{pw[2]}") if pw else ""))
    clean = secrets_mod.scan_text("clean.txt", "hello world\n")
    run("clean_text_no_secret", "allow", "", (not clean, ""))
    pem_text = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAgEA0demo\n-----END RSA PRIVATE KEY-----\n"
    pem = secrets_mod.scan_text("pem.txt", pem_text)
    run("pem_block_detected", "deny",
        "secret rule private-key-block matched",
        (not pem,
         err(f"secret rule {pem[0][0]} matched {pem[0][1]}:{pem[0][2]}") if pem else ""))
    run("signature_verifies", "allow", "",
        (sign_mod.verify(FIX / "h1_release_demo.bin", good_sig), ""))
    run("forged_signature_denied", "deny", "bad signature",
        (sign_mod.verify(FIX / "h1_release_demo.bin",
                         base64.b64decode(forged)),
         err("bad signature (forged)")))

    passed = sum(1 for _, ok, _ in results if ok)
    return results, passed


def main() -> int:
    vectors = json.loads(VECTORS.read_text(encoding="utf-8"))
    advisories = json.loads(ADVISORIES.read_text(encoding="utf-8"))
    if "--selftest" in sys.argv:
        results, passed = selftest(vectors, advisories)
        for case, ok, msg in results:
            print(f"{'PASS' if ok else 'FAIL'} {case}" + (f": {msg}" if msg and not ok else ""))
        print(f"RESULT: {passed}/{len(results)} supply-chain vectors PASS")
        return 0 if passed == len(results) else 1
    problems = repo_check(vectors, advisories)
    if problems:
        print("\n".join(problems))
        print(f"SUPPLY CHAIN: FAIL — {len(problems)} problem(s)")
        return 1
    print("SUPPLY CHAIN: OK — toolchain pinned, lockfile pinned, "
          "no denied advisories, sbom current, no secrets, signatures verify")
    return 0


if __name__ == "__main__":
    sys.exit(main())
