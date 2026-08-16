//! Tray App shell (`docs/UI.SPEC.md` §5a) + the Floating Query UI window
//! (§3/§4), wired to the daemon over the named-pipe transport
//! (`daemon_client.rs`) for real `Query` RPC results (`commands.rs`).

mod commands;
mod daemon_client;
mod dto;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use daemon_client::DaemonClient;

const QUERY_WINDOW: &str = "query";

fn toggle_query_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(QUERY_WINDOW) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn hide_query_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(QUERY_WINDOW) {
        let _ = window.hide();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DaemonClient::new())
        .invoke_handler(tauri::generate_handler![commands::submit_query])
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
            // "Settings…" are disabled placeholders — Graph Explorer and the
            // Settings window are separate, un-scoped milestones, not faked.
            let ask = MenuItem::with_id(app, "ask", "Ask Vision", true, None::<&str>)?;
            let explore = MenuItem::with_id(app, "explore", "Explore Graph", false, None::<&str>)?;
            let status = MenuItem::with_id(
                app,
                "status",
                "Vision — engine prototype",
                false,
                None::<&str>,
            )?;
            let settings = MenuItem::with_id(app, "settings", "Settings…", false, None::<&str>)?;
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
                "quit" => app.exit(0),
                _ => {}
            })
            .build(app)?;

            // Default hotkey per docs/UI.SPEC.md §7 (Ctrl+Shift+V; macOS's
            // Cmd+Shift+V equivalent would use Modifiers::SUPER, out of
            // scope for this Windows-first pass).
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
            app.global_shortcut().register(shortcut)?;

            // Click-outside dismiss (docs/UI.SPEC.md §3): hide, don't quit,
            // when the popup loses focus.
            if let Some(window) = app.get_webview_window(QUERY_WINDOW) {
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        hide_query_window(&handle);
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
