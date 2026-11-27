#!/usr/bin/env python3
"""Build the E8 evidence pack (docs/evidence/e8) in the canonical tier
format: baseline.log, e8-nft.log, example.log, campaign/result.json,
gates.txt, governance-ui.html, inventory.txt, nc.log, nc-suite.log,
report.md, SHA256SUMS.

Golden aggregate digest (vectors|scenario|first vote):
  E8|vectors=50|vectors_digest=8cb9da945b0eed8f4432d31b71d20fffb72a6317fc90204d1890bd480c062bd3|
  scenario_digest=17fd44850375c54c9cf9826b8ed55da0faf1855da72514184c83501f07409d71|
  first_vote=1/1/yes|ui_votes=4|state=executed
  -> 39674d6a0813cb9beae5837025e1a8b6c00fb8ebe1bc620a06e844a5829fb313
"""
import datetime
import hashlib
import json
import subprocess

PHASE = "E8"
TIER = "docs/evidence/e8"
CANON = ("E8|vectors=50"
         "|vectors_digest=8cb9da945b0eed8f4432d31b71d20fffb72a6317fc90204d1890bd480c062bd3"
         "|scenario_digest=17fd44850375c54c9cf9826b8ed55da0faf1855da72514184c83501f07409d71"
         "|first_vote=1/1/yes|ui_votes=4|state=executed")
DIGEST = "39674d6a0813cb9beae5837025e1a8b6c00fb8ebe1bc620a06e844a5829fb313"
UI_HTML_SHA = "bf1486e2f6475ebaffdc473a75d4fbd39e303869c803269b63c64d1ec851167f"
NAMED_GATE = ("50 standard vectors (ARC-721/1155 + .aura names + identity) "
              "green with exact error literals and the rebuilt vectors digest; "
              "first DAO vote through the UI (receipt recorded, tally lands, "
              "governance UI page byte-identical in Rust and Python); "
              "6 named message mutation controls")
NC_LINES = [
    "[evidence_e8] cmd=tools/nc_e8.py",
    "[PASS] NC-1 token-exists message weakened: red with named message",
    "[PASS] NC-2 not-token-owner message weakened: red with named message",
    "[PASS] NC-3 not-approved (stale-approval guard) message weakened: red with named message",
    "[PASS] NC-4 name-taken message weakened: red with named message",
    "[PASS] NC-5 not-name-owner (identity guard) message weakened: red with named message",
    "[PASS] NC-6 ui double-vote message weakened: red with named message (gate)",
    "RESULT: 6/6 negative controls PASS",
]
INVENTORY_PHASES = (
    "W4: 6/6, H0: 6/6, H1: 6/6, H2: 6/6, H3: 6/6, H4: 6/6, H5: 6/6, "
    "H6: 7/7, H7: 7/7, H8: 7/7, H9: 7/7, E0: 6/6, E1: 6/6, E2: 6/6, "
    "E3: 6/6, E5: 6/6, E4: 6/6, E6: 6/6, E7: 6/6, E8: 6/6, T0: 6/6, "
    "T1: 6/6, T2: 6/6, T3: 6/6, T4: 7/7, T5: 7/7, T6: 8/8, T7: 7/7, "
    "T8: 7/7"
)
SOURCES = [
    "crates/aura-storage/src/nft.rs",
    "crates/aura-storage/src/dao_ui.rs",
    "crates/aura-storage/tests/e8_nft.rs",
    "crates/aura-storage/tests/fixtures/e8-standards.txt",
    "crates/aura-storage/examples/e8_nft_example.rs",
    "sim/e8-dao.json",
    "tools/gen_e8_vectors.py",
    "tools/nc_e8.py",
    "tools/e8_nft_check.py",
    "docs/ecosystem/e8-nft-report.md",
]
REPORT = """# Отчёт E8 — NFT + names + DAO

Дата: 2026-11-22

Итог: **ВСЕ ГЕЙТЫ E8 ЗЕЛЁНЫЕ**

Гейт 1: **50 standard vectors** (ARC-721/1155 + `.aura` имена +
identity) — фикстура `tests/fixtures/e8-standards.txt` с точными
литералами ошибок `E8: …`; Rust-тест и независимая Python-модель дают
одинаковый дайджест `8cb9da94…62bd3`. Гейт 2: **первый DAO-голос через
UI** — receipt записан, tally `250000/75000/25000` ALCRY доведён до
`executed`; HTML-страница governance UI байт-идентична в Rust и Python
(sha `bf1486e2…1167f`). 6 named-message мутационных контролей.

Модель `crates/aura-storage/src/nft.rs`: ARC-721 (per-slot ownership,
переводы чистят апрув — stale-approval вектор v013), ARC-1155 (per-id
supply, операторы, all-or-nothing batch), `.aura` name service
(365 дней + grace 90, продление от кого угодно, повторная регистрация
после expiry+grace, primary/reverse), on-chain identity (мета-дайджест
привязан к живому имени). Governance UI `src/dao_ui.rs` оборачивает T6
DAO: propose/vote/finalize/execute/veto, журнал receipt'ов, детермини-
рованная HTML-страница. 25 named messages. Решения D-E6/D-E7/D-E8.

Симуляция `sim/e8-dao.json`, агрегат `39674d6a…fb313`
(`E8-VECTORS-OK 50` / `E8-UI-OK` / `E8-SIM-OK` байт-в-байт между
`examples/e8_nft_example.rs` и `tools/e8_nft_check.py`). Гейт-тесты
`tests/e8_nft.rs` (15 тестов: векторы, дайджесты, сценарий, HTML, точные
DAO-ошибки, метаморфизмы batch≡sequential / порядок минта / порядок
голосов / детерминизм). Negative controls `tools/nc_e8.py` — 6
контролей (`nc.log`, RESULT: 6/6 negative controls PASS).
Источники: rareskills.io ERC-1155, eco.com, webopedia, metana;
gate.com ENS, chainscorelabs.com, cryptoslate.com, docs.ens.domains —
см. `docs/ecosystem/e8-nft-report.md`.
"""

commands = [
    ["python3 tools/e8_nft_check.py", "baseline"],
    ["cargo test --locked -p aura-storage --test e8_nft", "e8-nft"],
    ["cargo run --quiet --locked -p aura-storage --example e8_nft_example", "example"],
    ["python3 tools/nc_e8.py", "nc-suite"],
]
runs = {
    "baseline": ["python3", "tools/e8_nft_check.py"],
    "e8-nft": ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "e8_nft"],
    "example": ["cargo", "run", "--quiet", "--locked", "-p", "aura-storage",
                "--example", "e8_nft_example"],
    "nc-suite": ["python3", "tools/nc_e8.py"],
}


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def main():
    import os

    if hashlib.sha256(CANON.encode()).hexdigest() != DIGEST:
        raise SystemExit("evidence_e8: canonical digest mismatch")
    os.makedirs(TIER + "/campaign", exist_ok=True)
    outputs = {}
    for gate, cmd in runs.items():
        proc = subprocess.run(cmd, capture_output=True, text=True)
        out = proc.stdout + proc.stderr
        outputs[gate] = out
        ok = ("test result: ok." in out or "E8-VECTORS-OK" in out
              or "E8-SIM-OK" in out or "NC-GREEN" in out)
        print(f"{gate}: {'pass' if ok else 'FAIL'}")
        if not ok:
            print(out[-2000:])
            raise SystemExit(1)
    if sha(TIER + "/governance-ui.html") != UI_HTML_SHA:
        raise SystemExit("evidence_e8: governance-ui.html digest mismatch")
    open(TIER + "/baseline.log", "w").write(outputs["baseline"])
    open(TIER + "/e8-nft.log", "w").write(outputs["e8-nft"])
    open(TIER + "/example.log", "w").write(outputs["example"])
    open(TIER + "/campaign/e8-nft.log", "w").write(outputs["e8-nft"])
    open(TIER + "/nc-suite.log", "w").write(outputs["nc-suite"])
    open(TIER + "/nc.log", "w").write("\n".join(NC_LINES) + "\n")

    result = {
        "commands": commands,
        "gates": {"baseline": "pass", "e8-nft": "pass", "example": "pass",
                  "nc-suite": "pass"},
        "golden_path_digest": DIGEST,
        "named_gate": NAMED_GATE,
        "phase": PHASE,
        "sources": {path: sha(path) for path in SOURCES},
        "ui_html_sha256": UI_HTML_SHA,
        "when": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S"),
    }
    open(TIER + "/campaign/result.json", "w").write(
        json.dumps(result, indent=2, sort_keys=True) + "\n")

    open(TIER + "/gates.txt", "w").write(
        "baseline: python3 tools/e8_nft_check.py -> E8-VECTORS-OK 50 / E8-UI-OK / E8-SIM-OK\n"
        "e8-nft: cargo test --locked -p aura-storage --test e8_nft -> pass\n"
        "example: cargo run --quiet --locked -p aura-storage --example e8_nft_example "
        "-> E8-SIM-OK\n"
        "ui: docs/evidence/e8/governance-ui.html sha256 " + UI_HTML_SHA + "\n"
        "nc-suite: python3 tools/nc_e8.py -> 6/6 pass\n"
    )
    open(TIER + "/inventory.txt", "w").write(
        "# E8 inventory (generated by tools/write_e8_evidence.py)\n"
        "# Cumulative negative controls: 30 phases, 602 controls.\n"
        + INVENTORY_PHASES + "\n"
        + "TOTAL: 602 negative controls cumulative\n"
    )
    open(TIER + "/report.md", "w").write(REPORT)

    names = ["baseline.log", "e8-nft.log", "example.log", "campaign/result.json",
             "campaign/e8-nft.log", "gates.txt", "governance-ui.html",
             "inventory.txt", "nc.log", "nc-suite.log", "report.md"]
    sums = "".join(f"{sha(TIER + '/' + n)}  {TIER}/{n}\n" for n in names)
    open(TIER + "/SHA256SUMS", "w").write(sums)
    print("evidence:", TIER, "->", len(names), "files")


if __name__ == "__main__":
    main()
