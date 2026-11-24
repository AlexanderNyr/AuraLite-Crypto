#!/usr/bin/env python3
"""P2 parallel execution — independent Python model (PERF_PLAN P2).

Reproduces the deterministic fixture generator, both executors and the
cost model of `crates/aura-bench/src/p2.rs` byte for byte:

  fixture:  936 disjoint transfers (cells 0..1999) + 32 planted
            cross-chunk conflict pairs on hot cells 2000..2031,
            1000 transfers over 2096 accounts (balances 1_000_000)
  engine:   optimistic chunks (8 workers) -> read-set validation ->
            conflict: discard speculative writes (honest rollback) and
            re-execute in index order (Block-STM-like, Kung-Robinson OCC)
  cost:     seq = 1000 * 99 = 99_000 units
            par = 99_000/8 + aborted * 99

Output (stdout) is byte-identical to
`cargo run -p aura-bench --example p2_parallel_example`. `--measured`
adds an honest wall-clock line (evidence only, not byte-compared).
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]

FIXTURE_TXS = 1000
FIXTURE_ACCOUNTS = 2096
FIXTURE_CONFLICTS = 32
WORKERS = 8
TX_COST = 99
RUNS = 5


def sha(s: str) -> str:
    return hashlib.sha256(s.encode()).hexdigest()


def fixture_txs() -> list[tuple[int, int, int]]:
    hot = {}
    for k in range(FIXTURE_CONFLICTS):
        a = 4 + 27 * k
        hot[a] = (k, 0)
        hot[a + 125] = (k, 1)
    txs = []
    for i in range(FIXTURE_TXS):
        if i in hot:
            k, side = hot[i]
            txs.append((2000 + k, 2032 + 2 * k + side, 1 + ((k + 3 * side) % 9)))
        else:
            txs.append((2 * i, 2 * i + 1, 1 + (i % 9)))
    return txs


def fixture_state() -> dict[int, int]:
    return {i: 1_000_000 for i in range(FIXTURE_ACCOUNTS)}


def state_canonical(state: dict[int, int]) -> str:
    return "p2s" + "".join(f"|{i}:{state[i]}" for i in sorted(state))


def state_digest(state: dict[int, int]) -> str:
    return sha(state_canonical(state))


def apply_tx(state: dict[int, int], tx: tuple[int, int, int]) -> bool:
    frm, to, amount = tx
    if state.get(frm, 0) < amount:
        return False
    state[frm] = state.get(frm, 0) - amount
    state[to] = state.get(to, 0) + amount
    return True


def sequential_execute(txs, base) -> dict:
    state = dict(base)
    applied = rejected = 0
    for tx in txs:
        if apply_tx(state, tx):
            applied += 1
        else:
            rejected += 1
    return {
        "state": state,
        "state_digest": state_digest(state),
        "applied": applied,
        "rejected": rejected,
        "aborted": 0,
    }


def speculate_chunk(txs, base, start, end) -> list[dict]:
    local = dict(base)
    out = []
    for i in range(start, end):
        frm, to, amount = txs[i]
        from_bal = local.get(frm, 0)
        to_bal = local.get(to, 0)
        reads = sorted([(frm, from_bal), (to, to_bal)])
        applied = from_bal >= amount
        writes = []
        if applied:
            local[frm] = from_bal - amount
            local[to] = to_bal + amount
            writes = sorted([(frm, from_bal - amount), (to, to_bal + amount)])
        out.append({"index": i, "reads": reads, "writes": writes, "applied": applied})
    return out


def parallel_execute(txs, base, workers: int) -> dict:
    chunk = -(-len(txs) // workers)
    chunk_specs = []
    for w in range(workers):
        start, end = w * chunk, min((w + 1) * chunk, len(txs))
        if start < end:
            chunk_specs.append(speculate_chunk(txs, base, start, end))
    committed = dict(base)
    applied = rejected = aborted = 0
    discarded = []
    commit_order = []
    for specs in chunk_specs:
        for spec in specs:
            valid = all(committed.get(c, 0) == v for c, v in spec["reads"])
            if valid:
                for c, v in spec["writes"]:
                    committed[c] = v
                if spec["applied"]:
                    applied += 1
                else:
                    rejected += 1
            else:
                for c, v in spec["writes"]:
                    discarded.append((spec["index"], c, v))
                aborted += 1
                if apply_tx(committed, txs[spec["index"]]):
                    applied += 1
                else:
                    rejected += 1
            commit_order.append(spec["index"])
    return {
        "state": committed,
        "state_digest": state_digest(committed),
        "applied": applied,
        "rejected": rejected,
        "aborted": aborted,
        "discarded": discarded,
        "commit_order": commit_order,
    }


def build(txs, base) -> dict:
    seq = sequential_execute(txs, base)
    par = parallel_execute(txs, base, WORKERS)
    assert seq["state"] == par["state"], "P2 DoD: parallel == sequential"
    assert par["commit_order"] == list(range(len(txs))), "serializable order"
    aborted = par["aborted"]
    seq_c = len(txs) * TX_COST
    par_c = (len(txs) * TX_COST) // WORKERS + aborted * TX_COST
    speedup = seq_c * 100 // par_c - 100
    wd = sha(f"p2w|block|{FIXTURE_TXS}|{FIXTURE_ACCOUNTS}")
    md = sha("p0m|vcpu=8|mem_gib=32|storage=nvme|validators=100|net_mbit=1000")
    canonical = (
        f"p2b|{wd}|{md}|runs={RUNS}|spread=0"
        f"|p50={par_c}|p95={par_c}|p99={par_c}|total={par_c * RUNS}"
        f"|seq={seq_c}|par={par_c}|workers={WORKERS}|aborted={aborted}"
        f"|speedup_pct={speedup}"
    )
    return {
        "workload_digest": wd,
        "machine_digest": md,
        "runs": RUNS,
        "spread_pct": 0,
        "p50": par_c,
        "p95": par_c,
        "p99": par_c,
        "total": par_c * RUNS,
        "seq_cost": seq_c,
        "par_cost": par_c,
        "workers": WORKERS,
        "aborted": aborted,
        "applied": par["applied"],
        "state_digest": par["state_digest"],
        "speedup_pct": speedup,
        "bench_digest": sha(canonical),
    }


def main() -> int:
    txs = fixture_txs()
    base = fixture_state()
    assert len(txs) == FIXTURE_TXS and len(base) == FIXTURE_ACCOUNTS
    report = build(txs, base)
    # Frozen golden guardrails (updated once the engine is pinned):
    assert report["seq_cost"] == 99_000
    print(f"P2-RUNS-OK {report['runs']}")
    print("P2-PARALLEL-OK " + json.dumps(report))

    body = {
        "report": report,
        "cost_model": {
            "tx_cost": TX_COST,
            "workers": WORKERS,
            "fixture_txs": FIXTURE_TXS,
            "fixture_accounts": FIXTURE_ACCOUNTS,
            "fixture_conflicts": FIXTURE_CONFLICTS,
            "formula": "par = (txs * tx_cost) / workers + aborted * tx_cost",
        },
        "machine": {
            "vcpu": 8,
            "mem_gib": 32,
            "storage": "nvme",
            "validators": 100,
            "net_mbit": 1000,
        },
        "goldens": {
            "workload_digest": report["workload_digest"],
            "machine_digest": report["machine_digest"],
            "state_digest": report["state_digest"],
            "bench_digest": report["bench_digest"],
        },
    }
    (REPO / "sim").mkdir(exist_ok=True)
    (REPO / "sim/p2-parallel.json").write_text(
        json.dumps(body, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    if "--measured" in sys.argv:
        import time

        samples = []
        for _ in range(3):
            start = time.perf_counter_ns()
            state = dict(base)
            for tx in txs:
                apply_tx(state, tx)
            samples.append(time.perf_counter_ns() - start)
        samples.sort()
        print(
            "P2-MEASURED "
            + json.dumps({"python_seq_block_ns": samples[1], "iterations": FIXTURE_TXS})
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
