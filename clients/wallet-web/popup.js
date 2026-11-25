// popup.js — the confirmation dialog (W6). UNTRUSTED GLUE.
//
// The dialog renders ONLY the `ui:` lines produced by
// `aura_keys_confirm_view` — the fields derived from the transaction bytes
// (D-W6-1). A page claim (to/amount) is compared against the bytes by
// `aura_keys_claim_check` and a disagreement blocks Confirm entirely.
// Confirm passes the EXACT rendered text back to `aura_keys_sign`, which
// re-derives the view and refuses on any difference (D-W6-2).

const CHAIN = "auralite-test-1"; // configured network (W6 demo policy)

const state = { keys: null, envelope: "", claim: null, shown: "" };

async function loadKeys() {
  const url = chrome.runtime.getURL("aura_keys.wasm");
  const bytes = await (await fetch(url)).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return instance.exports;
}

function put(keys, offset, text) {
  const bytes = new TextEncoder().encode(text);
  new Uint8Array(keys.memory.buffer).set(bytes, offset);
  return bytes.length;
}

function take(keys, offset, length) {
  return new TextDecoder().decode(
    new Uint8Array(keys.memory.buffer).subarray(offset, offset + length),
  );
}

async function render(envelope, claim) {
  const keys = (state.keys = await loadKeys());
  const envLen = put(keys, 16384, envelope);
  const chainLen = put(keys, 32768, CHAIN);
  const n = keys.aura_keys_confirm_view(16384, envLen, 32768, chainLen, 40960, 16384);
  if (n < 0) {
    document.getElementById("view").textContent = `refused (code ${n})`;
    document.getElementById("confirm").disabled = true;
    return;
  }
  state.envelope = envelope;
  state.claim = claim;
  state.shown = take(keys, 40960, n);
  document.getElementById("view").textContent = state.shown;

  if (claim) {
    const toLen = put(keys, 32768, claim.to || "");
    const amountLen = put(keys, 40960 + 16384, String(claim.amount ?? ""));
    const code = keys.aura_keys_claim_check(
      16384, envLen, 32768, chainLen, 32768, toLen, 40960 + 16384, amountLen,
    );
    if (code !== 0) {
      // D-W6-1: the claim disagrees with the bytes. Show the real fields,
      // disable Confirm, and name the substitution.
      document.getElementById("claim").textContent =
        `blocked: claimed fields disagree with the transaction (code ${code})`;
      document.getElementById("confirm").disabled = true;
    }
  }
}

document.getElementById("confirm").addEventListener("click", async () => {
  const keys = state.keys;
  const secret = await walletSecret(); // extension storage -> raw 32 bytes
  const envLen = put(keys, 16384, state.envelope);
  const chainLen = put(keys, 32768, CHAIN);
  // The binding (D-W6-2): the artifact only signs when `shown` is byte-identical
  // to the view it derives from the envelope.
  const n = keys.aura_keys_sign(
    65536, secret.length, 16384, envLen, 32768, chainLen,
    40960, put(keys, 40960, state.shown), 73728, 64,
  );
  if (n !== 64) {
    document.getElementById("claim").textContent = `sign refused (code ${n})`;
    return;
  }
  const sig = new Uint8Array(keys.memory.buffer.slice(73728, 73728 + 64));
  chrome.runtime.sendMessage({ type: "auralite:signed", sig });
  window.close();
});

document.getElementById("deny").addEventListener("click", () => {
  chrome.runtime.sendMessage({ type: "auralite:denied" });
  window.close();
});

// Demo-only secret fetch; the real extension keeps this in encrypted storage
// and never exposes it to page contexts.
async function walletSecret() {
  const stored = await chrome.storage.local.get(["signingKey"]);
  return new Uint8Array(stored.signingKey);
}

chrome.runtime.onMessage.addListener((message) => {
  if (message && message.type === "auralite:sign" && message.ok) {
    render(message.envelope, message.claim || null);
  }
});
