//! Tauri commands invoked from the Query UI and Settings Window frontends.

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tokio_stream::StreamExt;

use crate::daemon_client::DaemonClient;
use crate::dto::{AnswerChunkDto, GetGraphResponseDto};
use crate::settings::{self, Settings};
use crate::HotkeyState;

/// Streams a `Query` RPC's results to the frontend as events rather than a
/// single return value, so the UI can render Thinking -> Streaming ->
/// Answered as chunks actually arrive (`docs/UI.SPEC.md` §4), not all at
/// once after the whole response lands.
#[tauri::command]
pub async fn submit_query(
    app: AppHandle,
    daemon: State<'_, DaemonClient>,
    text: String,
) -> Result<(), String> {
    let mut stream = match daemon.query(text).await {
        Ok(stream) => stream,
        Err(err) => {
            let _ = app.emit("query-error", err.clone());
            return Err(err);
        }
    };

    loop {
        match stream.next().await {
            Some(Ok(chunk)) => {
                let _ = app.emit("query-chunk", AnswerChunkDto::from(chunk));
            }
            Some(Err(status)) => {
                let message = format!("query stream failed: {status}");
                let _ = app.emit("query-error", message.clone());
                return Err(message);
            }
            None => break,
        }
    }

    let _ = app.emit("query-done", ());
    Ok(())
}

/// Fetches the current document graph (`docs/UI.SPEC.md` §5e) for the
/// Graph Explorer window — a single request/response, unlike `Query`'s
/// stream, since the whole graph is small enough at prototype scale to
/// return in one shot.
#[tauri::command]
pub async fn get_graph(daemon: State<'_, DaemonClient>) -> Result<GetGraphResponseDto, String> {
    daemon.get_graph().await.map(GetGraphResponseDto::from)
}

/// The Settings Window's whole initial state in one call — just the two
/// persisted fields, no daemon round-trip (`settings.rs`'s doc comment
/// explains why this lives outside the daemon's stores).
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

/// Registers `shortcut` with the *real* OS global-hotkey API before
/// touching anything else — if `RegisterHotKey`/Carbon/the X11 equivalent
/// rejects it (already claimed by this or another app, unsupported key),
/// that `Err` is what reaches the UI, so "conflict" is never a guessed
/// reserved-shortcut list, it's the OS's own answer. Only unregisters the
/// previous binding and persists to disk once the new one is confirmed
/// live, so a failed rebind leaves the old hotkey working.
#[tauri::command]
pub fn set_hotkey(app: AppHandle, state: State<'_, HotkeyState>, shortcut: String) -> Result<String, String> {
    apply_hotkey(&app, &state, shortcut)
}

#[tauri::command]
pub fn reset_hotkey(app: AppHandle, state: State<'_, HotkeyState>) -> Result<String, String> {
    apply_hotkey(&app, &state, settings::default_hotkey())
}

fn apply_hotkey(app: &AppHandle, state: &State<'_, HotkeyState>, shortcut: String) -> Result<String, String> {
    if !shortcut.contains('+') {
        return Err(
            "Global shortcuts need at least one modifier key — a bare key would hijack every press of it, everywhere.".to_string(),
        );
    }

    let mut current = state.0.lock().expect("hotkey state mutex poisoned");
    if current.as_deref() == Some(shortcut.as_str()) {
        return Ok(shortcut); // No-op: already the live binding.
    }

    let gs = app.global_shortcut();
    gs.register(shortcut.as_str()).map_err(|err| err.to_string())?;

    if let Some(previous) = current.as_deref() {
        // Best-effort: if this ever fails, the OS still only fires our
        // handler (registered once, in lib.rs's setup) for whichever
        // shortcut(s) are live — a leftover previous binding is an extra
        // way *in*, not a broken one, and the next successful rebind will
        // try to clear it again.
        let _ = gs.unregister(previous);
    }

    *current = Some(shortcut.clone());
    drop(current);

    let mut settings = settings::load(app);
    settings.hotkey = shortcut.clone();
    settings::save(app, &settings).map_err(|err| err.to_string())?;

    Ok(shortcut)
}

#[tauri::command]
pub fn set_wake_word(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::load(&app);
    settings.wake_word = enabled;
    settings::save(&app, &settings).map_err(|err| err.to_string())
}
