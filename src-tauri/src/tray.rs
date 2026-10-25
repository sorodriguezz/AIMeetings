//! The tray icon: the app keeps recording or transcribing with its window closed, and this is how
//! the user gets back to it — or starts and stops a meeting without it.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// What the tray asks the window to do: `{ action: "start" | "stop" }`.
pub const TRAY_EVENT: &str = "tray:action";

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let english = !sys_locale_is_spanish();
    let open = MenuItem::with_id(app, "open", if english { "Open AIMeetings" } else { "Abrir AIMeetings" }, true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start", if english { "New meeting" } else { "Nueva reunión" }, true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", if english { "Stop recording" } else { "Detener grabación" }, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", if english { "Quit" } else { "Salir" }, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &start, &stop, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("AIMeetings")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "start" | "stop" => {
                show_main_window(app);
                let _ = app.emit(TRAY_EVENT, serde_json::json!({ "action": event.id.as_ref() }));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn sys_locale_is_spanish() -> bool {
    ["LANG", "LC_ALL", "LC_MESSAGES"].iter().filter_map(|k| std::env::var(k).ok()).any(|v| v.to_lowercase().starts_with("es"))
}
