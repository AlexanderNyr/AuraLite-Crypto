#!/usr/bin/env python3
"""E9 emulator lane: a deterministic CI run of `aurad` (ECOSYSTEM_PLAN
E9 — "CI run of `aurad` in AuraLite-Emulator", "emulator lane green").

AuraLite-Emulator is the external emulator repository; this harness is
the in-repo lane runner (deviation recorded in
`docs/ecosystem/e9-bridge-report.md`) that exercises the same contract
here:

  1. `aurad --version`                 -> exit 0
  2. `aurad report --fixture`          -> exit 0 (deterministic report)
  3. `aurad` without `--chain-id`      -> exit 2 (startup refusal)
  4. bounded node run: start listeners on free ports, wait for the RPC
     port to accept, SIGTERM -> clean exit 0 (D6 shutdown path)

Every step's output lands in `docs/evidence/e9/lane.log`; the lane
prints `E9-LANE-OK 4` when all four steps are green and exits non-zero
otherwise. Registered in CI via `make emulator-lane` (part of
`make test`).
"""

from __future__ import annotations

import os
import pathlib
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs" / "evidence" / "e9"
STEPS = 4


def find_aurad() -> list[str]:
    built = ROOT / "target" / "debug" / "aurad"
    if built.exists():
        return [str(built)]
    cargo = os.environ.get("CARGO", shutil.which("cargo") or "cargo")
    build = subprocess.run(
        [cargo, "build", "--locked", "-p", "aura-node", "--bin", "aurad"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if build.returncode != 0 or not built.exists():
        print(build.stdout + build.stderr, file=sys.stderr)
        raise SystemExit("E9-LANE-BAD: cannot build aurad")
    return [str(built)]


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def main() -> int:
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    aurad = find_aurad()
    log: list[str] = []
    ok = 0

    def step(name: str, args: list[str], expect: int) -> subprocess.CompletedProcess:
        proc = subprocess.run(args, cwd=ROOT, capture_output=True, text=True,
                              timeout=120)
        status = "PASS" if proc.returncode == expect else "FAIL"
        log.append(f"[{status}] {name}: exit={proc.returncode} (want {expect})")
        log.append(proc.stdout[-2000:])
        log.append(proc.stderr[-2000:])
        if proc.returncode != expect:
            raise SystemExit(f"E9-LANE-BAD [{name}] exit {proc.returncode}")
        return proc

    # 1. version
    step("aurad --version", aurad + ["--version"], 0)
    ok += 1

    # 2. deterministic report
    step("aurad report --fixture", aurad + ["report", "--fixture"], 0)
    ok += 1

    # 3. startup refusal without a chain id
    step("aurad without --chain-id", aurad, 2)
    ok += 1

    # 4. bounded node run with clean SIGTERM shutdown
    rpc, grpc = free_port(), free_port()
    with tempfile.TemporaryDirectory(prefix="e9-lane-") as data_dir:
        proc = subprocess.Popen(
            aurad + [
                "--chain-id", "aura-e9-lane",
                "--data-dir", data_dir,
                "--rpc-addr", f"127.0.0.1:{rpc}",
                "--grpc-addr", f"127.0.0.1:{grpc}",
            ],
            cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        up = False
        for _ in range(120):
            try:
                with socket.create_connection(("127.0.0.1", rpc), timeout=0.25):
                    up = True
                    break
            except OSError:
                if proc.poll() is not None:
                    break
                time.sleep(0.25)
        log.append(f"[{'PASS' if up else 'FAIL'}] node start: rpc 127.0.0.1:{rpc} up={up}")
        if not up:
            proc.kill()
            out, err = proc.communicate(timeout=30)
            (EVIDENCE / "lane.log").write_text(
                "\n".join(log) + out + err, encoding="utf-8"
            )
            raise SystemExit("E9-LANE-BAD: node did not start")
        proc.send_signal(signal.SIGTERM)
        out, err = proc.communicate(timeout=60)
        log.append(f"[{'PASS' if proc.returncode == 0 else 'FAIL'}] "
                   f"SIGTERM shutdown: exit={proc.returncode}")
        log.append(out[-2000:])
        log.append(err[-2000:])
        if proc.returncode != 0:
            (EVIDENCE / "lane.log").write_text("\n".join(log), encoding="utf-8")
            raise SystemExit("E9-LANE-BAD: unclean shutdown")
    ok += 1

    (EVIDENCE / "lane.log").write_text("\n".join(log) + "\n", encoding="utf-8")
    print(f"E9-LANE-OK {ok}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
