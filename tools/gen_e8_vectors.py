#!/usr/bin/env python3
"""E8 standard-vector generator (ECOSYSTEM_PLAN E8).

Computes the golden expectations for the 50 ARC-721/1155 + `.aura` name
service + identity conformance vectors and writes the pinned fixture
`crates/aura-storage/tests/fixtures/e8-standards.txt` (block format,
counted by `tools/count_vectors.py`).

The fixture is the golden oracle: every vector embeds its expected
status (ok / exact `E8: …` error literal) and post-state reads. The
Rust test (`tests/e8_nft.rs`) and the independent Python model
(`tools/e8_nft_check.py`) each re-implement the semantics and must
reproduce every embedded expectation byte for byte.

Usage:
    python3 tools/gen_e8_vectors.py            # (re)write the fixture
"""

from __future__ import annotations

import hashlib
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
FIXTURE = REPO / "crates/aura-storage/tests/fixtures/e8-standards.txt"

GRACE_DAYS = 90
TERM_DAYS = 365
ZERO = "zero"
DIGEST_1 = "11" * 32
DIGEST_2 = "22" * 32

# ---------------------------------------------------------------- model


class E8Error(Exception):
    def __init__(self, msg: str):
        super().__init__(msg)
        self.msg = msg


class Model:
    """Golden state machine for the E8 standard vectors."""

    def __init__(self) -> None:
        self.actors = ["root", "alice", "bob", "carol", "dave", "eve", ZERO]
        self.owner721: dict[int, str] = {}
        self.approved: dict[int, str] = {}
        self.bal721: dict[str, int] = {a: 0 for a in self.actors}
        self.supply721 = 0
        self.bal1155: dict[tuple[str, int], int] = {}
        self.supply1155: dict[int, int] = {}
        self.uri1155: dict[int, str] = {}
        self.operator: dict[tuple[str, str], bool] = {}
        # name service: name -> {owner, target, expiry}
        self.names: dict[str, dict] = {}
        self.primary: dict[str, str] = {}
        self.identity: dict[str, str] = {}
        self.day = 0

    # -- 721
    def mint721(self, by: str, to: str, id: int) -> dict:
        if by != "root":
            raise E8Error(f"E8: not collection owner ({by})")
        if to == ZERO:
            raise E8Error("E8: zero address")
        if id in self.owner721:
            raise E8Error(f"E8: token already exists ({id})")
        self.owner721[id] = to
        self.bal721[to] += 1
        self.supply721 += 1
        return self.reads721(id)

    def transfer721(self, by: str, id: int, to: str) -> dict:
        if id not in self.owner721:
            raise E8Error(f"E8: token does not exist ({id})")
        if self.owner721[id] != by:
            raise E8Error(f"E8: not token owner ({by}/{id})")
        if to == ZERO:
            raise E8Error("E8: zero address")
        self.bal721[by] -= 1
        self.bal721[to] += 1
        self.owner721[id] = to
        self.approved.pop(id, None)  # transfers clear the approval
        return self.reads721(id)

    def approve721(self, by: str, id: int, spender: str) -> dict:
        if id not in self.owner721:
            raise E8Error(f"E8: token does not exist ({id})")
        if self.owner721[id] != by:
            raise E8Error(f"E8: not token owner ({by}/{id})")
        if spender == ZERO:
            raise E8Error("E8: zero address")
        self.approved[id] = spender
        return self.reads721(id)

    def transfer_from721(self, by: str, id: int, to: str) -> dict:
        if id not in self.owner721:
            raise E8Error(f"E8: token does not exist ({id})")
        if to == ZERO:
            raise E8Error("E8: zero address")
        if self.approved.get(id) != by:
            raise E8Error(f"E8: not approved ({by}/{id})")
        frm = self.owner721[id]
        self.bal721[frm] -= 1
        self.bal721[to] += 1
        self.owner721[id] = to
        self.approved.pop(id, None)  # transfers clear the approval
        return self.reads721(id)

    def burn721(self, by: str, id: int) -> dict:
        if id not in self.owner721:
            raise E8Error(f"E8: token does not exist ({id})")
        if self.owner721[id] != by:
            raise E8Error(f"E8: not token owner ({by}/{id})")
        del self.owner721[id]
        self.approved.pop(id, None)
        self.bal721[by] -= 1
        self.supply721 -= 1
        return self.reads721(id)

    def read_owner_of(self, id: int) -> dict:
        if id not in self.owner721:
            raise E8Error(f"E8: token does not exist ({id})")
        return self.reads721(id)

    def reads721(self, id: int) -> dict:
        r = {"owner_of:" + str(id): self.owner721.get(id, "none"),
             "supply721": str(self.supply721)}
        if id in self.approved:
            r["approved:" + str(id)] = self.approved[id]
        else:
            r["approved:" + str(id)] = "none"
        for a in self.actors:
            r[f"balance721:{a}"] = str(self.bal721[a])
        return r

    # -- 1155
    def mint1155(self, by: str, to: str, id: int, amount: int) -> dict:
        if by != "root":
            raise E8Error(f"E8: not collection owner ({by})")
        if to == ZERO:
            raise E8Error("E8: zero address")
        if amount == 0:
            raise E8Error("E8: amount must be positive")
        self.bal1155[(to, id)] = self.bal1155.get((to, id), 0) + amount
        self.supply1155[id] = self.supply1155.get(id, 0) + amount
        return self.reads1155([(to, id)])

    def transfer1155(self, frm: str, by: str, id: int, to: str,
                     amount: int) -> dict:
        if to == ZERO:
            raise E8Error("E8: zero address")
        if amount == 0:
            raise E8Error("E8: amount must be positive")
        if by != frm and not self.operator.get((frm, by), False):
            raise E8Error(f"E8: not approved ({by}/{id})")
        have = self.bal1155.get((frm, id), 0)
        if have < amount:
            raise E8Error(
                f"E8: insufficient balance ({frm}/{id}/{have}/{amount})"
            )
        self.bal1155[(frm, id)] = have - amount
        self.bal1155[(to, id)] = self.bal1155.get((to, id), 0) + amount
        return self.reads1155([(frm, id), (to, id)])

    def set_operator(self, by: str, operator: str, approved: bool) -> dict:
        if operator == ZERO:
            raise E8Error("E8: zero address")
        self.operator[(by, operator)] = approved
        return {f"operator:{by}:{operator}": "true" if approved else "false"}

    def transfer_batch1155(self, frm: str, by: str, to: str, ids: list[int],
                           amounts: list[int]) -> dict:
        if len(ids) != len(amounts):
            raise E8Error(f"E8: batch length mismatch ({len(ids)}/{len(amounts)})")
        if to == ZERO:
            raise E8Error("E8: zero address")
        if by != frm and not self.operator.get((frm, by), False):
            raise E8Error(f"E8: not approved ({by}/{ids[0]})")
        reads: dict[str, str] = {}
        for id, amount in zip(ids, amounts):
            if amount == 0:
                raise E8Error("E8: amount must be positive")
            have = self.bal1155.get((frm, id), 0)
            if have < amount:
                raise E8Error(
                    f"E8: insufficient balance ({frm}/{id}/{have}/{amount})"
                )
        for id, amount in zip(ids, amounts):
            self.bal1155[(frm, id)] -= amount
            self.bal1155[(to, id)] = self.bal1155.get((to, id), 0) + amount
        for id in ids:
            reads.update(self.reads1155([(frm, id), (to, id)]))
        return reads

    def set_uri(self, by: str, id: int, uri: str) -> dict:
        if by != "root":
            raise E8Error(f"E8: not collection owner ({by})")
        self.uri1155[id] = uri
        return {f"uri:{id}": uri}

    def reads1155(self, pairs: list[tuple[str, int]]) -> dict:
        r: dict[str, str] = {}
        for a, id in pairs:
            r[f"balance1155:{a}:{id}"] = str(self.bal1155.get((a, id), 0))
            r[f"supply1155:{id}"] = str(self.supply1155.get(id, 0))
        return r

    # -- names
    @staticmethod
    def valid_name(name: str) -> bool:
        if not name.endswith(".aura") or len(name) == len(".aura"):
            return False
        label = name[: -len(".aura")]
        if not (1 <= len(label) <= 32):
            return False
        if not all(c.islower() or c.isdigit() or c == "-" for c in label):
            return False
        return label[0].isalnum() and label[-1].isalnum()

    def register_name(self, by: str, name: str, day: int) -> dict:
        self.day = day
        if not self.valid_name(name):
            raise E8Error(f"E8: invalid name ({name})")
        if name in self.names and day <= self.names[name]["expiry"] + GRACE_DAYS:
            raise E8Error(f"E8: name already registered ({name})")
        self.names[name] = {"owner": by, "target": by, "expiry": day + TERM_DAYS}
        return self.reads_name(name)

    def renew_name(self, name: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        rec = self.names[name]
        base = max(day, rec["expiry"])
        rec["expiry"] = base + TERM_DAYS
        return self.reads_name(name)

    def transfer_name(self, by: str, name: str, to: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        rec = self.names[name]
        if day > rec["expiry"] + GRACE_DAYS:
            raise E8Error(f"E8: name expired ({name})")
        if rec["owner"] != by:
            raise E8Error(f"E8: not name owner ({by}/{name})")
        if to == ZERO:
            raise E8Error("E8: zero address")
        if self.primary.get(by) == name:
            del self.primary[by]
        rec["owner"] = to
        reads = self.reads_name(name)
        reads[f"reverse:{by}"] = self.primary.get(by, "none")
        return reads

    def set_target(self, by: str, name: str, target: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        rec = self.names[name]
        if day > rec["expiry"] + GRACE_DAYS:
            raise E8Error(f"E8: name expired ({name})")
        if rec["owner"] != by:
            raise E8Error(f"E8: not name owner ({by}/{name})")
        if target == ZERO:
            raise E8Error("E8: zero address")
        rec["target"] = target
        return self.reads_name(name)

    def resolve(self, name: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        rec = self.names[name]
        if day > rec["expiry"] + GRACE_DAYS:
            raise E8Error(f"E8: name expired ({name})")
        return self.reads_name(name)

    def set_primary(self, by: str, name: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        rec = self.names[name]
        if day > rec["expiry"] + GRACE_DAYS:
            raise E8Error(f"E8: name expired ({name})")
        if rec["owner"] != by:
            raise E8Error(f"E8: not name owner ({by}/{name})")
        self.primary[by] = name
        return {f"reverse:{by}": name}

    def advance_day(self, day: int) -> dict:
        self.day = day
        return {"day": str(day)}

    def reads_name(self, name: str) -> dict:
        if name not in self.names:
            return {"resolve:" + name: "none"}
        rec = self.names[name]
        return {
            "resolve:" + name: rec["target"],
            "name_owner:" + name: rec["owner"],
            "expiry:" + name: str(rec["expiry"]),
        }

    # -- identity
    def set_identity(self, by: str, name: str, digest: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        rec = self.names[name]
        if day > rec["expiry"] + GRACE_DAYS:
            raise E8Error(f"E8: name expired ({name})")
        if rec["owner"] != by:
            raise E8Error(f"E8: not name owner ({by}/{name})")
        self.identity[name] = digest
        return {f"identity:{name}": digest}

    def clear_identity(self, by: str, name: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        rec = self.names[name]
        if rec["owner"] != by:
            raise E8Error(f"E8: not name owner ({by}/{name})")
        self.identity.pop(name, None)
        return {f"identity:{name}": "none"}

    def read_identity(self, name: str, day: int) -> dict:
        self.day = day
        if name not in self.names:
            raise E8Error(f"E8: name not registered ({name})")
        if name not in self.identity:
            raise E8Error(f"E8: no identity record ({name})")
        return {f"identity:{name}": self.identity[name]}


# ------------------------------------------------------------- vectors
# (op, kwargs, expected_reads_subset) — expectations computed by the
# model below; every vector is pinned literally into the fixture.

def build_vectors() -> list[dict]:
    m = Model()
    out: list[dict] = []

    def ok(op: str, **kw):
        fn = getattr(m, op)
        reads = fn(**kw)
        out.append({"op": op, "args": kw, "status": "ok", "err": None,
                    "reads": reads})

    def err(op: str, **kw):
        fn = getattr(m, op)
        try:
            fn(**kw)
        except E8Error as e:
            out.append({"op": op, "args": kw, "status": "err", "err": e.msg,
                        "reads": {}})
            return
        raise AssertionError(f"{op} {kw}: expected error, got ok")

    # -- ARC-721 (v001..v016)
    ok("mint721", by="root", to="alice", id=1)
    ok("mint721", by="root", to="alice", id=2)
    err("mint721", by="alice", to="alice", id=3)
    err("mint721", by="root", to="alice", id=1)
    err("read_owner_of", id=9)
    ok("transfer721", by="alice", id=1, to="bob")
    err("transfer721", by="carol", id=1, to="eve")
    err("transfer721", by="bob", id=1, to="zero")
    ok("approve721", by="bob", id=1, spender="carol")
    ok("transfer_from721", by="carol", id=1, to="carol")
    ok("approve721", by="alice", id=2, spender="dave")
    ok("transfer721", by="alice", id=2, to="eve")
    err("transfer_from721", by="dave", id=2, to="dave")   # stale approval died
    ok("burn721", by="eve", id=2)
    err("read_owner_of", id=2)
    ok("mint721", by="root", to="eve", id=2)              # re-mint after burn

    # -- ARC-1155 (v017..v028)
    ok("mint1155", by="root", to="alice", id=100, amount=1000)
    ok("mint1155", by="root", to="alice", id=101, amount=1)
    ok("transfer1155", frm="alice", by="alice", id=100, to="bob", amount=400)
    err("transfer1155", frm="bob", by="bob", id=100, to="carol", amount=500)
    err("transfer1155", frm="alice", by="carol", id=100, to="alice", amount=1)
    err("transfer1155", frm="alice", by="alice", id=100, to="bob", amount=0)
    ok("set_operator", by="alice", operator="dave", approved=True)
    ok("transfer1155", frm="alice", by="dave", id=100, to="carol", amount=100)
    ok("set_operator", by="alice", operator="dave", approved=False)
    err("transfer1155", frm="alice", by="dave", id=100, to="carol", amount=100)
    ok("transfer_batch1155", frm="alice", by="alice", to="carol",
       ids=[100, 101], amounts=[50, 1])
    err("transfer_batch1155", frm="alice", by="alice", to="carol",
        ids=[100, 101], amounts=[50])

    # -- .aura names (v029..v043)
    ok("register_name", by="alice", name="alice.aura", day=1)
    err("register_name", by="bob", name="alice.aura", day=1)
    err("resolve", name="unknown.aura", day=1)
    err("register_name", by="alice", name="Alice.aura", day=1)
    ok("register_name", by="alice", name="dao.aura", day=1)
    ok("set_target", by="alice", name="dao.aura", target="bob", day=1)
    ok("transfer_name", by="alice", name="alice.aura", to="bob", day=1)
    ok("set_primary", by="bob", name="alice.aura", day=1)
    ok("transfer_name", by="bob", name="alice.aura", to="alice", day=2)
    err("set_target", by="carol", name="alice.aura", target="carol", day=2)
    ok("advance_day", day=367)          # grace: expiry 366 + 90
    ok("renew_name", name="alice.aura", day=367)   # anyone may renew
    ok("advance_day", day=822)          # boundary day == expiry 732 + grace 90
    err("resolve", name="alice.aura", day=823)     # 823 > 822 -> expired
    ok("register_name", by="eve", name="alice.aura", day=823)

    # -- identity (v044..v050)
    ok("set_identity", by="eve", name="alice.aura", digest=DIGEST_1, day=823)
    ok("register_name", by="carol", name="carol.aura", day=823)
    err("read_identity", name="carol.aura", day=823)
    err("set_identity", by="bob", name="carol.aura", digest=DIGEST_1, day=823)
    ok("set_identity", by="eve", name="alice.aura", digest=DIGEST_2, day=823)
    ok("clear_identity", by="eve", name="alice.aura", day=823)
    err("read_identity", name="alice.aura", day=823)

    assert len(out) == 50, len(out)
    for i, v in enumerate(out, 1):
        v["id"] = f"v{i:03d}"
    return out


def canonical(v: dict) -> str:
    reads = " ".join(f"{k}={v}" for k, v in sorted(v["reads"].items()))
    status = v["status"] if v["status"] == "ok" else f"err:{v['err']}"
    return f"{v['id']}|{v['op']}|{status}|{reads}"


def main() -> int:
    vectors = build_vectors()
    digest = hashlib.sha256(
        ";".join(canonical(v) for v in vectors).encode()
    ).hexdigest()
    lines = [
        "# E8 standard vectors — ARC-721/1155 + .aura names + on-chain identity.",
        "# Generated by tools/gen_e8_vectors.py (golden model). 50 vectors;",
        "# block format for tools/count_vectors.py. Every status/err literal",
        "# and post-state read is an exact expectation for the Rust test and",
        "# the independent Python model tools/e8_nft_check.py.",
        f"digest={digest}",
    ]
    for v in vectors:
        lines.append(f"vector={v['id']}")
        lines.append(f"op={v['op']}")
        for k, val in v["args"].items():
            if isinstance(val, bool):
                val = "true" if val else "false"
            elif isinstance(val, list):
                val = ",".join(str(x) for x in val)
            lines.append(f"{k}={val}")
        lines.append(f"status={v['status']}")
        if v["err"] is not None:
            lines.append(f"err={v['err']}")
        for k, val in sorted(v["reads"].items()):
            lines.append(f"read.{k}={val}")
        lines.append("")
    FIXTURE.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"fixture: {FIXTURE.relative_to(REPO)} ({len(vectors)} vectors)")
    print(f"digest:  {digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
