// background.js — AuraLite wallet service worker (W6).
//
// UNTRUSTED GLUE. Every security decision runs inside the aura-keys WASM
// artifact (crates/aura-wallet/src/web.rs): the phishing policy (D-W6-3),
// the deep-link schema (D-W6-4), the real-tx-field view (D-W6-1) and the
// confirmation binding (D-W6-2). This file only relays messages and moves
// bytes in and out of the artifact's linear memory; it never interprets
// transaction contents itself.
//
// Buffer convention: inputs are copied into the artifact's linear memory at
// fixed offsets and every export receives (ptr, len) pairs; outputs are read
// back from the out offset the export was given. The artifact has zero
// imports, so nothing in this file can be called from inside the core.

const MEM = Object.freeze({
  URI: 0,        // deep-link text
  ORIGIN: 8192,  // request origin
  ENVELOPE: 16384,
  CHAIN: 32768,
  VIEW: 40960,   // derived ui: lines (what the human must see)
  OUT: 49152,    // generic out buffer (16 KiB)
});

const CODES = Object.freeze({
  OK: 0,
  ERR_MALFORMED_DEEP_LINK: -1,
  ERR_UNKNOWN_PARAMETER: -2,
  ERR_SUSPICIOUS_ORIGIN: -3,
  ERR_UNTRUSTED_ORIGIN: -4,
  ERR_UI_CONFIRMATION_SKIPPED: -5,
  ERR_SHOWN_VIEW_MISMATCH: -10,
});

let keysPromise = null;

function loadKeys() {
  if (!keysPromise) {
    keysPromise = (async () => {
      const url = chrome.runtime.getURL("aura_keys.wasm");
      const bytes = await (await fetch(url)).arrayBuffer();
      // The artifact is pure WASM with no imports: instantiate directly.
      const { instance } = await WebAssembly.instantiate(bytes, {});
      return instance.exports;
    })();
  }
  return keysPromise;
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

// Route one deep link through the wasm core: parse (D-W6-4) -> origin check
// (D-W6-3). The response handed to the popup carries only the raw envelope;
// the view is derived by `popup.js` via `aura_keys_confirm_view` and is the
// ONLY text the confirm dialog may render (D-W6-1).
async function handleDeepLink(uri, origin) {
  const keys = await loadKeys();
  const uriLen = put(keys, MEM.URI, uri);
  const n = keys.aura_keys_deep_link_parse(MEM.URI, uriLen, MEM.ENVELOPE, 16384);
  if (n < 0) return { ok: false, code: n };
  const originLen = put(keys, MEM.ORIGIN, origin);
  const code = keys.aura_keys_origin_check(MEM.ORIGIN, originLen);
  if (code !== CODES.OK) return { ok: false, code };
  return { ok: true, envelope: take(keys, MEM.ENVELOPE, n) };
}

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (message && message.type === "auralite:sign") {
    handleDeepLink(message.deepLink, message.origin)
      .then(respond)
      .catch(() => respond({ ok: false, code: CODES.ERR_MALFORMED_DEEP_LINK }));
    return true; // async respond
  }
  return false;
});
