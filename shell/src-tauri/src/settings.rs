//! Shell-local settings (global hotkey binding, wake-word toggle).
//!
//! Deliberately **not** routed through the daemon's `config.sqlite`: the
//! global hotkey has to be registered by whichever process owns the OS
//! event loop, which is this Tauri process, not the headless daemon — so
//! there's no "one writer" violation (`ARCHITECTURE.md` §1) in keeping it
//! local, the daemon was never going to be the thing calling
//! `RegisterHotKey`/Carbon event taps either way. Persisted as a small JSON
//! file under the OS app-config dir rather than a new SQLite store, since
//! it's two fields and never queried, only loaded whole at startup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Accelerator string in `tauri_plugin_global_shortcut`'s own format,
    /// e.g. `"Ctrl+Shift+KeyV"` — stored pre-parsed-and-validated (every
    /// write went through a real `register()` call first, see
    /// `commands::apply_hotkey`), so a plain string round-trips with no
    /// lossy structured representation in between.
    pub hotkey: String,
    /// Persisted so the toggle survives restarts, but not wired to
    /// anything yet — no wake-word engine exists (`ROADMAP.md` M14, Phase
    /// 2). Same "real control, inert backend" pattern as the disabled
    /// "View in graph" / "Ask follow-up" buttons in `App.tsx`.
    #[serde(default)]
    pub wake_word: bool,
}

/// Per `UI.SPEC.md` §7: `Ctrl+Shift+V` on Windows/Linux, `Cmd+Shift+V` on
/// macOS (`Cmd` maps to this crate's `Super` modifier everywhere).
pub fn default_hotkey() -> String {
    if cfg!(target_os = "macos") {
        "Super+Shift+KeyV".to_string()
    } else {
        "Ctrl+Shift+KeyV".to_string()
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("settings.json"))
}

/// Falls back to defaults on any read/parse failure (missing file on first
/// run, hand-edited garbage, a future field a downgrade doesn't know about)
/// rather than erroring — settings are a convenience, not load-bearing
/// state, so "can't read it" should never block the app from starting.
pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Settings {
            hotkey: default_hotkey(),
            wake_word: false,
        })
}

pub fn save(app: &AppHandle, settings: &Settings) -> std::io::Result<()> {
    let path = settings_path(app)
        .ok_or_else(|| std::io::Error::other("no app config dir available on this platform"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(settings)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_requires_shift_and_a_primary_modifier() {
        let hotkey = default_hotkey();
        assert!(hotkey.contains("Shift"));
        assert!(hotkey.contains("Ctrl") || hotkey.contains("Super"));
        assert!(hotkey.ends_with("KeyV"));
    }

    #[test]
    fn settings_round_trip_through_json() {
        let original = Settings {
            hotkey: "Ctrl+Alt+F5".to_string(),
            wake_word: true,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hotkey, "Ctrl+Alt+F5");
        assert!(parsed.wake_word);
    }

    #[test]
    fn missing_wake_word_field_defaults_to_false() {
        let parsed: Settings = serde_json::from_str(r#"{"hotkey":"Ctrl+Shift+KeyV"}"#).unwrap();
        assert!(!parsed.wake_word);
    }
}
