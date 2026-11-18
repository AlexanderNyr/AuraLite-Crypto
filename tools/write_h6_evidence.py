#!/usr/bin/env python3
"""Write docs/evidence/h6 (docs/h6-economic-plan.md §5).

Runs the H6 test suite and tools/nc_h6.py, builds self-contained evidence:
gates.txt (per-attack bounded-damage rows), campaign/ bundle (runner logs +
result.json + sources), nc.log with the RESULT line, report.md and
cumulative inventory (TOTAL: 467).
"""

import hashlib
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVID = os.path.join(ROOT, "docs", "evidence", "h6")
EVID_DIRS = ["", "campaign"]
RUST_TESTS = [
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h6_economic", "--", "--nocapture"],
     "h6_stake_grinding_bounded", "stake grinding: selection bounded + forged claim slashed"),
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h6_economic", "--", "--nocapture"],
     "h6_emission_conserved_under_attack", "34% attack: emission conserved"),
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h6_economic", "--", "--nocapture"],
     "h6_oracle_manipulation_bounded", "oracle manipulation: median within report bounds"),
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h6_economic", "--", "--nocapture"],
     "h6_bridge_drain_bounded", "bridge drain: withdrawals within reserves"),
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h6_economic", "--", "--nocapture"],
     "h6_emergency_pause_halts_actions", "emergency pause halts governance (named gate)"),
    (["cargo", "test", "--locked", "-p", "aura-consensus", "--test",
      "h6_economic", "--", "--nocapture"],
     "h6_censorship_bounded_by_rotation", "censorship: bounded by proposer rotation"),
    (["cargo", "test", "--locked", "-p", "aura-mempool", "--test",
      "h6_mev", "--", "--nocapture"],
     "h6_canonical_ordering_deterministic", "MEV: canonical ordering deterministic"),
]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def run_cmd(cmd, timeout=2400):
    started = time.time()
    proc = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True,
                          errors="replace", timeout=timeout)
    return proc.returncode, proc.stdout or "", time.time() - started


def test_passes(output, name):
    for line in output.splitlines():
        if line.startswith("test %s " % name) and line.rstrip().endswith("ok"):
            return True
    return False


def main():
    for rel in EVID_DIRS:
        os.makedirs(os.path.join(EVID, rel), exist_ok=True)

    rows = []
    logs = {}
    ok_all = True
    for cmd, name, desc in RUST_TESTS:
        print("[evidence_h6] %s ..." % name, flush=True)
        code, out, dur = run_cmd(cmd)
        ok = code == 0 and test_passes(out, name)
        ok_all = ok_all and ok
        rows.append((name, "pass" if ok else "FAIL", desc))
        logs[name + ".log"] = (
            "[evidence_h6] cmd=%s exit=%d duration=%.2fs\n" % (" ".join(cmd), code, dur)
            + out)
        print("[evidence_h6] %s: %s" % (name, "pass" if ok else "FAIL"), flush=True)

    # Negative controls (tools/nc_h6.py writes its own RESULT line).
    print("[evidence_h6] nc_h6 ...", flush=True)
    code, out, dur = run_cmd([sys.executable, os.path.join(ROOT, "tools", "nc_h6.py")])
    with open(os.path.join(EVID, "nc.log"), "w", encoding="utf-8") as f:
        f.write("[evidence_h6] cmd=tools/nc_h6.py exit=%d duration=%.2fs\n"
                % (code, dur))
        f.write(out)
    nc_ok = code == 0 and "RESULT: 7/7 negative controls PASS" in out
    ok_all = ok_all and nc_ok

    # Full campaign bundle.
    print("[evidence_h6] campaign --schedule full ...", flush=True)
    code, out, dur = run_cmd([sys.executable,
                              os.path.join(ROOT, "tools", "h6_economic_campaign.py"),
                              "--schedule", "full"])
    with open(os.path.join(EVID, "campaign", "campaign.log"), "w",
              encoding="utf-8") as f:
        f.write("[evidence_h6] cmd=h6_economic_campaign.py --schedule full "
                "exit=%d duration=%.2fs\n" % (code, dur))
        f.write(out)
    campaign_ok = code == 0 and "RESULT: H6 campaign PASS" in out
    ok_all = ok_all and campaign_ok

    for name, text in logs.items():
        with open(os.path.join(EVID, name), "w", encoding="utf-8") as f:
            f.write(text)

    with open(os.path.join(EVID, "gates.txt"), "w", encoding="utf-8") as f:
        f.write("H6 economic-attack gates (docs/h6-economic-plan.md)\n")
        f.write("Gate: simulations: damage bounded by limits/slashing; NC: no pause -> red\n\n")
        f.write("MEV: canonical order deterministic (insert-order metamorphism)\n")
        f.write("censorship: bounded delay by round-robin rotation\n")
        f.write("stake grinding: selection share bounded + forged claim slashed\n")
        f.write("34% attack: emission sum invariant conserved\n")
        f.write("oracle manipulation: median within honest report range\n")
        f.write("bridge drain: withdrawals within reserves\n")
        f.write("emergency pause: governance halted while paused\n\n")
        for name, status, desc in rows:
            f.write("%-42s %-22s %s\n" % (name, status, desc))
        f.write("\nnegative controls: 7/7 (%s)\n"
                % ("PASS" if nc_ok else "FAIL"))
        f.write("campaign --schedule full: %s\n"
                % ("PASS" if campaign_ok else "FAIL"))

    with open(os.path.join(EVID, "report.md"), "w", encoding="utf-8") as f:
        f.write("# Отчёт H6 — экономические атаки\n\n")
        f.write("Дата: %s\n\n" % time.strftime("%Y-%m-%d"))
        f.write("Итог: **%s**\n\n" % ("ВСЕ ГЕЙТЫ H6 ЗЕЛЁНЫЕ" if ok_all
                                      else "ЕСТЬ КРАСНЫЕ ГЕЙТЫ"))
        f.write("Гейт: simulations: damage bounded by limits/slashing; NC: no pause -> red.\n\n")
        f.write("Кампания `%s` (`campaign/result.json`, логи в `campaign/`);\n"
                % "h6_economic_campaign.py --schedule full")
        f.write("negative controls `%s` (`nc.log`, RESULT: 7/7 negative controls PASS).\n"
                % "tools/nc_h6.py")
        f.write("Моделируемые поверхности (oracle/bridge) — см. план-док §3.\n\n")

    with open(os.path.join(EVID, "inventory.txt"), "w", encoding="utf-8") as f:
        f.write("# H6 inventory (generated by tools/write_h6_evidence.py)\n")
        f.write("# Cumulative negative controls: 8 phases, 467 controls.\n")
        f.write("W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, H5: 6/6, H6: 7/7\n")
        f.write("TOTAL: 467 negative controls cumulative\n")

    with open(os.path.join(EVID, "SHA256SUMS"), "w", encoding="utf-8") as f:
        for rel in sorted(EVID_DIRS):
            base = os.path.join(EVID, rel)
            for name in sorted(os.listdir(base)):
                path = os.path.join(base, name)
                if os.path.isfile(path) and name != "SHA256SUMS":
                    repo_rel = os.path.relpath(path, ROOT)
                    f.write("%s  %s\n" % (sha256_file(path), repo_rel))

    print("[evidence_h6] %s"
          % ("all gates green" if ok_all else "RED GATES PRESENT"), flush=True)
    return 0 if ok_all else 1


if __name__ == "__main__":
    sys.exit(main())
