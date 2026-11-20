"""AuraLite SDK (Python) — client for the ecosystem HTTP APIs
(explorer from E2, faucet from E3). Zero dependencies: urllib + json.
CI runs sdk/python/example.py via tools/e3_examples_check.py.
"""

import json
import urllib.request


class HttpResult:
    """One HTTP round trip: status code and raw body."""

    def __init__(self, status, body):
        self.status = status
        self.body = body


class AuraLiteClient:
    """Client for one explorer+faucet endpoint pair."""

    def __init__(self, base):
        self.base = base.rstrip("/")

    def _get(self, path):
        with urllib.request.urlopen(self.base + path) as response:
            return HttpResult(response.status, response.read().decode("utf-8"))

    def _post(self, path, payload):
        request = urllib.request.Request(
            self.base + path,
            data=payload.encode("utf-8"),
            headers={"content-type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(request) as response:
            return HttpResult(response.status, response.read().decode("utf-8"))

    def faucet_request(self, now, network, address):
        payload = json.dumps({"now": now, "network": network, "address": address})
        return self._post("/faucet", payload)

    def faucet_status(self):
        return self._get("/faucet/status")

    def block(self, height):
        return self._get("/block/%d" % height)

    def account(self, address):
        return self._get("/account/" + address)

    def health(self):
        return self._get("/health")
