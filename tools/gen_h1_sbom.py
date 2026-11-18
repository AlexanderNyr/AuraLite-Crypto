#!/usr/bin/env python3
"""Deterministic CycloneDX 1.5 SBOM generator from `Cargo.lock` (H1).

Pure function of the lockfile: sorted components, content-derived
`serialNumber`, no timestamps. `tools/check_supply_chain.py` pins the digest
of the generated `sbom.cdx.json`; regenerate with `make gen-h1-sbom`.
Design: docs/h1-supply-chain-plan.md (D-H1-3).
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "Cargo.lock"
OUT = ROOT / "sbom.cdx.json"


def parse_packages(lock_text: str) -> list[dict]:
    pkgs = []
    for block in lock_text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        source = re.search(r'^source = "([^"]+)"', block, re.M)
        checksum = re.search(r'^checksum = "([^"]+)"', block, re.M)
        if name and version:
            pkgs.append({
                "name": name.group(1),
                "version": version.group(1),
                "source": source.group(1) if source else "",
                "checksum": checksum.group(1) if checksum else "",
            })
    pkgs.sort(key=lambda p: (p["name"], p["version"]))
    return pkgs


def build_sbom(lock_text: str) -> dict:
    pkgs = parse_packages(lock_text)
    components = []
    for p in pkgs:
        comp = {
            "type": "library",
            "name": p["name"],
            "version": p["version"],
            "purl": f"pkg:cargo/{p['name']}@{p['version']}",
        }
        if p["checksum"]:
            comp["hashes"] = [{"alg": "SHA-256", "content": p["checksum"]}]
        if p["source"]:
            comp["properties"] = [{"name": "cdx:cargo:source", "value": p["source"]}]
        components.append(comp)
    digest = hashlib.sha256(
        json.dumps(components, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": "urn:uuid:" + "-".join(
            [digest[:8], digest[8:12], digest[12:16], digest[16:20], digest[20:32]]
        ),
        "version": 1,
        "metadata": {
            "tools": [{"vendor": "AuraLite", "name": "gen_h1_sbom", "version": "1"}],
            "component": {
                "type": "application",
                "name": "AuraLite-Crypto",
                "version": "wip",
            },
        },
        "components": components,
    }


def main() -> int:
    sbom = build_sbom(LOCK.read_text(encoding="utf-8"))
    OUT.write_text(json.dumps(sbom, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)} ({len(sbom['components'])} components)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
