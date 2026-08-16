import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./SettingsWindow.css";

/** Mirrors src-tauri/src/settings.rs's `Settings`. */
interface Settings {
  hotkey: string;
  wake_word: boolean;
}

type Platform = "mac" | "win" | "linux";

function detectPlatform(): Platform {
  const ua = navigator.userAgent;
  if (/Mac/i.test(ua) && !/iPhone|iPad/i.test(ua)) return "mac";
  if (/Win/i.test(ua)) return "win";
  return "linux";
}

interface Combo {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  super: boolean;
  key: string; // a JS KeyboardEvent.code value, e.g. "KeyV", "Digit5", "Escape"
}

const EMPTY_MODS = { ctrl: false, alt: false, shift: false, super: false };

const MOD_LABELS: Record<Platform, Record<"ctrl" | "alt" | "shift" | "super", string>> = {
  mac: { ctrl: "⌃", alt: "⌥", shift: "⇧", super: "⌘" },
  win: { ctrl: "Ctrl", alt: "Alt", shift: "Shift", super: "Win" },
  linux: { ctrl: "Ctrl", alt: "Alt", shift: "Shift", super: "Super" },
};

const NAMED_KEYS: Record<string, string> = {
  Space: "Space",
  Escape: "Esc",
  Enter: "↩",
  Tab: "Tab",
  Backspace: "⌫",
  Delete: "Del",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

// Physical modifier keys never finalize a combo on their own — held down,
// they update the live preview; only a non-modifier keydown commits.
const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

function keyLabel(code: string): string {
  if (NAMED_KEYS[code]) return NAMED_KEYS[code];
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}

/** Parses an accelerator string in `tauri_plugin_global_shortcut`'s own
 *  format ("Ctrl+Shift+KeyV") — the same format `comboToAccelerator` below
 *  builds and `settings::apply_hotkey` parses on the Rust side. */
function parseCombo(accelerator: string): Combo {
  const parts = accelerator.split("+");
  const key = parts.pop() ?? "";
  const mods = parts.map((p) => p.toUpperCase());
  return {
    ctrl: mods.includes("CTRL") || mods.includes("CONTROL"),
    alt: mods.includes("ALT") || mods.includes("OPTION"),
    shift: mods.includes("SHIFT"),
    super: mods.includes("SUPER") || mods.includes("CMD") || mods.includes("COMMAND"),
    key,
  };
}

function comboToAccelerator(combo: Combo): string {
  const parts: string[] = [];
  if (combo.ctrl) parts.push("Ctrl");
  if (combo.alt) parts.push("Alt");
  if (combo.shift) parts.push("Shift");
  if (combo.super) parts.push("Super");
  parts.push(combo.key);
  return parts.join("+");
}

function Keycombo({ combo, platform, small }: { combo: Combo; platform: Platform; small?: boolean }) {
  const L = MOD_LABELS[platform];
  const caps: string[] = [];
  if (combo.ctrl) caps.push(L.ctrl);
  if (combo.alt) caps.push(L.alt);
  if (combo.shift) caps.push(L.shift);
  if (combo.super) caps.push(L.super);
  caps.push(keyLabel(combo.key));
  return (
    <span className={`keycombo${small ? " keycombo--small" : ""}`}>
      {caps.map((c, i) => (
        <span className="keycap" key={i}>
          {c}
        </span>
      ))}
    </span>
  );
}

const SearchIcon = () => (
  <svg viewBox="0 0 24 24" fill="none">
    <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth="1.6" />
    <path d="M20 20L16 16" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);
const MicIcon = () => (
  <svg viewBox="0 0 24 24" fill="none">
    <rect x="9" y="3" width="6" height="11" rx="3" stroke="currentColor" strokeWidth="1.6" />
    <path d="M6 11a6 6 0 0 0 12 0M12 20v2" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);
const WarnIcon = () => (
  <svg viewBox="0 0 24 24" fill="none">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.6" />
    <path d="M12 8v5M12 16h.01" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  </svg>
);
const CheckIcon = () => (
  <svg viewBox="0 0 24 24" fill="none">
    <path d="M4 12l5 5L20 6" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

const FIND_COMBO_BY_PLATFORM: Record<Platform, Combo> = {
  mac: { ...EMPTY_MODS, super: true, key: "KeyK" },
  win: { ...EMPTY_MODS, ctrl: true, key: "KeyK" },
  linux: { ...EMPTY_MODS, ctrl: true, key: "KeyK" },
};

/**
 * Settings Window, General tab (`docs/UI.SPEC.md` §5c) — hotkey rebinding
 * only so far. Every change here goes straight through `set_hotkey`/
 * `reset_hotkey` to a *real* `tauri_plugin_global_shortcut` registration
 * (`src-tauri/src/commands.rs`) before it's ever shown as saved: there is
 * no "Save" step because there is nothing to stage — either the OS
 * accepted the new binding and it's already live, or it didn't and the
 * previous one still is.
 */
export default function SettingsWindow() {
  const platform = useRef(detectPlatform()).current;
  const [hotkey, setHotkey] = useState<string | null>(null);
  const [wakeWord, setWakeWord] = useState(false);
  const [recording, setRecording] = useState(false);
  const [liveMods, setLiveMods] = useState(EMPTY_MODS);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    void invoke<Settings>("get_settings").then((s) => {
      setHotkey(s.hotkey);
      setWakeWord(s.wake_word);
    });
  }, []);

  const showToast = useCallback((text: string) => {
    setToast(text);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 4000);
  }, []);

  const cancelRecording = useCallback(() => {
    setRecording(false);
    setLiveMods(EMPTY_MODS);
  }, []);

  useEffect(() => {
    if (!recording) return;

    function onKeyDown(e: KeyboardEvent) {
      if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
        e.preventDefault();
        cancelRecording();
        return;
      }
      e.preventDefault();
      e.stopPropagation();

      if (MODIFIER_CODES.has(e.code)) {
        setLiveMods({ ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, super: e.metaKey });
        return;
      }

      const combo: Combo = { ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, super: e.metaKey, key: e.code };
      if (!combo.ctrl && !combo.alt && !combo.shift && !combo.super) {
        setError("Global shortcuts need at least one modifier key.");
        return;
      }

      setRecording(false);
      setLiveMods(EMPTY_MODS);
      setError(null);
      setBusy(true);
      void invoke<string>("set_hotkey", { shortcut: comboToAccelerator(combo) })
        .then((saved) => {
          setHotkey(saved);
          showToast("Shortcut saved");
        })
        .catch((err: string) => setError(String(err)))
        .finally(() => setBusy(false));
    }

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("blur", cancelRecording);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("blur", cancelRecording);
    };
  }, [recording, cancelRecording, showToast]);

  function startRecording() {
    setError(null);
    setLiveMods(EMPTY_MODS);
    setRecording(true);
  }

  function reset() {
    setError(null);
    setBusy(true);
    void invoke<string>("reset_hotkey")
      .then((saved) => {
        setHotkey(saved);
        showToast("Restored default shortcut");
      })
      .catch((err: string) => setError(String(err)))
      .finally(() => setBusy(false));
  }

  const currentCombo = hotkey ? parseCombo(hotkey) : null;

  return (
    <div className="settings-root">
      <header className="settings-header">
        <div className="eyebrow">Settings · General</div>
        <h1>Keyboard Shortcuts</h1>
        <p className="subtitle">Choose how you summon Vision. Changes take effect immediately.</p>
      </header>

      <section className="card">
        <div className="row">
          <div className="row-icon" aria-hidden="true">
            <SearchIcon />
          </div>
          <div className="row-text">
            <div className="row-title">Open Vision</div>
            <div className="row-caption">Global hotkey — works from anywhere, even when Vision isn't focused.</div>
          </div>
          <div className="row-control">
            {currentCombo && !recording && <Keycombo combo={currentCombo} platform={platform} />}
            <button
              className="btn btn-ghost"
              onClick={recording ? cancelRecording : startRecording}
              disabled={busy}
              type="button"
            >
              {recording ? "Cancel" : "Change…"}
            </button>
          </div>
        </div>

        {recording && (
          <div className="recorder">
            <div className="recorder-status">
              <span className="rec-dot" aria-hidden="true" />
              Recording — press your new shortcut
            </div>
            <Keycombo combo={{ ...liveMods, key: "…" }} platform={platform} />
            <div className="recorder-hint">
              Hold your modifier keys, then tap a letter or number. <strong>Esc</strong> cancels.
            </div>
          </div>
        )}

        {error && (
          <div className="inline-message critical">
            <WarnIcon />
            <span>{error}</span>
          </div>
        )}

        <div className="divider" />

        <div className="row">
          <div className="row-icon" aria-hidden="true">
            <MicIcon />
          </div>
          <div className="row-text">
            <div className="row-title">&ldquo;Hey Vision&rdquo; wake word</div>
            <div className="row-caption">Coming soon — no wake-word engine yet (ROADMAP.md M14).</div>
          </div>
          <button
            className="switch"
            role="switch"
            aria-checked={wakeWord}
            aria-label="Toggle wake word — coming soon"
            disabled
            type="button"
          >
            <span className="switch-thumb" />
          </button>
        </div>

        <div className="divider" />

        <div className="row">
          <div className="row-text">
            <div className="row-title">Restore default</div>
            <div className="row-caption">Reset the global hotkey to Vision's default binding.</div>
          </div>
          <button className="btn btn-ghost" onClick={reset} disabled={busy || recording} type="button">
            Reset
          </button>
        </div>
      </section>

      <div className="section-label">
        Other shortcuts <span className="section-label-note">Fixed</span>
      </div>
      <section className="card">
        <div className="fixed-row">
          <Keycombo combo={{ ...EMPTY_MODS, key: "Escape" }} platform={platform} small />
          <div className="row-text">
            <div className="row-title">Dismiss overlay</div>
            <div className="row-caption">Query UI, Graph Explorer detail panel</div>
          </div>
        </div>
        <div className="divider" />
        <div className="fixed-row">
          <Keycombo combo={{ ...EMPTY_MODS, key: "Enter" }} platform={platform} small />
          <div className="row-text">
            <div className="row-title">Submit query</div>
            <div className="row-caption">Query UI, while the input is focused</div>
          </div>
        </div>
        <div className="divider" />
        <div className="fixed-row">
          <Keycombo combo={FIND_COMBO_BY_PLATFORM[platform]} platform={platform} small />
          <div className="row-text">
            <div className="row-title">Focus search-within-graph</div>
            <div className="row-caption">Graph Explorer</div>
          </div>
        </div>
      </section>

      <div className={`toast${toast ? " toast--show" : ""}`} role="status" aria-live="polite">
        <CheckIcon />
        <span>{toast}</span>
      </div>
    </div>
  );
}
