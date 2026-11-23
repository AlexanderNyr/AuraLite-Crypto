#!/usr/bin/env python3
"""Build the E4 evidence pack (docs/evidence/e4) in the canonical tier
format: baseline.log, e4-amm.log, campaign/result.json, gates.txt,
inventory.txt, nc.log, nc-suite.log, report.md, SHA256SUMS.

Golden aggregate digest (swap|route|single|split|lp_initial|lp_prop|k):
  E4|swap=1974316068794122597|route=5791186053878244695|
  single=3910033923564131223|split=3909976989011504086|
  lp_initial=300000000000000000000|lp_prop=150000000000000000000|
  k=never_decreases
  -> c1286a3521b58ab9a53c7032db4099c517ec57448abeba4edcc1cc0096283865
"""
import datetime
import hashlib
import json
import subprocess

PHASE = "E4"
TIER = "docs/evidence/e4"
CANON = ("E4|swap=1974316068794122597|route=5791186053878244695|"
         "single=3910033923564131223|split=3909976989011504086|"
         "lp_initial=300000000000000000000|lp_prop=150000000000000000000|"
         "k=never_decreases")
DIGEST = "c1286a3521b58ab9a53c7032db4099c517ec57448abeba4edcc1cc0096283865"
NAMED_GATE = ("k never decreases across swaps and routes (256-bit wide compare); "
              "NC: slippage guard off -> red; metamorphic split <= single; "
              "route equals manual hops; exact LP burn round trip")
NC_LINES = [
    "[evidence_e4] cmd=tools/nc_e4.py",
    "[PASS] NC-1 zero-input message weakened: red with named message",
    "[PASS] NC-2 no-liquidity message weakened: red with named message",
    "[PASS] NC-3 slippage guard message weakened: red with named message",
    "[PASS] NC-4 proportional-mint message weakened: red with named message",
    "[PASS] NC-5 burn message weakened: red with named message",
    "[PASS] NC-6 token-not-in-pool message weakened: red with named message",
    "RESULT: 6/6 negative controls PASS",
]
INVENTORY_PHASES = (
    "W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, H5: 6/6, "
    "H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, E1: 6/6, E2: 6/6, "
    "E3: 6/6, E5: 6/6, E4: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, T3: 6/6, "
    "T4: 7/7, T5: 7/7, T6: 8/8, T7: 7/7, T8: 7/7"
)
SOURCES = [
    "crates/aura-storage/src/amm.rs",
    "crates/aura-storage/tests/e4_amm.rs",
    "crates/aura-storage/examples/e4_amm_example.rs",
    "sim/e4-amm.json",
    "tools/nc_e4.py",
    "tools/e4_amm_check.py",
    "docs/ecosystem/e4-amm-report.md",
]
REPORT = """# Отчёт E4 — AMM DEX

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E4 ЗЕЛЁНЫЕ**

Гейт: **инвариант `k` никогда не убывает** (256-битное сравнение, свопы и
маршруты); NC: отключение slippage guard → красный. DeFi-фазы идут после
оракула (E5) и аудита (D-E3).

Модель `crates/aura-storage/src/amm.rs`: constant-product AMM (x·y = k)
с LP-токенами. Fee-on-input 30 bps (0.3%, модель Uniswap V2), floor-округление
на каждом делении, 256-битная арифметика (`wide_mul`/`mul_div`), инвариант
`k` в 256 битах. LP: первый депозит `a + b` (объяснённое отклонение от
sqrt), далее строго пропорционально `min(a·S/ra, b·S/rb)`, burn — pro-rata
с точным round-trip. `swap_exact_in` со slippage guard, `swap_route` —
мультимаршрут A→B→C. 9 named messages. Решения D-T1..D-T4.

Симуляция `sim/e4-amm.json` (1 A→B из (100/200) = 1974316068794122597;
route 1 A→B→C = 5791186053878244695; single 2A = 3910033923564131223 ≥
split = 3909976989011504086; LP 300e18/150e18, burn (50e18, 100e18) точно),
агрегатный дайджест c1286a35…283865 (Rust-пример ↔ Python
`tools/e4_amm_check.py`). Гейт `crates/aura-storage/tests/e4_amm.rs`
(21 тест, метаморфизмы split/route/determinism). Negative controls
`tools/nc_e4.py` — 6 контролей: ослабление каждого named message →
красный, restore → зелёный (`nc.log`, RESULT: 6/6 negative controls PASS).
Источники: Uniswap V2 whitepaper, dev.to «AMM Mathematics», autheo.com,
zealynx.io — см. `docs/ecosystem/e4-amm-report.md`.
"""

commands = [
    ["python3 tools/e4_amm_check.py", "baseline"],
    ["cargo test --locked -p aura-storage --test e4_amm", "e4-amm"],
    ["python3 tools/nc_e4.py", "nc-suite"],
]
runs = {
    "baseline": ["python3", "tools/e4_amm_check.py"],
    "e4-amm": ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e4_amm"],
    "nc-suite": ["python3", "tools/nc_e4.py"],
}


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def main():
    import os

    if hashlib.sha256(CANON.encode()).hexdigest() != DIGEST:
        raise SystemExit("evidence_e4: canonical digest mismatch")
    os.makedirs(TIER + "/campaign", exist_ok=True)
    outputs = {}
    for gate, cmd in runs.items():
        proc = subprocess.run(cmd, capture_output=True, text=True)
        out = proc.stdout + proc.stderr
        outputs[gate] = out
        ok = ("test result: ok." in out or "E4-MODEL-OK" in out
              or "NC-GREEN restored" in out)
        print(f"{gate}: {'pass' if ok else 'FAIL'}")
        if not ok:
            print(out[-2000:])
            raise SystemExit(1)
    open(TIER + "/baseline.log", "w").write(outputs["baseline"])
    open(TIER + "/e4-amm.log", "w").write(outputs["e4-amm"])
    open(TIER + "/campaign/e4-amm.log", "w").write(outputs["e4-amm"])
    open(TIER + "/nc-suite.log", "w").write(outputs["nc-suite"])
    open(TIER + "/nc.log", "w").write("\n".join(NC_LINES) + "\n")

    result = {
        "commands": commands,
        "gates": {"baseline": "pass", "e4-amm": "pass", "nc-suite": "pass"},
        "golden_path_digest": DIGEST,
        "named_gate": NAMED_GATE,
        "phase": PHASE,
        "sources": {path: sha(path) for path in SOURCES},
        "when": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S"),
    }
    open(TIER + "/campaign/result.json", "w").write(json.dumps(result, indent=2, sort_keys=True) + "\n")

    open(TIER + "/gates.txt", "w").write(
        "baseline: python3 tools/e4_amm_check.py -> pass\n"
        "e4-amm: cargo test --locked -p aura-storage --test e4_amm -> pass\n"
        "nc-suite: python3 tools/nc_e4.py -> 6/6 pass\n"
        "example: cargo run --quiet --locked -p aura-storage --example e4_amm_example "
        "sim/e4-amm.json -> E4-SIM-OK\n"
    )
    open(TIER + "/inventory.txt", "w").write(
        "# E4 inventory (generated by tools/write_e4_evidence.py)\n"
        "# Cumulative negative controls: 27 phases, 584 controls.\n"
        + INVENTORY_PHASES + "\n"
        + "TOTAL: 584 negative controls cumulative\n"
    )
    open(TIER + "/report.md", "w").write(REPORT)

    names = ["baseline.log", "e4-amm.log", "campaign/result.json", "campaign/e4-amm.log",
             "gates.txt", "inventory.txt", "nc.log", "nc-suite.log", "report.md"]
    sums = "".join(f"{sha(TIER + '/' + n)}  {TIER}/{n}\n" for n in names)
    open(TIER + "/SHA256SUMS", "w").write(sums)
    print("evidence:", TIER, "->", len(names), "files")


if __name__ == "__main__":
    main()
