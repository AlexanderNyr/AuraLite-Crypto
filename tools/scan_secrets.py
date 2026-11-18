#!/usr/bin/env python3
"""Deterministic secrets scanner (gitleaks class) for the H1 supply-chain gate.

Fixed rule set (docs/h1-supply-chain-plan.md D-H1-4). Scans text files under
the repository; intentional fixtures and the rule definitions themselves are
allow-listed. The immutable `patches/` review archive is out of scope: its
diffs embed the demo-fixture lines of the phases they record, and those
strings are scanned at their canonical live paths instead (post-closeout
deviation in docs/h1-supply-chain-plan.md; guarded by
tools/test_patch_scope_guard.py). Findings are reported as
`secret rule <id> matched <path>:<line>`. Never run this on repos that must
keep live secrets; this repo is public-test material only.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

RULES: list[tuple[str, str, bool]] = [
    # (id, pattern, restricted-to-code-ext)
    ("private-key-block", r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----", False),
    ("aws-access-key", r"\bAKIA[0-9A-Z]{16}\b", False),
    ("github-token", r"\bgh[pousr]_[A-Za-z0-9]{20,}\b", False),
    ("password-assignment",
     r"\b(?:password|passwd|pwd)\s*[=:]\s*[\"'][^\"']{8,}[\"']", True),
    ("bearer-token", r"(?i)\bbearer\s+[A-Za-z0-9\-._~+/]{20,}", False),
]

CODE_EXT = {".rs", ".py", ".toml", ".json", ".yml", ".yaml", ".env", ".sh",
            ".txt", ".lock"}

ALLOWLIST = {
    "tools/scan_secrets.py",
    "tools/check_supply_chain.py",
    "tools/nc_h1.py",
    "tools/write_h0_evidence.py",
    "tools/write_h1_evidence.py",
    "tools/gen_h1_vectors.py",
    "tools/fixtures/h1_secret_leak.txt",
    "docs/h1-supply-chain-plan.md",
}

# "patches": immutable review archive; diffs necessarily embed past demo
# fixture lines (e.g. tools/fixtures/h1_secret_leak.txt inside patches/0064).
SKIP_DIRS = {".git", "target", "node_modules", "__pycache__", ".arena", "patches"}


def scan_text(path: str, text: str) -> list[tuple[str, str, int]]:
    findings: list[tuple[str, str, int]] = []
    if path in ALLOWLIST:
        return findings
    restricted_ok = Path(path).suffix in CODE_EXT
    lines = text.splitlines()
    for rid, pattern, restricted in RULES:
        if restricted and not restricted_ok:
            continue
        rx = re.compile(pattern)
        for lineno, line in enumerate(lines, start=1):
            if rx.search(line):
                findings.append((rid, path, lineno))
    return findings


def scan_tree(root: Path = ROOT) -> list[tuple[str, str, int]]:
    findings: list[tuple[str, str, int]] = []
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if any(part in SKIP_DIRS for part in path.relative_to(root).parts):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        findings.extend(scan_text(rel, text))
    return findings


def main() -> int:
    findings = scan_tree()
    for rid, path, lineno in findings:
        print(f"error: supply-chain: secret rule {rid} matched {path}:{lineno}")
    if findings:
        print(f"SECRETS: FAIL — {len(findings)} finding(s)")
        return 1
    print("SECRETS: OK — no findings")
    return 0


if __name__ == "__main__":
    sys.exit(main())
