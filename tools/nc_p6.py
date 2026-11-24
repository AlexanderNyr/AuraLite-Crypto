#!/usr/bin/env python3
"""P6 SLO publication — negative controls (X4-nc; DELIVERY_PROTOCOL 7).

Each mutation corrupts one publication invariant; every red run must
log the named message. `bad-rigged` is the P6 DoD NC: a rigged run ->
log check red. Green restore must return `11 passed; 0 failed`.
"""


def main() -> int:
    import subprocess

    target = "crates/aura-bench/src/p6.rs"
    text = open(target).read()
    before = {
        "northstar": "pub const P3_COMMIT_UNITS: u64 = 12300;",
        "chain": "        prev = chain_step(&prev, &line);",
        "rigged": "        .map(|e| (e.name, e.observed))",
        "methodology": (
            'pub const METHODOLOGY: [&str; 6] = [\n'
            '    "p0-methodology",\n    "p1-methodology",\n    "p2-methodology",\n'
            '    "p3-methodology",\n    "p4-methodology",\n    "p5-methodology",\n];'
        ),
        "honest": (
            'pub const HONEST_FAILURES: [&str; 4] = [\n'
            '    "p2-measured-wall-clock",\n    "p3-measured-sandbox-fsync",\n'
            '    "p1-bls-model-only",\n    "p2-lcg-fixture",\n];'
        ),
        "determinism": "        let again = Self::build(self.runs);",
    }
    after = {
        "northstar": "pub const P3_COMMIT_UNITS: u64 = 16000;",
        "chain": "        prev = chain_step(LOG_SEED, &line);",
        "rigged": (
            "        .map(|e| (e.name, "
            'if e.name == "p3.commit_units" { e.observed + 1 } else { e.observed }))'
        ),
        "methodology": before["methodology"].replace('"p5-methodology",', '"p4-methodology",'),
        "honest": before["honest"].replace('"p2-lcg-fixture",', '"p1-bls-model-only",'),
        "determinism": "        let again = Self::build(self.runs + 1);",
    }
    want = {
        "northstar": "P6: north star metric not confirmed (p3.commit_units 16000/15000)",
        "chain": "P6: run log hash chain broken at entry 1",
        "rigged": "P6: rigged run detected (p3.commit_units=12301)",
        "methodology": "P6: methodology gap (p5-methodology)",
        "honest": "P6: honest-failure item missing (p2-lcg-fixture)",
        "determinism": "P6: determinism violated (run 1)",
    }

    def test() -> tuple[bool, str]:
        p = subprocess.run(
            ["cargo", "test", "--locked", "-p", "aura-bench", "--test", "p6_slo"],
            text=True,
            capture_output=True,
        )
        out = p.stdout + p.stderr
        return (p.returncode == 0 and " 0 failed" in out), out

    ok, out = test()
    assert ok, "base run must be green:\n" + out[-2000:]
    print("base run green")
    for key in (
        "northstar",
        "chain",
        "rigged",
        "methodology",
        "honest",
        "determinism",
    ):
        assert before[key] in text, f"anchor missing: {key}"
        open(target, "w").write(text.replace(before[key], after[key], 1))
        ok, out = test()
        assert not ok, f"mutation must be RED: {key}"
        assert want[key] in out, f"named message missing for {key}:\n{out[-2000:]}"
        print(f"{key} ok ({want[key]})")
        open(target, "w").write(text)
    ok, out = test()
    assert ok, "green restore failed:\n" + out[-2000:]
    print("green restore ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
