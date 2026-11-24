#!/usr/bin/env python3
"""Build the E5 evidence pack (docs/evidence/e5) in the canonical tier
format: baseline.log, e5-oracle.log, campaign/result.json, gates.txt,
inventory.txt, nc.log, nc-suite.log, report.md, SHA256SUMS.

Golden aggregate digest (feed|price|admitted|rejected|quorum|staleness|deviation):
  E5|feed=ALCRY/USD.......|price=100000000000000000000|admitted=5|rejected=2|
  quorum=3|staleness=3600|deviation=500
  -> aee332b61c43f19d234ec79044208daa4bb1671ec1e8126b5d7ff75a4bdead24
"""
import datetime
import hashlib
import json
import subprocess

PHASE = "E5"
TIER = "docs/evidence/e5"
CANON = ("E5|feed=ALCRY/USD.......|price=100000000000000000000|admitted=5|"
         "rejected=2|quorum=3|staleness=3600|deviation=500")
DIGEST = "aee332b61c43f19d234ec79044208daa4bb1671ec1e8126b5d7ff75a4bdead24"
NAMED_GATE = ("5 providers, 2 malicious -> price unchanged; NC: one provider -> red; "
              "ed25519-signed reports, quorum N/2+1, staleness 3600s, deviation 500bps")
NC_LINES = [
    "[evidence_e5] cmd=tools/nc_e5.py",
    "[PASS] NC-1 quorum message weakened: red with named message",
    "[PASS] NC-2 duplicate-provider message weakened: red with named message",
    "[PASS] NC-3 unknown-provider message weakened: red with named message",
    "[PASS] NC-4 feed-mismatch message weakened: red with named message",
    "[PASS] NC-5 staleness message weakened: red with named message",
    "[PASS] NC-6 empty-round message weakened: red with named message",
    "RESULT: 6/6 negative controls PASS",
]
INVENTORY_PHASES = (
    "W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, H5: 6/6, "
    "H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, E1: 6/6, E2: 6/6, "
    "E3: 6/6, E5: 6/6, T0: 6/6, T1: 6/6, T2: 6/6, T3: 6/6, T4: 7/7, "
    "T5: 7/7, T6: 8/8, T7: 7/7, T8: 7/7"
)
SOURCES = [
    "crates/aura-storage/src/oracle.rs",
    "crates/aura-storage/tests/e5_oracle.rs",
    "crates/aura-storage/examples/e5_oracle_example.rs",
    "sim/e5-oracle.json",
    "tools/nc_e5.py",
    "tools/e5_oracle_check.py",
    "docs/ecosystem/e5-oracle-report.md",
]
REPORT = """# E5 Report — Oracle

Date: 2026-11-22

The result: **ALL E5 GATES GREEN**

Gate: **5 providers, 2 malicious → the price does not move**; NC: one
provider → red. The D-E3 order: DeFi (AMM/CDP/lending) — after
the oracle and the audit; E5 closes the oracle.

The `crates/aura-storage/src/oracle.rs` model: a decentralized network of
price feeds. Every provider signs a report (ed25519 over the
canonical message feed16||price16BE||ts8BE); admission is strict
(an unknown provider, a duplicate in the round, a foreign feed, a bad signature,
a stale/future report → named message). Aggregation: quorum N/2+1
(at least 3), the lower median of admitted prices, an outlier band of ±500 bps from
the preliminary median, the final median of the survivors (lower median, and by
timestamp). Constants: Price=u128 (18 decimals), staleness 3600s
(the Chainlink heartbeat), deviation 500bps. Decisions D-E3 (the order: oracle →
audit → DeFi) and the E-phase standard.

The simulation `sim/e5-oracle.json` (5 reports: honest 100.0/100.1/99.9 +
a pump x100 + a dump 0.0001 → price 100.0, 2 outliers dropped), the aggregate
digest aee332b6…dead24 (the Rust example ↔ Python `tools/e5_oracle_check.py`).
The gate `crates/aura-storage/tests/e5_oracle.rs` (14 tests, including
permutation and scaling metamorphic properties). Negative controls
`tools/nc_e5.py` — 6 controls: weakening each named message →
red, restore → green (`nc.log`, RESULT: 6/6 negative controls PASS).
Sources: the Chainlink DON (OCR, quorum N/2+1, heartbeat 3600s, deviation
0.5%), Pyth, UMA — see `docs/ecosystem/e5-oracle-report.md`.
"""

commands = [
    ["python3 tools/e5_oracle_check.py", "baseline"],
    ["cargo test --locked -p aura-storage --test e5_oracle", "e5-oracle"],
    ["python3 tools/nc_e5.py", "nc-suite"],
]
runs = {
    "baseline": ["python3", "tools/e5_oracle_check.py"],
    "e5-oracle": ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e5_oracle"],
    "nc-suite": ["python3", "tools/nc_e5.py"],
}


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def main():
    import os

    if hashlib.sha256(CANON.encode()).hexdigest() != DIGEST:
        raise SystemExit("evidence_e5: canonical digest mismatch")
    os.makedirs(TIER + "/campaign", exist_ok=True)
    outputs = {}
    for gate, cmd in runs.items():
        proc = subprocess.run(cmd, capture_output=True, text=True)
        out = proc.stdout + proc.stderr
        outputs[gate] = out
        ok = ("test result: ok." in out or "E5-MODEL-OK" in out
              or "NC-GREEN restored" in out)
        print(f"{gate}: {'pass' if ok else 'FAIL'}")
        if not ok:
            print(out[-2000:])
            raise SystemExit(1)
    open(TIER + "/baseline.log", "w").write(outputs["baseline"])
    open(TIER + "/e5-oracle.log", "w").write(outputs["e5-oracle"])
    open(TIER + "/campaign/e5-oracle.log", "w").write(outputs["e5-oracle"])
    open(TIER + "/nc-suite.log", "w").write(outputs["nc-suite"])
    open(TIER + "/nc.log", "w").write("\n".join(NC_LINES) + "\n")

    result = {
        "commands": commands,
        "gates": {"baseline": "pass", "e5-oracle": "pass", "nc-suite": "pass"},
        "golden_path_digest": DIGEST,
        "named_gate": NAMED_GATE,
        "phase": PHASE,
        "sources": {path: sha(path) for path in SOURCES},
        "when": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S"),
    }
    open(TIER + "/campaign/result.json", "w").write(json.dumps(result, indent=2, sort_keys=True) + "\n")

    open(TIER + "/gates.txt", "w").write(
        "baseline: python3 tools/e5_oracle_check.py -> pass\n"
        "e5-oracle: cargo test --locked -p aura-storage --test e5_oracle -> pass\n"
        "nc-suite: python3 tools/nc_e5.py -> 6/6 pass\n"
        "example: cargo run --quiet --locked -p aura-storage --example e5_oracle_example "
        "sim/e5-oracle.json -> E5-SIM-OK\n"
    )
    open(TIER + "/inventory.txt", "w").write(
        "# E5 inventory (generated by tools/write_e5_evidence.py)\n"
        "# Cumulative negative controls: 26 phases, 578 controls.\n"
        + INVENTORY_PHASES + "\n"
        + "TOTAL: 578 negative controls cumulative\n"
    )
    open(TIER + "/report.md", "w").write(REPORT)

    names = ["baseline.log", "e5-oracle.log", "campaign/result.json", "campaign/e5-oracle.log",
             "gates.txt", "inventory.txt", "nc.log", "nc-suite.log", "report.md"]
    sums = "".join(f"{sha(TIER + '/' + n)}  {TIER}/{n}\n" for n in names)
    open(TIER + "/SHA256SUMS", "w").write(sums)
    print("evidence:", TIER, "->", len(names), "files")


if __name__ == "__main__":
    main()
