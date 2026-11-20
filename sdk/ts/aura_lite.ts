// AuraLite SDK (TypeScript) — client for the ecosystem HTTP APIs
// (explorer from E2, faucet from E3). Zero dependencies: fetch + JSON.
// CI runs sdk/ts/example.ts via tools/e3_examples_check.py.

export interface HttpResult {
  status: number;
  body: string;
}

export interface AuraLiteClientOptions {
  base: string;
}

export function connect(options: AuraLiteClientOptions) {
  const base: string = options.base.replace(/\/+$/, "");
  async function get(path: string): Promise<HttpResult> {
    const response: Response = await fetch(base + path);
    const body: string = await response.text();
    return { status: response.status, body: body };
  }
  async function post(path: string, payload: string): Promise<HttpResult> {
    const response: Response = await fetch(base + path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: payload,
    });
    const body: string = await response.text();
    return { status: response.status, body: body };
  }
  return {
    faucetRequest: async function (now: number, network: string, address: string) {
      const payload: string = JSON.stringify({ now: now, network: network, address: address });
      return post("/faucet", payload);
    },
    faucetStatus: async function () {
      return get("/faucet/status");
    },
    block: async function (height: number) {
      return get("/block/" + height);
    },
    account: async function (address: string) {
      return get("/account/" + address);
    },
    health: async function () {
      return get("/health");
    },
  };
}
