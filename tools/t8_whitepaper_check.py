#!/usr/bin/env python3
"""T8 white paper and law oracle.

A self-contained step-log model of the pinned T8 scenario (8 MiCA white
paper documents through the lifecycle; 1 fully valid). Derives the
canonical digest (must match the Rust `run_t8_scenario` step log,
`tests/t8_whitepaper.rs` and `sim/t8-whitepaper.json`) and checks the
governance named messages.
"""
import hashlib
import json
import sys

MESSAGES = {
    1: "T8: every white paper claim must be traceable to a pinned model",
    2: "T8: the MiCA checklist must cover articles 4, 6, 8 and 9",
    3: "T8: the notification must be at least 20 working days before publication",
    4: "T8: the white paper must be published before the offer starts",
    5: "T8: marketing communications must reference the published white paper",
    6: "T8: the white paper must carry the not-legal-advice disclaimer",
    7: "T8: the reference simulation must agree with the pinned digest",
}

NOTICE_DAYS = 20
DIGEST = "03a859433b58f14d6ad538faf2f3b504631c8b05707856a6ef1580694a89396f"
PINS = {
    1: 6_000_000 * 10 ** 18,
    2: 3_000_000 * 10 ** 18,
    3: 1_000_000_000,
    4: 10_000 * 10 ** 18,
    5: 2,
}
SECTIONS = (1, 2, 3, 4)

SECTION, CLAIM, VERIFY, DISCLAIMER, NOTIFY, PUBLISH, MARKET, CHECK, FAIL, FINAL = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
SECMISSING, CLAIMUNVER, NODISCLAIMER, NOTICETOOSHORT, NOTPUBLISHED, MKTNOTCOMPLIANT, PUBAFTEROFFER, VERSIONMISMATCH, NOLEGALPERSON = range(1, 10)
DRAFT, NOTIFIED, PUBLISHED, READY = 0, 1, 2, 3


class WP:
    def __init__(self, wp_id, legal_person=True):
        self.id, self.legal_person = wp_id, legal_person
        self.sections, self.claims = [], []
        self.disclaimer = False
        self.notified_day = self.published_day = None
        self.notified_version = self.published_version = 1
        self.offer_start = 0
        self.marketing = []
        self.state = DRAFT


class Machine:
    def __init__(self):
        self.wps, self.steps = [], []

    def snap(self, kind, subject, day, a, b, c, state, detail=0):
        self.steps.append((len(self.steps), kind, subject, day, a, b, c, state, detail))

    def new_wp(self, legal_person=True):
        wp = WP(len(self.wps) + 1, legal_person)
        self.wps.append(wp)
        return wp

    def verify(self, wp):
        ok = sum(1 for kind, expected in wp.claims if PINS[kind] == expected)
        return ok == len(wp.claims), ok

    def check(self, wp):
        if not wp.legal_person:
            return NOLEGALPERSON
        if sorted(wp.sections) != list(SECTIONS):
            return SECMISSING
        good, _ = self.verify(wp)
        if not good:
            return CLAIMUNVER
        if not wp.disclaimer:
            return NODISCLAIMER
        if wp.notified_day is None or wp.published_day is None:
            return NOTPUBLISHED
        if wp.published_day - wp.notified_day < NOTICE_DAYS:
            return NOTICETOOSHORT
        if wp.published_version != wp.notified_version:
            return VERSIONMISMATCH
        if wp.published_day >= wp.offer_start:
            return PUBAFTEROFFER
        for refs, after in wp.marketing:
            if not refs or not after:
                return MKTNOTCOMPLIANT
        return 0

    def state_of(self, wp):
        if wp.published_day is not None:
            return PUBLISHED
        if wp.notified_day is not None:
            return NOTIFIED
        return DRAFT

    def do_section(self, wp, code, day):
        wp.sections.append(code)
        self.snap(SECTION, wp.id, day, code, 0, 0, self.state_of(wp))

    def do_claim(self, wp, kind, expected, day):
        wp.claims.append((kind, expected))
        self.snap(CLAIM, wp.id, day, kind, expected, 0, self.state_of(wp))

    def do_verify(self, wp, day):
        good, ok = self.verify(wp)
        self.snap(VERIFY, wp.id, day, ok, len(wp.claims), 0, self.state_of(wp), 0 if good else CLAIMUNVER)

    def do_disclaimer(self, wp, day):
        wp.disclaimer = True
        self.snap(DISCLAIMER, wp.id, day, 1, 0, 0, self.state_of(wp))

    def do_notify(self, wp, day):
        wp.notified_day = day
        wp.notified_version = 1
        self.snap(NOTIFY, wp.id, day, 1, 0, 0, self.state_of(wp))

    def do_publish(self, wp, day, offer_start, version=None):
        wp.published_day = day
        wp.published_version = wp.notified_version if version is None else version
        wp.offer_start = offer_start
        self.snap(PUBLISH, wp.id, day, 1, offer_start, wp.published_version, self.state_of(wp))

    def do_market(self, wp, refs, after, day):
        wp.marketing.append((refs, after))
        self.snap(MARKET, wp.id, day, 1 if refs else 0, 1 if after else 0, 0, self.state_of(wp))

    def do_check(self, wp, day):
        r = self.check(wp)
        if r == 0:
            self.snap(CHECK, wp.id, day, 1, 0, 0, READY, 0)
        else:
            self.snap(FAIL, wp.id, day, 0, 0, 0, self.state_of(wp), r)


def build_scenario():
    m = Machine()
    wp = m.new_wp()
    for code in SECTIONS:
        m.do_section(wp, code, 10 + code)
    for kind in (1, 2, 3, 4, 5):
        m.do_claim(wp, kind, PINS[kind], 20 + kind)
    m.do_verify(wp, 30)
    m.do_disclaimer(wp, 31)
    m.do_notify(wp, 100)
    m.do_publish(wp, 120, 130)
    m.do_market(wp, True, True, 121)
    m.do_check(wp, 122)

    wp = m.new_wp()
    for code in SECTIONS:
        m.do_section(wp, code, 200 + code)
    m.do_claim(wp, 1, PINS[1], 210)
    m.do_claim(wp, 3, PINS[3] + 7, 211)
    m.do_verify(wp, 212)
    m.do_check(wp, 213)

    wp = m.new_wp()
    for code in SECTIONS:
        m.do_section(wp, code, 300 + code)
    m.do_claim(wp, 5, PINS[5], 310)
    m.do_verify(wp, 311)
    m.do_notify(wp, 320)
    m.do_publish(wp, 341, 350)
    m.do_check(wp, 342)

    wp = m.new_wp()
    for code in SECTIONS:
        m.do_section(wp, code, 400 + code)
    m.do_claim(wp, 1, PINS[1], 410)
    m.do_verify(wp, 411)
    m.do_disclaimer(wp, 412)
    m.do_notify(wp, 420)
    m.do_publish(wp, 439, 450)
    m.do_check(wp, 440)

    wp = m.new_wp()
    for code in SECTIONS:
        m.do_section(wp, code, 500 + code)
    m.do_claim(wp, 1, PINS[1], 510)
    m.do_verify(wp, 511)
    m.do_disclaimer(wp, 512)
    m.do_notify(wp, 520)
    m.do_publish(wp, 541, 540)
    m.do_check(wp, 542)

    wp = m.new_wp()
    for code in SECTIONS:
        m.do_section(wp, code, 600 + code)
    m.do_claim(wp, 1, PINS[1], 610)
    m.do_verify(wp, 611)
    m.do_disclaimer(wp, 612)
    m.do_notify(wp, 620)
    m.do_publish(wp, 641, 650)
    m.do_market(wp, False, False, 625)
    m.do_check(wp, 642)

    wp = m.new_wp()
    for code in (1, 2, 4):
        m.do_section(wp, code, 700 + code)
    m.do_claim(wp, 1, PINS[1], 710)
    m.do_verify(wp, 711)
    m.do_check(wp, 712)

    wp = m.new_wp(legal_person=False)
    for code in SECTIONS:
        m.do_section(wp, code, 800 + code)
    m.do_claim(wp, 1, PINS[1], 810)
    m.do_verify(wp, 811)
    m.do_check(wp, 812)

    valid = sum(1 for w in m.wps if m.check(w) == 0)
    m.snap(FINAL, 0, 0, len(m.wps), valid, 0, 9)
    return m


def digest_of(steps):
    blob = "".join("%d:%d:%d:%d:%d:%d:%d:%d:%d\n" % s for s in steps)
    return hashlib.sha256(blob.encode()).hexdigest()


def check(run_id):
    m = build_scenario()
    result = {"run_id": run_id, "schema": "auralite-sim-check-1", "tool": "t8_whitepaper_check.py"}
    ok = True
    try:
        d = digest_of(m.steps)
        assert len(m.steps) == 77, f"step count {len(m.steps)}"
        assert d == DIGEST, f"digest {d}"
        assert len(m.wps) == 8 and sum(1 for w in m.wps if m.check(w) == 0) == 1
        result["digest"] = d
        result["steps"] = len(m.steps)
        result["documents"] = len(m.wps)
        # msg1: claims traceable (unverified claim is red)
        assert (22, 8, 2, 2) == (22, m.steps[22][1], m.steps[22][2], m.steps[22][8]), m.steps[22]
        probe = Machine()
        wp = probe.new_wp()
        wp.claims.append((1, PINS[1] + 1))
        assert probe.verify(wp) == (False, 0), "probe"
        # msg2: checklist covers 4/6/8/9 (run outcomes)
        outcomes = [m.check(w) for w in m.wps]
        assert outcomes == [0, 2, 3, 4, 7, 6, 1, 9], outcomes
        # msg3: notification >= 20 working days (boundary)
        assert (41, 8, 4, 4) == (41, m.steps[41][1], m.steps[41][2], m.steps[41][8]), m.steps[41]
        assert m.steps[14][1] == CHECK, m.steps[14]
        # msg4: publication before the offer
        assert (51, 8, 5, 7) == (51, m.steps[51][1], m.steps[51][2], m.steps[51][8]), m.steps[51]
        # msg5: marketing compliance
        assert (62, 8, 6, 6) == (62, m.steps[62][1], m.steps[62][2], m.steps[62][8]), m.steps[62]
        # msg6: disclaimer present
        assert (31, 8, 3, 3) == (31, m.steps[31][1], m.steps[31][2], m.steps[31][8]), m.steps[31]
        # msg7: checkpoints + digest + sim file
        s14, s76 = m.steps[14], m.steps[76]
        assert (s14[1], s14[2], s14[7]) == (CHECK, 1, READY), s14
        assert (s76[1], s76[4], s76[5]) == (FINAL, 8, 1), s76
        try:
            sim = json.load(open("sim/t8-whitepaper.json"))
            if "digest" in sim:
                assert sim["digest"] == DIGEST, "sim digest mismatch"
                result["sim"] = "match"
        except FileNotFoundError:
            result["sim"] = "absent"
    except AssertionError as err:
        ok = False
        result["error"] = str(err)
    result["result"] = "pass" if ok else "fail"
    print(json.dumps(result, sort_keys=True))
    return ok


if __name__ == "__main__":
    if len(sys.argv) > 2:
        print(__doc__)
        raise SystemExit("usage: t8_whitepaper_check.py [run-id]")
    sys.exit(0 if check(sys.argv[1] if len(sys.argv) == 2 else "t8-run") else 1)
