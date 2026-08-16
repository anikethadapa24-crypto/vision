//! Tray App shell (`docs/UI.SPEC.md` §5a) + the Floating Query UI window
//! (§3/§4), wired to the daemon over the named-pipe transport
//! (`daemon_client.rs`) for real `Query` RPC results (`commands.rs`). Also
//! owns the Settings Window (§5c's General tab, hotkey rebinding only so
//! far) and the real OS global-hotkey registration backing it
//! (`settings.rs`).

mod commands;
mod daemon_client;
mod dto;
mod settings;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use daemon_client::DaemonClient;

const QUERY_WINDOW: &str = "query";
const GRAPH_WINDOW: &str = "graph";
const SETTINGS_WINDOW: &str = "settings";

/// The accelerator string currently registered with the OS, mirrored here
/// so `commands::apply_hotkey` can unregister it before registering a
/// replacement without re-reading `settings.json` (which could have
/// drifted from what's actually live if a register call ever silently
/// failed).
pub struct HotkeyState(pub Mutex<Option<String>>);

/// A freshly-shown window on Windows reliably emits a spurious
/// `Focused(false)` before the real focus grant lands (observed directly:
/// the window opens then hides itself within a couple hundred ms with no
/// user input at all) — `set_focus()` racing the OS's own focus-settling
/// event, not a real click-outside. The click-outside-dismiss handler
/// below ignores `Focused(false)` for a short window after every
/// programmatic show, so only a real, later loss of focus dismisses it.
static SUPPRESS_NEXT_BLUR: AtomicBool = AtomicBool::new(false);
const BLUR_SUPPRESSION_MS: u64 = 400;

fn toggle_query_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(QUERY_WINDOW) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
        SUPPRESS_NEXT_BLUR.store(true, Ordering::SeqCst);
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(BLUR_SUPPRESSION_MS));
            SUPPRESS_NEXT_BLUR.store(false, Ordering::SeqCst);
        });
    }
}

fn hide_query_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(QUERY_WINDOW) {
        let _ = window.hide();
    }
}

fn show_graph_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(GRAPH_WINDOW) else {
        return;
    };
    let _ = window.show();
    let _ = window.set_focus();
}

fn show_settings_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(SETTINGS_WINDOW) else {
        return;
    };
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DaemonClient::new())
        .manage(HotkeyState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::submit_query,
            commands::get_graph,
            commands::get_settings,
            commands::set_hotkey,
            commands::reset_hotkey,
            commands::set_wake_word
        ])
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_query_window(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Menu order/labels per docs/UI.SPEC.md §5a. "Explore Graph" and
            // "Settings…" both open real windows (`GRAPH_WINDOW` /
            // `SETTINGS_WINDOW`).
            let ask = MenuItem::with_id(app, "ask", "Ask Vision", true, None::<&str>)?;
            let explore = MenuItem::with_id(app, "explore", "Explore Graph", true, None::<&str>)?;
            let status = MenuItem::with_id(
                app,
                "status",
                "Vision — engine prototype",
                false,
                None::<&str>,
            )?;
            let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Vision", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &ask,
                    &explore,
                    &PredefinedMenuItem::separator(app)?,
                    &status,
                    &settings,
                    &PredefinedMenuItem::separator(app)?,
                    &quit,
                ],
            )?;

            let app_icon = app.default_window_icon().cloned();
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(true);
            if let Some(icon) = app_icon {
                tray = tray.icon(icon);
            }
            tray.on_menu_event(|app, event| match event.id().as_ref() {
                "ask" => toggle_query_window(app),
                "explore" => show_graph_window(app),
                "settings" => show_settings_window(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .build(app)?;

            // Register the user's saved hotkey (docs/UI.SPEC.md §7 default:
            // Ctrl+Shift+V / Cmd+Shift+V — `settings::default_hotkey()` on
            // first run). If a saved binding no longer registers — another
            // app claimed it while Vision was closed, a settings.json
            // hand-edit, etc. — fall back to the default rather than
            // leaving the Settings Window's own hotkey rebind as the only
            // way to open the Query UI at all.
            let saved = settings::load(app.handle());
            let registered = match app.global_shortcut().register(saved.hotkey.as_str()) {
                Ok(()) => Some(saved.hotkey),
                Err(err) => {
                    eprintln!(
                        "saved hotkey {:?} failed to register ({err}); falling back to default",
                        saved.hotkey
                    );
                    let default = settings::default_hotkey();
                    app.global_shortcut()
                        .register(default.as_str())
                        .map(|()| default)
                        .ok()
                }
            };
            *app.state::<HotkeyState>().0.lock().unwrap() = registered;

            // Click-outside dismiss (docs/UI.SPEC.md §3): hide, don't quit,
            // when the popup loses focus.
            if let Some(window) = app.get_webview_window(QUERY_WINDOW) {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        if SUPPRESS_NEXT_BLUR.load(Ordering::SeqCst) {
                            return;
                        }
                        hide_query_window(&handle);
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
