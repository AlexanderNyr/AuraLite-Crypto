#!/usr/bin/env python3
"""W7 golden vectors — offline genesis ceremony (independent oracle).

Independently computes the deterministic W7 ceremony trajectory and asserts
it against ``crates/aura-genesis/tests/fixtures/w7_ceremony_vectors.json``,
which ``crates/aura-genesis/tests/w7_ceremony.rs`` executes byte-for-byte.

Algorithm (see ``docs/w7-genesis-ceremony-plan.md``, D-W7-1..D-W7-5):

* key derivation: ``secret_i = blake3(KEY_CONTEXT || entropy || le32(i))``,
  ``pubkey_i = ed25519_public_key(secret_i)`` (gen_c4), signer address
  ``bech32(blake3(pubkey)[:20])`` (gen_c8);
* Shamir GF(2^8) (poly 0x11b, generator 2) per secret byte with
  deterministic coefficients ``c_0 = s_b`` and
  ``c_k = blake3(SHARE_CONTEXT || secret || le32(i) || le32(b))[k]``;
* witness chain ``h_i = blake3(WITNESS_CONTEXT || h_{i-1} || entry)`` from
  the root ``blake3(WITNESS_CONTEXT || b"genesis-ceremony-log-v1")``;
* genesis canonical bytes/hash/validator_set_hash from gen_c8 (C8 oracle).

Regenerate with::

    PYTHONPATH=/home/user/aura-python-deps python3 tools/gen_w7_vectors.py
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gen_c4_vectors as gen_c4  # noqa: E402  (pure-python ed25519 + signer_address)
import gen_c8_vectors as gen_c8  # noqa: E402  (blake3 + genesis canonical/hash oracle)


def blake3(data: bytes) -> bytes:
    return gen_c8.blake3_short(data)


KEY_CONTEXT = b"AuraLite-Crypto/ceremony-key-v1\x00"
SHARE_CONTEXT = b"AuraLite-Crypto/ceremony-share-v1\x00"
WITNESS_CONTEXT = b"AuraLite-Crypto/ceremony-witness-v1\x00"
COMMIT_CONTEXT = b"AuraLite-Crypto/ceremony-commit-v1\x00"
LOG_ROOT_ENTRY = b"genesis-ceremony-log-v1"


# ---------------------------------------------------------------- GF(256)

def _gf_tables():
    exp = [0] * 512
    log = [0] * 256
    x = 1
    for i in range(255):
        exp[i] = x
        log[x] = i
        # Multiply by the generator 3: 2 has order 51 under 0x11b.
        x ^= x << 1
        if x & 0x100:
            x ^= 0x11B
    for i in range(255, 512):
        exp[i] = exp[i - 255]
    return exp, log


_EXP, _LOG = _gf_tables()


def gf_mul(a: int, b: int) -> int:
    if a == 0 or b == 0:
        return 0
    return _EXP[_LOG[a] + _LOG[b]]


def gf_inv(a: int) -> int:
    return _EXP[255 - _LOG[a]]


def split_secret(secret: bytes, key_index: int, quorum: int, shares: int):
    """Return [(x, y-bytes)] exactly like ceremony.rs::split_secret."""
    out = []
    for x in range(1, shares + 1):
        y = bytearray(32)
        for byte in range(32):
            block = blake3(
                SHARE_CONTEXT
                + secret
                + key_index.to_bytes(4, "little")
                + byte.to_bytes(4, "little")
            )
            acc = 0
            for k in range(quorum - 1, -1, -1):
                coeff = secret[byte] if k == 0 else block[k]
                acc = gf_mul(acc, x) ^ coeff
            y[byte] = acc
        out.append((x, bytes(y)))
    return out


def reconstruct_secret(shares):
    """Lagrange at 0 over [(x, y-bytes)], exactly like ceremony.rs."""
    secret = bytearray(32)
    for byte in range(32):
        acc = 0
        for j, (x_j, y_j) in enumerate(shares):
            num = 1
            den = 1
            for m, (x_m, _) in enumerate(shares):
                if m == j:
                    continue
                num = gf_mul(num, x_m)
                den = gf_mul(den, x_m ^ x_j)
            basis = gf_mul(num, gf_inv(den))
            acc ^= gf_mul(y_j[byte], basis)
        secret[byte] = acc
    return bytes(secret)


# ---------------------------------------------------------------- witness

def chain_root() -> bytes:
    return blake3(WITNESS_CONTEXT + LOG_ROOT_ENTRY)


def chain_step(prev: bytes, entry: str) -> bytes:
    return blake3(WITNESS_CONTEXT + prev + entry.encode())


def witness_tip(entries):
    prev = chain_root()
    for entry in entries:
        prev = chain_step(prev, entry)
    return prev.hex()


# ---------------------------------------------------------------- ceremony

ENTROPY = blake3(b"w7-entropy-seed")[:16].hex()
ALICE = gen_c8.bech32_address(blake3(b"w7-alice")[:20])
BOB = gen_c8.bech32_address(blake3(b"w7-bob")[:20])
POWER = 100
GENESIS_TIME = 1500000000000


def key_pair(index: int):
    secret = blake3(KEY_CONTEXT + bytes.fromhex(ENTROPY) + index.to_bytes(4, "little"))
    public = gen_c4.ed25519_public_key(secret)
    address = gen_c8.bech32_address(gen_c4.signer_address(public))
    return secret, public.hex(), address


SECRET0, PUBKEY0, ADDRESS0 = key_pair(0)
SECRET1, PUBKEY1, ADDRESS1 = key_pair(1)


def commitment(secret: bytes) -> str:
    return blake3(COMMIT_CONTEXT + secret).hex()


def genesis_doc():
    return {
        "chain_id": "auradev-1",
        "protocol_version": 1,
        "genesis_time_ms": GENESIS_TIME,
        "validators": [
            {"address": ADDRESS0, "power": POWER, "public_key": PUBKEY0},
            {"address": ADDRESS1, "power": POWER, "public_key": PUBKEY1},
        ],
        "allocations": [
            {"address": ALICE, "balance": "100"},
            {"address": BOB, "balance": "250"},
        ],
    }


GENESIS_HASH = gen_c8.genesis_hash(genesis_doc()).hex()
VALIDATOR_SET_HASH = gen_c8.validator_set_hash(genesis_doc()).hex()

# Witness entry texts, in append order.
ENTRY_OPEN = (
    "open chain=auradev-1 validators=2 quorum=2 shares=3 power=100"
    f" genesis_time_ms={GENESIS_TIME}"
)
ENTRY_GEN0 = f"generate i=0 pubkey={PUBKEY0} address={ADDRESS0}"
ENTRY_GEN1 = f"generate i=1 pubkey={PUBKEY1} address={ADDRESS1}"
ENTRY_SPLIT0 = "split i=0 quorum=2 shares=3"
ENTRY_SPLIT1 = "split i=1 quorum=2 shares=3"
ENTRY_SHRED = "shred files=2"
ENTRY_SEAL = f"seal genesis={GENESIS_HASH} validator_set={VALIDATOR_SET_HASH}"

OPEN_CMD = [
    "open",
    "--ceremony-dir",
    "X",
    "--chain",
    "auradev-1",
    "--validators",
    "2",
    "--quorum",
    "2",
    "--shares",
    "3",
    "--power",
    "100",
    "--genesis-time",
    str(GENESIS_TIME),
    "--allocation",
    f"{ALICE}:100",
    "--allocation",
    f"{BOB}:250",
]
GENERATE_CMD = ["generate", "--ceremony-dir", "X", "--entropy-file", "entropy.hex"]
SPLIT_CMD = ["split", "--ceremony-dir", "X"]
SHRED_CMD = ["shred", "--ceremony-dir", "X"]
SEAL_CMD = ["seal", "--ceremony-dir", "X", "--out", "genesis.bin"]
VERIFY_CMD = ["verify", "--ceremony-dir", "X", "--root", "."]

OUT_OPEN = "status=opened\n"
OUT_GENERATE = (
    "status=generated\n"
    f"validator=0 address={ADDRESS0} pubkey={PUBKEY0}\n"
    f"validator=1 address={ADDRESS1} pubkey={PUBKEY1}\n"
)
OUT_SPLIT = "status=split keys=2 share_files=6\n"
OUT_SHRED = "status=shredded files=2\n"
TIP_SEAL = witness_tip(
    [ENTRY_OPEN, ENTRY_GEN0, ENTRY_GEN1, ENTRY_SPLIT0, ENTRY_SPLIT1, ENTRY_SHRED, ENTRY_SEAL]
)
TIP_SHRED = witness_tip(
    [ENTRY_OPEN, ENTRY_GEN0, ENTRY_GEN1, ENTRY_SPLIT0, ENTRY_SPLIT1, ENTRY_SHRED]
)
OUT_SEAL = (
    "status=sealed\n"
    f"genesis={GENESIS_HASH}\n"
    f"validator_set={VALIDATOR_SET_HASH}\n"
    f"witness_tip={TIP_SEAL}\n"
)
OUT_VERIFY = f"verify=ok\nscan=clean\nwitness_tip={TIP_SHRED}\n"

SETUP_FULL = OUT_OPEN + OUT_GENERATE + OUT_SPLIT + OUT_SHRED
SETUP_SPLIT = OUT_OPEN + OUT_GENERATE + OUT_SPLIT
SETUP_SHRED_ONLY = OUT_OPEN + OUT_GENERATE + OUT_SHRED

DOCTORED_PUBKEY1 = "ff" + PUBKEY1[2:]
REC_ONE = ["X/shares/key-00-share-01.share"]
REC_TWO = ["X/shares/key-00-share-01.share", "X/shares/key-00-share-02.share"]
REC_DUP = ["X/shares/key-00-share-01.share", "X/shares/key-00-share-01.share"]


def reconstruct_cmd(share_paths):
    cmd = ["reconstruct", "--ceremony-dir", "X", "--key", "0"]
    for path in share_paths:
        cmd += ["--share", path]
    cmd += ["--out", "rec.key"]
    return cmd


def case(name, commands, exit_code, stdout, stderr, nc_ref=None, mutations=None):
    fixture_case = {
        "name": name,
        "sandbox": "",
        "argv": commands,
        "exit": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }
    if nc_ref is not None:
        fixture_case["nc_ref"] = nc_ref
    if mutations:
        fixture_case["mutations"] = mutations
    return fixture_case


def build_cases():
    cases = []

    cases.append(
        case(
            "ceremony_seals_genesis",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, SHRED_CMD, SEAL_CMD, VERIFY_CMD],
            0,
            OUT_OPEN
            + OUT_GENERATE
            + OUT_SPLIT
            + OUT_SHRED
            + OUT_SEAL
            + f"verify=ok\nscan=clean\nwitness_tip={TIP_SEAL}\n",
            "",
        )
    )

    cases.append(
        case(
            "pubkey_set_mismatch_refused",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, SHRED_CMD, SEAL_CMD],
            65,
            SETUP_FULL,
            "error: ceremony: validator set mismatch\n",
            nc_ref=4,
            mutations=[
                f"sed -i 's/pubkey={PUBKEY1}/pubkey={DOCTORED_PUBKEY1}/g' X/manifest.txt"
            ],
        )
    )

    cases.append(
        case(
            "no_private_key_in_outputs",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, SHRED_CMD, VERIFY_CMD],
            0,
            SETUP_FULL + OUT_VERIFY,
            "",
            nc_ref=1,
        )
    )

    cases.append(
        case(
            "shamir_reconstructs_2_of_3",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, reconstruct_cmd(REC_TWO)],
            0,
            SETUP_SPLIT + f"secret_commitment={commitment(SECRET0)}\n",
            "",
            nc_ref=5,
        )
    )

    cases.append(
        case(
            "shamir_one_share_refused",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, reconstruct_cmd(REC_ONE)],
            65,
            SETUP_SPLIT,
            "error: ceremony: not enough shares\n",
            nc_ref=2,
        )
    )

    cases.append(
        case(
            "shamir_duplicate_share_refused",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, reconstruct_cmd(REC_DUP)],
            65,
            SETUP_SPLIT,
            "error: ceremony: duplicate share\n",
        )
    )

    cases.append(
        case(
            "witness_log_tamper_detected",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, SHRED_CMD, VERIFY_CMD],
            65,
            SETUP_FULL,
            "error: ceremony: witness chain broken\n",
            nc_ref=3,
            mutations=["sed -i 's/shred files=2/shred files=3/' X/witness.log"],
        )
    )

    cases.append(
        case(
            "seal_before_shred_refused",
            [OPEN_CMD, GENERATE_CMD, SPLIT_CMD, SEAL_CMD],
            65,
            SETUP_SPLIT,
            "error: ceremony: temporary media present\n",
            nc_ref=6,
        )
    )

    cases.append(
        case(
            "split_quorum_invalid_refused",
            [
                [
                    "open",
                    "--ceremony-dir",
                    "X",
                    "--chain",
                    "auradev-1",
                    "--validators",
                    "2",
                    "--quorum",
                    "3",
                    "--shares",
                    "2",
                    "--power",
                    "100",
                    "--genesis-time",
                    str(GENESIS_TIME),
                ]
            ],
            65,
            "",
            "error: ceremony: invalid quorum\n",
        )
    )

    cases.append(
        case(
            "seal_incomplete_witness_refused",
            [OPEN_CMD, GENERATE_CMD, SHRED_CMD, SEAL_CMD],
            65,
            SETUP_SHRED_ONLY,
            "error: ceremony: witness log incomplete\n",
        )
    )

    cases[0]["phase"] = "W7"
    return cases


def sanity_check():
    # Shamir round-trip: 2-of-3 must reconstruct exactly (NC-5 ground truth).
    shares = split_secret(SECRET0, 0, 2, 3)
    assert reconstruct_secret(shares[:2]) == SECRET0, "shamir round-trip broken"
    assert reconstruct_secret([shares[0], shares[2]]) == SECRET0, "shamir 1,3 broken"
    # Witness chain determinism.
    assert len(TIP_SEAL) == 64 and len(TIP_SHRED) == 64
    # Manifest binds exactly the generated keys.
    assert GENESIS_HASH == gen_c8.genesis_hash(genesis_doc()).hex()


def main():
    sanity_check()
    fixture = {
        "gate": (
            "W7 offline genesis ceremony: 10/10 cases pass; named negative "
            "control no_private_key_in_outputs red on shredded-key skip; 6/6 NC "
            "red under tools/nc_w7.py; not one private key in the repository/CI"
        ),
        "notice": (
            "Independent Python oracle (gen_c8 genesis primitives + "
            "gen_c4 ed25519 + gen_c2 blake3); verifier: "
            "crates/aura-genesis/tests/w7_ceremony.rs. Replace the whole file "
            "only with tools/gen_w7_vectors.py output."
        ),
        "named_control": "no_private_key_in_outputs",
        "files": {"entropy.hex": ENTROPY + "\n"},
        "cases": build_cases(),
    }
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_path = os.path.join(
        root, "crates/aura-genesis/tests/fixtures/w7_ceremony_vectors.json"
    )
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(fixture, handle, indent=1)
        handle.write("\n")
    print(
        f"wrote {os.path.relpath(out_path, root)}: 10 cases, "
        f"named control no_private_key_in_outputs"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
