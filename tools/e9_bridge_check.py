#!/usr/bin/env python3
"""E9 bridge deterministic model check (ECOSYSTEM_PLAN E9).

Independent Python model for `crates/aura-storage/src/bridge.rs`
(ed25519 via `cryptography`, RFC 8032 — the same signatures the Rust
side makes with `aura_crypto::sign_raw`). It replays the pinned
scenario: the remote chain emulator lane (headers 1..5 committed by
validators 0,1,2), the MITM attack lane (forged header → reject), the
limits/pause/replay protections, the reserve invariant and the OS
client's finalized-checkpoint check, then prints exactly the two pinned
lines the Rust example `e9_bridge_example.rs` prints:

    E9-STEPS-OK 25
    E9-SIM-OK {"steps": 25, ...}

Writes `sim/e9-bridge.json`.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)

REPO = Path(__file__).resolve().parents[1]

ONE = 10**18
PER_TX = 1_000 * ONE
DAILY = 1_200 * ONE
QUORUM_NUM, QUORUM_DEN = 2, 3  # strictly more than 2/3

SCENARIO_DIGEST = "039d498ba6fcd17e7640094492283a94be2c3447f8ebec89f9306eb438339580"
AGGREGATE_DIGEST = "4f8d9af363ab88fb7a6ce0ef6af52facb886f5e123ca0a128d1693e06d575e4f"


class E9Error(Exception):
    pass


def sha256_32(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def hexs(b: bytes) -> str:
    return b.hex()


def validator_seed(vid: int) -> bytes:
    return sha256_32(f"aura-e9-validator-{vid}".encode())


def attacker_seed() -> bytes:
    return sha256_32(b"aura-e9-attacker")


class Header:
    def __init__(self, height, parent: bytes, root: bytes, time):
        self.height = height
        self.parent = parent
        self.root = root
        self.time = time

    def canonical(self) -> str:
        return f"e9h|{self.height}|{hexs(self.parent)}|{hexs(self.root)}|{self.time}"

    def hash(self) -> bytes:
        return sha256_32(self.canonical().encode())


def make_header(height: int, parent: bytes) -> Header:
    return Header(
        height, parent, sha256_32(f"e9-root-{height}".encode()), height * 10
    )


def sign(seed: bytes, msg: bytes) -> bytes:
    return Ed25519PrivateKey.from_private_bytes(seed).sign(msg)


def pubkey_of(seed: bytes) -> bytes:
    return (
        Ed25519PrivateKey.from_private_bytes(seed)
        .public_key()
        .public_bytes_raw()
    )


def verify(pk: bytes, msg: bytes, sig: bytes) -> bool:
    try:
        Ed25519PublicKey.from_public_bytes(pk).verify(sig, msg)
        return True
    except Exception:
        return False


VALIDATORS = {i: (10, pubkey_of(validator_seed(i))) for i in range(4)}


def required_power() -> int:
    total = sum(p for p, _ in VALIDATORS.values())
    return total * QUORUM_NUM // QUORUM_DEN + 1


def verify_commit(header: Header, commit) -> None:
    h = header.hash()
    seen: set[int] = set()
    signed = 0
    for vid, sig in sorted(commit, key=lambda x: x[0]):
        if vid not in VALIDATORS:
            raise E9Error(f"E9: forged header signature ({vid})")
        if vid in seen:
            raise E9Error(f"E9: forged header signature ({vid})")
        seen.add(vid)
        power, pk = VALIDATORS[vid]
        if not verify(pk, h, sig):
            raise E9Error(f"E9: forged header signature ({vid})")
        signed += power
    req = required_power()
    if signed < req:
        raise E9Error(f"E9: commit lacks quorum ({signed}/{req})")


class LightClient:
    def __init__(self, genesis: Header):
        self.trusted_height = genesis.height
        self.trusted_hash = genesis.hash()
        self.roots = {genesis.height: genesis.root}

    def update(self, header: Header, commit) -> None:
        if header.height <= self.trusted_height:
            raise E9Error(
                f"E9: header not newer ({header.height}/{self.trusted_height})"
            )
        if header.parent != self.trusted_hash:
            raise E9Error(f"E9: header does not chain ({header.height})")
        verify_commit(header, commit)
        self.trusted_height = header.height
        self.trusted_hash = header.hash()
        self.roots[header.height] = header.root


class Bridge:
    def __init__(self, genesis: Header):
        self.lc = LightClient(genesis)
        self.paused = False
        self.day = 1
        self.spent = 0
        self.processed: set[int] = set()
        self.locked = 0
        self.minted = 0

    def update_header(self, header, commit):
        self.lc.update(header, commit)

    def submit(self, msg, height: int, root: bytes) -> None:
        mid, kind, amount = msg
        if self.paused:
            raise E9Error("E9: bridge is paused")
        if mid in self.processed:
            raise E9Error(f"E9: message already processed ({mid})")
        if amount == 0:
            raise E9Error("E9: amount must be positive")
        if height > self.lc.trusted_height:
            raise E9Error(f"E9: checkpoint not finalized ({height})")
        if self.lc.roots.get(height) != root:
            raise E9Error(f"E9: message root mismatch ({height})")
        if amount > PER_TX:
            raise E9Error(
                f"E9: transfer exceeds per-tx limit ({amount}/{PER_TX})"
            )
        remaining = max(0, DAILY - self.spent)
        if amount > remaining:
            raise E9Error(
                f"E9: transfer exceeds daily limit ({amount}/{remaining})"
            )
        if kind == "withdraw":
            if amount > self.locked:
                raise E9Error(
                    f"E9: withdraw exceeds reserve ({amount}/{self.locked})"
                )
            self.locked -= amount
            self.minted -= amount
        else:
            self.locked += amount
            self.minted += amount
        self.processed.add(mid)
        self.spent += amount


class OsClient:
    def __init__(self, height: int, root: bytes):
        self.checkpoint_height = height
        self.checkpoint_root = root

    def accept(self, height: int) -> None:
        if height > self.checkpoint_height:
            raise E9Error(f"E9: checkpoint not finalized ({height})")


def signed_commit(signers, header: Header, flip=None) -> list:
    h = header.hash()
    commit = []
    for vid in signers:
        sig = sign(validator_seed(vid), h)
        if flip == vid:
            sig = bytes([sig[0] ^ 0x01]) + sig[1:]
        commit.append((vid, sig))
    return sorted(commit, key=lambda x: x[0])


def run_scenario():
    steps: list[str] = []
    forged_rejected = 0

    genesis = make_header(0, b"\x00" * 32)
    bridge = Bridge(genesis)
    trusted_hash = genesis.hash()
    headers = {}

    for height in range(1, 6):
        header = make_header(height, trusted_hash)
        bridge.update_header(header, signed_commit([0, 1, 2], header))
        trusted_hash = header.hash()
        headers[height] = header
        steps.append(f"header|{height}|ok")

    try:
        bridge.update_header(headers[5], signed_commit([0, 1, 2], headers[5]))
        raise AssertionError("stale header must fail")
    except E9Error as e:
        steps.append(f"header|5|err|{e}")

    h6 = make_header(6, trusted_hash)
    try:
        bridge.update_header(h6, signed_commit([0, 1], h6))
        raise AssertionError("under quorum must fail")
    except E9Error as e:
        steps.append(f"header|6|err|{e}")

    forged_a = signed_commit([0, 1, 2], h6) + [(99, sign(attacker_seed(), h6.hash()))]
    try:
        bridge.update_header(h6, forged_a)
        raise AssertionError("unknown signer must fail")
    except E9Error as e:
        steps.append(f"forged|6|err|{e}")
        forged_rejected += 1

    try:
        bridge.update_header(h6, signed_commit([0, 1, 2], h6, flip=1))
        raise AssertionError("bad signature must fail")
    except E9Error as e:
        steps.append(f"forged|6|err|{e}")
        forged_rejected += 1

    h7 = make_header(7, bytes([7] * 32))
    try:
        bridge.update_header(h7, signed_commit([0, 1, 2], h7))
        raise AssertionError("fork must fail")
    except E9Error as e:
        steps.append(f"chain|7|err|{e}")

    root3, root4, root5 = headers[3].root, headers[4].root, headers[5].root

    def submit_or_err(step, msg, height, root):
        try:
            bridge.submit(msg, height, root)
            steps.append(f"{step}|ok")
        except E9Error as e:
            steps.append(f"{step}|err|{e}")

    submit_or_err("deposit|1|500000000000000000000", (1, "deposit", 500 * ONE), 3, root3)
    submit_or_err("deposit|2|400000000000000000000", (2, "deposit", 400 * ONE), 4, root4)
    submit_or_err("withdraw|3|100000000000000000000", (3, "withdraw", 100 * ONE), 5, root5)
    submit_or_err("deposit|1|500000000000000000000", (1, "deposit", 500 * ONE), 3, root3)
    submit_or_err("deposit|4|5000000000000000000000", (4, "deposit", 5000 * ONE), 5, root5)
    submit_or_err("deposit|5|400000000000000000000", (5, "deposit", 400 * ONE), 5, root5)
    submit_or_err("deposit|8|10000000000000000000", (8, "deposit", 10 * ONE), 3, root4)

    bridge.paused = True
    steps.append("pause|ok")
    submit_or_err("deposit|6|10000000000000000000", (6, "deposit", 10 * ONE), 5, root5)
    bridge.paused = False
    steps.append("unpause|ok")

    os = OsClient(bridge.lc.trusted_height, bridge.lc.roots[bridge.lc.trusted_height])
    try:
        os.accept(5)
        steps.append("os|5|ok")
    except E9Error as e:
        steps.append(f"os|5|err|{e}")
    try:
        os.accept(6)
        raise AssertionError("non-finalized must fail")
    except E9Error as e:
        steps.append(f"os|6|err|{e}")

    bridge.day = 2
    bridge.spent = 0
    steps.append("day|2|ok")
    submit_or_err("withdraw|7|900000000000000000000", (7, "withdraw", 900 * ONE), 5, root5)

    assert bridge.locked == bridge.minted, "reserve invariant"
    steps.append(f"invariant|locked={bridge.locked}|minted={bridge.minted}")

    return steps, forged_rejected, bridge


def main() -> int:
    steps, forged_rejected, bridge = run_scenario()
    assert len(steps) == 25, len(steps)
    digest = hashlib.sha256(";".join(steps).encode()).hexdigest()
    assert digest == SCENARIO_DIGEST, f"scenario digest {digest}"
    assert forged_rejected == 2
    assert bridge.locked == 800 * ONE == bridge.minted
    assert bridge.lc.trusted_height == 5

    canon = (
        f"E9|steps=25|scenario_digest={SCENARIO_DIGEST}|forged=2"
        f"|finalized=5|invariant={800 * ONE}|lane=green"
    )
    agg = hashlib.sha256(canon.encode()).hexdigest()
    assert agg == AGGREGATE_DIGEST, f"aggregate {agg}"

    print(f"E9-STEPS-OK {len(steps)}")
    print(
        'E9-SIM-OK {"steps": 25, "forged_rejected": 2, "finalized_height": 5, '
        '"locked": 800000000000000000000, "minted": 800000000000000000000, '
        '"lane": "green"}'
    )

    sim = {
        "steps": 25,
        "scenario_digest": SCENARIO_DIGEST,
        "aggregate_digest": AGGREGATE_DIGEST,
        "forged_rejected": 2,
        "finalized_height": 5,
        "locked": 800 * ONE,
        "minted": 800 * ONE,
        "per_tx_limit": PER_TX,
        "daily_limit": DAILY,
        "required_power": required_power(),
        "lane": "green",
    }
    (REPO / "sim/e9-bridge.json").write_text(
        json.dumps(sim, indent=2) + "\n", encoding="utf-8"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
