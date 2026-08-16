const dot = document.getElementById("status-dot");
const label = document.getElementById("status-label");
const toggle = document.getElementById("toggle");
const originEl = document.getElementById("origin");
const resultEl = document.getElementById("result");

let currentTab = null;

async function refresh() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  currentTab = tab;
  const status = await chrome.runtime.sendMessage({ type: "get-status", tab });

  dot.className = "dot " + (status.connected ? "dot--good" : "dot--critical");
  label.textContent = status.connected ? "Vision connected" : "Vision isn't running";

  toggle.checked = !!status.enabled;
  toggle.disabled = !status.origin;
  originEl.textContent = status.origin || "(unsupported page — not http/https)";
}

toggle.addEventListener("change", async () => {
  if (!currentTab || !currentTab.url) return;
  const origin = new URL(currentTab.url).origin;
  resultEl.textContent = toggle.checked ? "Indexing…" : "";

  const resp = await chrome.runtime.sendMessage({
    type: "toggle",
    origin,
    tabId: currentTab.id,
    url: currentTab.url,
  });

  toggle.checked = resp.enabled;
  if (resp.captureResult) {
    resultEl.textContent = resp.captureResult.ok
      ? `Indexed (${resp.captureResult.chunks_indexed ?? 0} chunk(s))`
      : `Failed: ${resp.captureResult.error || "unknown error"}`;
  } else {
    resultEl.textContent = "";
  }
});

refresh();
