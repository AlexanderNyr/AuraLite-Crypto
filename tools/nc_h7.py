#!/usr/bin/env python3
"""Mutation controls for H7 (docs/h7-opsec-plan.md §5).

Each control injects ONE bug (restored after) and requires the named H7
gate to go RED with the named message. NC-1 and NC-2 are the phase gate
literally: a secret in git must turn CI red, and two hosts must build to
the same sha256. The secret literal is assembled at run time so this file
itself never trips the scanner.
"""

import hashlib
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLAN = os.path.join(ROOT, "docs", "h7-opsec-plan.md")
REPRO = os.path.join(ROOT, "tools", "h7_repro_build.py")
CI = os.path.join(ROOT, ".github", "workflows", "ci.yml")
KEYSTORE = os.path.join(ROOT, "crates", "aura-wallet/src/keystore.rs")
TOOLCHAIN = os.path.join(ROOT, "rust-toolchain.toml")

FAKE_SECRET = "AKIA" + "IOSFODNN7EXAMPLE"

CHECK_OPSec = [sys.executable, os.path.join(ROOT, "tools", "check_opsec.py")]
REPRO_CMD = [sys.executable, os.path.join(ROOT, "tools", "h7_repro_build.py")]
WALLET_CMD = ["cargo", "test", "--locked", "-p", "aura-wallet",
              "--features", "insecure-test-kdf", "--test", "h7_opsec"]

# (name, path, anchor, replacement, test cmd, expected message)
CONTROLS = [
    (
        "NC-1 injected bug -> secret in git must turn CI red",
        PLAN,
        "<!-- H7 NC secret anchor -->\n",
        "<!-- H7 NC secret anchor -->\n%s\n" % FAKE_SECRET,
        CHECK_OPSec,
        "H7: secret in git must turn CI red",
    ),
    (
        "NC-2 two hosts diverge when the remap stops being shared",
        REPRO,
        '    remap = "--remap-path-prefix=%s=%s" % (host_dir, VIRTUAL_PREFIX)\n',
        '    remap = "--remap-path-prefix=%s=%s-%s" % (host_dir, VIRTUAL_PREFIX,\n'
        "                                               os.path.basename(host_dir))\n",
        REPRO_CMD,
        "H7: two hosts must build to the same sha256",
    ),
    (
        "NC-3 workflow permissions block removed",
        CI,
        "permissions:\n  contents: read\n",
        "x-permissions:\n  contents: read\n",
        CHECK_OPSec,
        "H7: CI must pin permissions and toolchain",
    ),
    (
        "NC-4 keystore save mode widened to group-readable",
        KEYSTORE,
        "                Mode::RUSR | Mode::WUSR,\n",
        "                Mode::RUSR | Mode::WUSR | Mode::RGRP,\n",
        WALLET_CMD + ["h7_keystore_owner_only", "--", "--exact"],
        "H7: validator keys must stay owner-only",
    ),
    (
        "NC-5 loader permission check disabled",
        KEYSTORE,
        "                & 0o077\n",
        "                & 0o000\n",
        WALLET_CMD + ["h7_keystore_owner_only", "--", "--exact"],
        "H7: keystore loader must reject loose permissions",
    ),
    (
        "NC-6 toolchain pin removed",
        TOOLCHAIN,
        'channel = "1.99.1"\n',
        'channel = "stable"\n',
        CHECK_OPSec,
        "H7: toolchain must stay pinned",
    ),
    (
        "NC-7 access log record format leaks extra fields",
        KEYSTORE,
        '    format!("keystore {op} file={name} ok={ok}")\n',
        '    format!("keystore {op} file={name} ok={ok} secret=leak")\n',
        WALLET_CMD + ["h7_access_log_without_secrets", "--", "--exact"],
        "H7: key access must be logged without secrets",
    ),
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def run_control(name, path, anchor, replacement, cmd, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
    compile_failed = "error[" in output and "test result" not in output
    if compile_failed:
        return False, "mutation broke compilation"
    passed_markers = ("test result: ok.",
                      "RESULT: 3/3 opsec checks PASS",
                      "RESULT: H7 two-host reproducible build PASS")
    if any(marker in output for marker in passed_markers):
        return False, "mutated gate still passes"
    if expected in output:
        return True, "red with named message"
    return False, "red without named message"


def main():
    passed = 0
    for name, path, anchor, replacement, cmd, expected in CONTROLS:
        ok, note = run_control(name, path, anchor, replacement, cmd, expected)
        line = f"[{'PASS' if ok else 'FAIL'}] {name}: {note}"
        print(line, flush=True)
        passed += bool(ok)
    print("RESULT: %d/%d negative controls PASS" % (passed, len(CONTROLS)),
          flush=True)
    print("control-anchors sha256: keystore=%s repro=%s"
          % (sha256_file(KEYSTORE)[:16], sha256_file(REPRO)[:16]), flush=True)
    return 0 if passed == len(CONTROLS) else 1


if __name__ == "__main__":
    sys.exit(main())
