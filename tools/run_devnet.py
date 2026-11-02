#!/usr/bin/env python3
"""Run the 4-validator local aurad devnet (M1 acceptance scenario)."""
from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CHAIN_ID = "auralite-devnet-1"
BLS_SEEDS = [bytes([11]) * 32, bytes([22]) * 32, bytes([33]) * 32, bytes([44]) * 32]


def reserve_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def http_get(addr: str, path: str) -> tuple[int, str]:
    req = urllib.request.Request(f"http://{addr}{path}", method="GET")
    with urllib.request.urlopen(req, timeout=5) as resp:
        return resp.status, resp.read().decode("utf-8")


def rpc_call(addr: str, method: str, params: object) -> object:
    payload = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    ).encode("utf-8")
    req = urllib.request.Request(
        f"http://{addr}/",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=5) as resp:
        body = json.loads(resp.read().decode("utf-8"))
    if "error" in body and body["error"] is not None:
        raise RuntimeError(f"RPC {method} error on {addr}: {body['error']}")
    return body["result"]


def trigger_round(devnet_dir: Path, round_num: int, online: list[int], timeout: float = 10.0) -> None:
    wire_dir = devnet_dir / "wire"
    wire_dir.mkdir(parents=True, exist_ok=True)
    req_path = wire_dir / f"round-{round_num}.req"
    tmp_path = wire_dir / f"round-{round_num}.tmp"
    tmp_path.write_bytes(round_num.to_bytes(8, "little"))
    tmp_path.replace(req_path)

    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        done = sum(
            1 for idx in online if (wire_dir / f"round-{round_num}-done-{idx}.ok").exists()
        )
        if done == len(online):
            return
        time.sleep(0.005)
    raise TimeoutError(f"Timed out waiting for round {round_num} across {online}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Run 4-validator aurad devnet (M1)")
    parser.add_argument(
        "--rounds",
        type=int,
        default=103,
        help="Number of 4-validator rounds before 1-validator outage test (default: 103)",
    )
    parser.add_argument(
        "--outage-rounds",
        type=int,
        default=12,
        help="Number of 3-of-4 quorum rounds after stopping validator-3 (default: 12)",
    )
    parser.add_argument(
        "--evidence-dir",
        type=Path,
        default=None,
        help="Optional directory to write validator-0..3.log excerpts",
    )
    args = parser.parse_args()

    subprocess.run(
        ["cargo", "build", "-p", "aura-node", "--bin", "aurad"],
        cwd=REPO,
        check=True,
    )
    aurad_bin = REPO / "target/debug/aurad"
    if not aurad_bin.exists():
        raise FileNotFoundError(f"aurad binary not found at {aurad_bin}")

    devnet_dir = Path(tempfile.mkdtemp(prefix="aura-devnet-run-"))
    processes: list[subprocess.Popen[bytes]] = []
    rpc_addrs: list[str] = []
    log_paths: list[Path] = []
    log_files = []

    try:
        for idx in range(4):
            node_dir = devnet_dir / f"validator-{idx}"
            node_dir.mkdir(parents=True, exist_ok=True)
            key_path = node_dir / "validator.key"
            key_path.write_bytes(BLS_SEEDS[idx])

            rpc_addr = f"127.0.0.1:{reserve_port()}"
            grpc_addr = f"127.0.0.1:{reserve_port()}"
            log_path = devnet_dir / f"validator-{idx}.log"
            lf = log_path.open("wb")
            log_files.append(lf)

            env = os.environ.copy()
            env["AURA_DEVNET_DIR"] = str(devnet_dir)
            env["AURA_VALIDATOR_INDEX"] = str(idx)

            proc = subprocess.Popen(
                [
                    str(aurad_bin),
                    "--chain-id",
                    CHAIN_ID,
                    "--data-dir",
                    str(node_dir),
                    "--rpc-addr",
                    rpc_addr,
                    "--grpc-addr",
                    grpc_addr,
                    "--validator-key",
                    str(key_path),
                ],
                cwd=REPO,
                env=env,
                stdout=lf,
                stderr=lf,
            )
            processes.append(proc)
            rpc_addrs.append(rpc_addr)
            log_paths.append(log_path)

        # Wait for all 4 validators to report healthy.
        deadline = time.monotonic() + 15.0
        for idx, addr in enumerate(rpc_addrs):
            while True:
                try:
                    status, body = http_get(addr, "/health")
                    if status == 200 and '"status":"healthy"' in body:
                        break
                except Exception:
                    pass
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"validator-{idx} ({addr}) did not become healthy")
                time.sleep(0.025)

        print(f"DEVNET: 4 aurad validators online on {rpc_addrs}")

        for r in range(1, args.rounds + 1):
            trigger_round(devnet_dir, r, [0, 1, 2, 3])

        heads = [rpc_call(addr, "get_block", None) for addr in rpc_addrs]
        h0 = heads[0]["height"]
        hash0 = heads[0]["hash"]
        root0 = heads[0]["stateRoot"]
        assert h0 >= 100, f"expected height >= 100, got {h0}"
        for idx, head in enumerate(heads):
            assert head["height"] == h0, f"validator-{idx} height mismatch"
            assert head["hash"] == hash0, f"validator-{idx} block_hash mismatch"
            assert head["stateRoot"] == root0, f"validator-{idx} state_root mismatch"

        print(
            f"DEVNET 4/4 FINALIZED: height={h0} block_hash={hash0} state_root={root0}"
        )

        # Stop validator-3 and run outage rounds across remaining 3 validators.
        processes[3].send_signal(signal.SIGTERM)
        processes[3].wait(timeout=5)
        for r in range(args.rounds + 1, args.rounds + args.outage_rounds + 1):
            trigger_round(devnet_dir, r, [0, 1, 2])

        post_heads = [rpc_call(rpc_addrs[i], "get_block", None) for i in range(3)]
        post_h0 = post_heads[0]["height"]
        post_hash0 = post_heads[0]["hash"]
        post_root0 = post_heads[0]["stateRoot"]
        assert post_h0 > h0, "height did not advance with 1 of 4 validators stopped"
        for idx, head in enumerate(post_heads):
            assert head["height"] == post_h0
            assert head["hash"] == post_hash0
            assert head["stateRoot"] == post_root0

        print(
            f"DEVNET 3/4 OUTAGE SURVIVED: height={post_h0} block_hash={post_hash0} state_root={post_root0}"
        )

        for idx in range(3):
            if processes[idx].poll() is None:
                processes[idx].send_signal(signal.SIGTERM)
                processes[idx].wait(timeout=5)

        for lf in log_files:
            lf.flush()
            lf.close()
        log_files.clear()

        if args.evidence_dir is not None:
            args.evidence_dir.mkdir(parents=True, exist_ok=True)
            for idx, lp in enumerate(log_paths):
                lines = lp.read_text(encoding="utf-8").splitlines()
                # Keep startup + first 5 rounds + round 100..103 + outage rounds + shutdown
                selected: list[str] = []
                for line in lines:
                    if (
                        "aurad running" in line
                        or "aurad stopped" in line
                        or '"round":1,' in line
                        or '"round":2,' in line
                        or '"round":3,' in line
                        or '"round":100,' in line
                        or '"round":101,' in line
                        or '"round":102,' in line
                        or '"round":103,' in line
                        or '"round":114,' in line
                        or '"round":115,' in line
                    ):
                        selected.append(line)
                (args.evidence_dir / f"validator-{idx}.log").write_text(
                    "\n".join(selected) + "\n", encoding="utf-8"
                )
        return 0
    finally:
        for lf in log_files:
            try:
                lf.close()
            except Exception:
                pass
        for proc in processes:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
        shutil.rmtree(devnet_dir, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
