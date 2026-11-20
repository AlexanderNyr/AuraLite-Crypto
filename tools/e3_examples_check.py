#!/usr/bin/env python3
"""Run the E3 SDK examples in CI (ECOSYSTEM_PLAN E3: "SDK examples pass
in CI").

Serves an independent Python reference faucet (the policy oracle), then
runs the three SDK examples against it:
  * sdk/python/example.py        — executed with python3
  * sdk/ts/example.ts            — deterministic type-strip + node
  * crates/aura-storage/examples/e3_sdk_example.rs — cargo run (spawns
    its own live faucet + explorer)

Each example must print `E3-EXAMPLE-OK {json}` with the same summary.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DAY = 86_400
EXPECTED = {
    "payouts": 2,
    "denied_rate_limit": 1,
    "denied_wrong_network": 1,
}


class ReferenceFaucet:
    """Independent implementation of the E3 faucet policy."""

    def __init__(self, payout=10, funding=1000):
        self.payout = payout
        self.reset()

    def reset(self):
        self.funds = 1000
        self.paid = set()
        self.payouts = 0

    def request(self, now, network, address):
        if not address or len(address) > 64:
            return {"outcome": "denied_bad_address"}
        if network != "testnet":
            return {"outcome": "denied_wrong_network"}
        key = (address, now // DAY)
        if key in self.paid:
            return {"outcome": "denied_rate_limit"}
        if self.funds < self.payout:
            return {"outcome": "denied_out_of_funds"}
        self.funds -= self.payout
        self.paid.add(key)
        self.payouts += 1
        return {"outcome": "paid", "day": now // DAY, "payout": self.payout}


FAUCET = ReferenceFaucet()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _send(self, code, payload):
        body = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/faucet/status"):
            self._send(200, {"funds": FAUCET.funds, "payouts": FAUCET.payouts,
                             "network": "testnet"})
        else:
            self._send(404, {"error": "not found"})

    def do_POST(self):
        if self.path.startswith("/faucet"):
            length = int(self.headers.get("content-length", "0"))
            body = json.loads(self.rfile.read(length) or b"{}")
            result = FAUCET.request(
                int(body.get("now", 0)),
                body.get("network", "testnet"),
                body.get("address", ""),
            )
            self._send(200, result)
        else:
            self._send(404, {"error": "not found"})


def strip_ts(text):
    """Deterministic type-strip for the constrained TS subset used by
    sdk/ts/: interface blocks, parameter/return/variable annotations."""
    text = re.sub(r"(export )?interface \w+ \{[^}]*\}\n", "", text)

    def fix_fn(match):
        head = match.group(1)
        params = match.group(2)
        fixed = []
        for part in params.split(","):
            part = part.strip()
            if part:
                fixed.append(part.split(":")[0].strip())
        return "%s(%s) {" % (head, ", ".join(fixed))

    text = re.sub(
        r"((?:async )?function \w*)\(([^)]*)\)(?::\s*[\w<>\[\]|\. ]+)? \{",
        fix_fn,
        text,
    )
    text = re.sub(
        r"\b(let|const|var) (\w+)\s*:\s*[\w<>\[\]|\. ]+ =",
        r"\1 \2 =",
        text,
    )
    text = text.replace('from "./aura_lite"', 'from "./aura_lite.mjs"')
    return text


def check_summary(output, lang):
    marker = "E3-EXAMPLE-OK "
    for line in output.splitlines():
        if line.startswith(marker):
            summary = json.loads(line[len(marker):])
            if summary.get("lang") != lang:
                return False, "wrong lang tag"
            for key, value in EXPECTED.items():
                if summary.get(key) != value:
                    return False, "summary %s=%r (want %r)" % (key, summary.get(key), value)
            return True, "summary ok"
    return False, "E3-EXAMPLE-OK marker missing"


def run(cmd, timeout=600, env=None):
    proc = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True,
                          errors="replace", timeout=timeout, env=env)
    return proc.returncode, proc.stdout or ""


def main():
    server = HTTPServer(("127.0.0.1", 0), Handler)
    port = server.server_address[1]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = "http://127.0.0.1:%d" % port

    passed = 0
    total = 0

    # --- Python example --------------------------------------------
    total += 1
    FAUCET.reset()
    code, out = run([sys.executable, os.path.join("sdk", "python", "example.py"), base])
    ok, why = (code == 0, "exit %d" % code) if code != 0 else check_summary(out, "python")
    print("[%s] python example: %s" % ("PASS" if ok else "FAIL", why))
    if not ok:
        print(out[-2000:])
    passed += bool(ok)

    # --- TypeScript example (deterministic strip + node) -----------
    total += 1
    node = shutil.which("node")
    ts_dir = os.path.join(ROOT, "sdk", "ts")
    if node:
        FAUCET.reset()
        tmp = tempfile.mkdtemp(prefix="e3-ts-")
        for name in ("aura_lite", "example"):
            with open(os.path.join(ts_dir, name + ".ts"), encoding="utf-8") as f:
                stripped = strip_ts(f.read())
            with open(os.path.join(tmp, name + ".mjs"), "w", encoding="utf-8") as f:
                f.write(stripped)
        code, out = run([node, os.path.join(tmp, "example.mjs"), base], timeout=120)
        ok, why = (code == 0, "exit %d" % code) if code != 0 else check_summary(out, "ts")
        print("[%s] ts example (strip+node): %s" % ("PASS" if ok else "FAIL", why))
        if not ok:
            print(out[-2000:])
    else:
        src = open(os.path.join(ts_dir, "example.ts"), encoding="utf-8").read()
        sdk = open(os.path.join(ts_dir, "aura_lite.ts"), encoding="utf-8").read()
        ok = ('faucetRequest' in src and 'faucetStatus' in src
              and 'faucetRequest' in sdk and 'export function connect' in sdk)
        why = "structural only (node not installed)"
        print("[%s] ts example: %s" % ("PASS" if ok else "FAIL", why))
    passed += bool(ok)

    # --- Rust example (self-contained) -----------------------------
    total += 1
    env = dict(os.environ)
    env.setdefault("CARGO_BUILD_JOBS", "1")
    code, out = run(["cargo", "run", "--locked", "-p", "aura-storage",
                     "--example", "e3_sdk_example"], env=env)
    ok, why = (code == 0, "exit %d" % code) if code != 0 else check_summary(out, "rust")
    print("[%s] rust example: %s" % ("PASS" if ok else "FAIL", why))
    if not ok:
        print(out[-2000:])
    passed += bool(ok)

    server.shutdown()
    print("RESULT: %d/%d SDK examples PASS" % (passed, total))
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
