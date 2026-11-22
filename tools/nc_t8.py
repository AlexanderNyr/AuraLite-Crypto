#!/usr/bin/env python3
"""T8 negative-control runner (7 mutations of crates/aura-storage/src/whitepaper.rs)."""
import os
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "crates/aura-storage/src/whitepaper.rs"
TEST_CMD = ["cargo", "test", "--locked", "-p", "aura-storage", "--test", "t8_whitepaper"]
PASS_MARKERS = ("test result: ok.",)
MUTATIONS = [
    ("claim.expected == claim.kind.pin()",
     "true",
     "T8: every white paper claim must be traceable to a pinned model"),
    ("if !self.legal_person {",
     "if false {",
     "T8: the MiCA checklist must cover articles 4, 6, 8 and 9"),
    ("if published - notified < T8_NOTICE_WORKING_DAYS {",
     "if published - notified < T8_NOTICE_WORKING_DAYS - 1 {",
     "T8: the notification must be at least 20 working days before publication"),
    ("if published >= self.offer_start {",
     "if false {",
     "T8: the white paper must be published before the offer starts"),
    ("if !marketing.references_wp || !marketing.after_publication {",
     "if false {",
     "T8: marketing communications must reference the published white paper"),
    ("if !self.disclaimer {",
     "if false {",
     "T8: the white paper must carry the not-legal-advice disclaimer"),
    ("pub const T8_NOTICE_WORKING_DAYS: u32 = 20;",
     "pub const T8_NOTICE_WORKING_DAYS: u32 = 19;",
     "T8: the reference simulation must agree with the pinned digest"),
]


def run_control(name, path, anchor, replacement, expected):
    with open(path, encoding="utf-8") as f:
        original = f.read()
    if original.count(anchor) != 1:
        return False, "anchor count %d" % original.count(anchor)
    mutated = original.replace(anchor, replacement)
    try:
        with open(path, "w", encoding="utf-8") as f:
            f.write(mutated)
        proc = subprocess.run(TEST_CMD, cwd=ROOT, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True,
                              errors="replace")
        output = proc.stdout or ""
    finally:
        with open(path, "w", encoding="utf-8") as f:
            f.write(original)
    compile_failed = "error[" in output and "test result" not in output
    if compile_failed:
        return False, "mutation broke compilation"
    if any(marker in output for marker in PASS_MARKERS):
        return False, "mutated gate still passes"
    if expected in output:
        return True, "red with named message"
    return False, "red without named message"


def main():
    print("running negative controls against", TARGET)
    names = [
        "NC-1 unverified claim accepted",
        "NC-2 legal person requirement dropped",
        "NC-3 notification window shortened",
        "NC-4 publication after offer allowed",
        "NC-5 marketing reference not enforced",
        "NC-6 disclaimer not enforced",
        "NC-7 pinned parameters drift",
    ]
    for i, (anchor, replacement, expected) in enumerate(MUTATIONS):
        ok, note = run_control(names[i], TARGET, anchor, replacement, expected)
        print("%s: %s" % (names[i].split()[0], note))
        if not ok:
            print("negative control %d failed" % (i + 1))
            raise SystemExit(1)
    print("RESULT: 7/7 negative controls PASS")


if __name__ == "__main__":
    main()
