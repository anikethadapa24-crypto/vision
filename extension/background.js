// Vision browser extension — background service worker.
//
// Talks to the daemon over its loopback HTTP bridge
// (daemon/vision-daemon/src/http_bridge.rs), the extension-facing half of
// the Local API Gateway (docs/ARCHITECTURE.md §4.2) — the gRPC/named-pipe
// transport every other client uses isn't reachable from a browser, which
// has no named-pipe API.
//
// Per-site indexing is opt-in (docs/UI.SPEC.md §5b/§5c "opt-in, never
// pre-checked"): nothing is captured until the user flips "Index this
// site" on in the popup for that origin, which also triggers a real Chrome
// host-permission prompt scoped to just that origin.

const BRIDGE = "http://127.0.0.1:47823";
const STORAGE_KEY = "visionEnabledOrigins";

function originOf(url) {
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

async function getEnabledOrigins() {
  const { [STORAGE_KEY]: origins } = await chrome.storage.local.get(STORAGE_KEY);
  return origins || [];
}

async function setEnabledOrigins(origins) {
  await chrome.storage.local.set({ [STORAGE_KEY]: origins });
}

async function checkConnected() {
  try {
    const resp = await fetch(`${BRIDGE}/health`, { method: "GET" });
    return resp.ok;
  } catch {
    return false;
  }
}

/** Extracts the active tab's visible text and posts it to the daemon. */
async function captureAndSend(tabId, url) {
  if (!url || !/^https?:\/\//.test(url)) {
    return { ok: false, error: "unsupported URL (not http/https)" };
  }

  let text = "";
  try {
    const results = await chrome.scripting.executeScript({
      target: { tabId },
      func: () => (document.body ? document.body.innerText : ""),
    });
    // Cap payload size — a page's full innerText can be huge; the daemon
    // chunks whatever it's given, so this just bounds one request's cost.
    text = (results[0]?.result || "").slice(0, 200000);
  } catch (e) {
    return { ok: false, error: `couldn't read page content: ${e}` };
  }

  try {
    const resp = await fetch(`${BRIDGE}/ingest`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url, text }),
    });
    const json = await resp.json();
    return { ok: resp.ok && json.accepted, ...json };
  } catch (e) {
    return { ok: false, error: `couldn't reach Vision: ${e}` };
  }
}

// Re-index an already-enabled origin whenever a tab there finishes loading
// (real "capture tabs" behavior, not just a one-off manual send) —
// per-navigation, not per-scroll/edit, matching what the daemon's watcher
// does for the filesystem (index on settle, not continuously).
chrome.tabs.onUpdated.addListener(async (tabId, changeInfo, tab) => {
  if (changeInfo.status !== "complete" || !tab.url) return;
  const origin = originOf(tab.url);
  if (!origin) return;
  const enabled = await getEnabledOrigins();
  if (enabled.includes(origin)) {
    await captureAndSend(tabId, tab.url);
  }
});

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  (async () => {
    if (message.type === "get-status") {
      const tab = message.tab;
      const origin = tab?.url ? originOf(tab.url) : null;
      const enabledOrigins = await getEnabledOrigins();
      const connected = await checkConnected();
      sendResponse({
        origin,
        enabled: origin ? enabledOrigins.includes(origin) : false,
        connected,
      });
      return;
    }

    if (message.type === "toggle") {
      const { origin, tabId, url } = message;
      const origins = await getEnabledOrigins();
      const alreadyEnabled = origins.includes(origin);

      if (alreadyEnabled) {
        await setEnabledOrigins(origins.filter((o) => o !== origin));
        sendResponse({ enabled: false, captureResult: null });
        return;
      }

      // Chrome's real permission prompt, scoped to exactly this origin —
      // no broader access is requested than what the user just opted into.
      const granted = await chrome.permissions.request({ origins: [`${origin}/*`] });
      if (!granted) {
        sendResponse({
          enabled: false,
          captureResult: { ok: false, error: "permission denied" },
        });
        return;
      }

      await setEnabledOrigins([...origins, origin]);
      const captureResult = await captureAndSend(tabId, url);
      sendResponse({ enabled: true, captureResult });
      return;
    }
  })();
  return true; // keep the message channel open for the async response above
});
