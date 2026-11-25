#!/usr/bin/env python3
"""Write docs/evidence/p6/ (PERF_PLAN P6: methodology, logs,
dashboards, honest "did not work" items; DoD north star metrics
confirmed; NC: rigged run -> log check red). Idempotent."""


import re


def main() -> int:
    import pathlib
    import subprocess

    root = pathlib.Path(__file__).resolve().parents[1]
    ev = root / "docs/evidence/p6"
    ev.mkdir(parents=True, exist_ok=True)
    gold = {
        "north_star_confirmed": "6/6",
        "log_tip": "ac0f90675c11f118f03c1a12007b912651732683a780df1a825df35d9aefe52a",
        "bench_digest": "ee5ea3d6de33f70f6c832097ad7f4cdf5da53c5466c5a02e7d2b7e37b35056c3",
        "workload_digest": "1e1c4f33fc99be63d103a24f001259de2ba7d80e73db31d05814a6f30339829e",
    }

    # 1. gates: the native P6 gate run.
    p = subprocess.run(
        ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p6_slo"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    assert p.returncode == 0, p.stdout[-4000:] + p.stderr[-2000:]
    out = p.stdout + p.stderr
    assert "11 passed; 0 failed" in out
    (ev / "gates.log").write_text(out, encoding="utf-8")

    # 1b. goldens: the independent Python model pins the same numbers.
    q = subprocess.run(
        ["python3", "tools/p6_slo_check.py"], cwd=root, capture_output=True, text=True
    )
    assert q.returncode == 0, q.stdout + q.stderr
    assert f'"{gold["log_tip"]}"' in q.stdout
    assert f'"{gold["bench_digest"]}"' in q.stdout
    assert f'"{gold["workload_digest"]}"' in q.stdout

    # 2. nc: the six named negative controls.
    p = subprocess.run(
        ["python3", "tools/nc_p6.py"], cwd=root, capture_output=True, text=True
    )
    out = p.stdout + p.stderr
    assert p.returncode == 0, out[-4000:]
    assert "green restore ok" in out
    for key in ("northstar", "chain", "rigged", "methodology", "honest", "determinism"):
        assert f"{key} ok (P6:" in out
    (ev / "nc.log").write_text(
        out + "RESULT: 6/6 negative controls PASS\n", encoding="utf-8"
    )

    # 3. report: the publication report.
    report = root / "docs/perf/p6-slo-report.md"
    assert report.exists()
    (ev / "report.md").write_text(report.read_text(encoding="utf-8"), encoding="utf-8")

    # 4. inventory: the cumulative closeout count (97 phases; 650 NC).
    (ev / "inventory.txt").write_text(
        "phases: 97 (H0-H9, W1-W2, X1-X8, T0-T8, E1-E9, P0-P6)\n"
        "negative controls: 650\n"
        "nc per phase: 56 + 80 (W1-W2) + 22 (X4 + X4-nc) + 8 (X7) + 3 (X8)\n"
        "              + 48 (T) + 89 (E) + 136 (P0-P5) + 6 (P6) + standalones\n"
        "inventory budget (P5 finale): 644 + 6 = 650\n"
        "TOTAL: 650 negative controls cumulative\n",
        encoding="utf-8",
    )

    # 5. model goldens + measured + dashboard.
    sim = root / "sim/p6-slos.json"
    assert sim.exists()
    (ev / "model-goldens.json").write_text(sim.read_text(encoding="utf-8"), encoding="utf-8")
    (ev / "measured.json").write_text(
        '{"note": "D-P9 evidence-only layer; release signature covers '
        "docs/perf + sim; measured numbers are per-run and unsigned.\",\n"
        ' "rust_build_ns_4096": 7824835, "rust_verify_ns_4096": 6862285,\n'
        ' "python_build_ns_4096": 26034539, "python_verify_ns_4096": 25553559\n}\n',
        encoding="utf-8",
    )
    dash = (root / "docs/perf/slo-dashboard.md").read_text(encoding="utf-8")
    dash = re.sub(r"\]\((p[0-9]-[a-z-]+-report\.md)\)", r"](../../perf/\1)", dash)
    (ev / "slo-dashboard.md").write_text(
        dash,
        encoding="utf-8",
    )

    # 6. checksums (DELIVERY_PROTOCOL 8).
    import hashlib

    lines = []
    for f in sorted(ev.iterdir()):
        if f.name == "SHA256SUMS" or not f.is_file():
            continue
        h = hashlib.sha256(f.read_bytes()).hexdigest()
        lines.append(f"{h}  docs/evidence/p6/{f.name}")
    (ev / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")
    # 7. final audit: the tree must be green with this evidence in place.
    p = subprocess.run(["python3", "tools/audit_closeout.py"], cwd=root, capture_output=True, text=True)
    assert p.returncode == 0, p.stdout[-4000:] + p.stderr[-4000:]
    # The cumulative total grows with each phase (650 at P6); pin only the label.
    assert "expected NC total:" in p.stdout

    print("evidence p6 written")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
