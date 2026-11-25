// content.js — dApp bridge (W6). UNTRUSTED GLUE.
//
// A page asks for a signature by presenting an `auralite://sign?…` deep link.
// This script only forwards the link and the true page origin to the
// background worker; it never sees keys, never renders fields and never
// decides anything (D-W6-1..D-W6-4 are enforced in the aura-keys WASM core).

(() => {
  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const data = event.data;
    if (!data || data.type !== "auralite:request-sign") return;
    if (typeof data.deepLink !== "string") return;
    chrome.runtime.sendMessage(
      {
        type: "auralite:sign",
        deepLink: data.deepLink,
        origin: window.location.origin,
      },
      (response) => {
        window.postMessage({ type: "auralite:sign-response", ...response }, "*");
      },
    );
  });
})();
