// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod calendar;
mod claude;
mod focus;
mod hit;
#[cfg(target_os = "linux")]
mod kwin;
mod media;
mod notes;
mod settings;
mod sys;
#[cfg(target_os = "linux")]
mod webkit;

use std::sync::OnceLock;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

static APP: OnceLock<AppHandle> = OnceLock::new();

fn main() {
    // CLI modes share the binary so Claude Code needs nothing else installed.
    match std::env::args().nth(1).as_deref() {
        // Called by Claude Code for each hook event: forward it and exit.
        // (`island-hook` is the fast path; this stays as a fallback.)
        Some("hook") => return claude::hookclient::run(),
        // Also run by the uninstaller so no hook is left pointing at a deleted file.
        Some("hooks") => {
            let result = match std::env::args().nth(2).as_deref() {
                Some("install") => claude::hooks::install(),
                Some("uninstall") => claude::hooks::uninstall(),
                _ => Err("uso: dynamic-island hooks install|uninstall".into()),
            };
            if let Err(e) = result {
                eprintln!("{e}");
                std::process::exit(1);
            }
            return;
        }
        // Run by both uninstallers (NSIS and MSI): undo what lives outside the
        // install folder. Best effort; never blocks the uninstall.
        Some("uninstall-cleanup") => {
            let _ = claude::hooks::uninstall();
            settings::remove_autostart_entry();
            return;
        }
        _ => {}
    }

    // Linux: Wayland doesn't let a normal window position itself or stay on top
    // (GNOME has no layer-shell), so run through XWayland everywhere for now.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    #[cfg(target_os = "linux")]
    webkit::tune_env();

    // Claim the local channel; if another island owns it, this one isn't needed.
    let claimed = claude::ipc::serve(|bytes| {
        if let (Some(app), Ok(ev)) = (APP.get(), serde_json::from_slice(&bytes)) {
            claude::handle(app, ev);
        }
    });
    if !claimed {
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(settings::SettingsState::default())
        .manage(hit::HitState::default())
        .manage(focus::FocusState::default())
        .manage(media::MediaState::default())
        .manage(claude::ClaudeState::default())
        .manage(sys::SysState::default())
        .manage(calendar::CalState::default())
        .invoke_handler(tauri::generate_handler![
            settings::settings_get,
            settings::settings_set,
            settings::autostart,
            claude::claude_snapshot,
            claude::claude_hooks,
            hit::set_hit_rect,
            hit::hover_native,
            hit::release_focus,
            hit::covered,
            notes::notes_load,
            notes::notes_save,
            sys::sys_stats,
            sys::island_memory,
            focus::focus_get,
            focus::focus_toggle,
            focus::focus_reset,
            focus::focus_mode,
            focus::focus_minutes,
            media::media_snapshot,
            media::media_control,
            calendar::cal_snapshot,
            calendar::cal_add,
            calendar::cal_remove,
            calendar::cal_refresh,
            calendar::open_url,
        ])
        .setup(|app| {
            let _ = APP.set(app.handle().clone());
            settings::load(app.handle());
            let s = settings::get(app.handle());
            focus::apply_defaults(app.handle(), s.focus_mins, s.break_mins);
            claude::hooks::repair();

            let win = app.get_webview_window("island").expect("island window missing");
            #[cfg(target_os = "linux")]
            webkit::tune(&win);
            hit::place_window(&win);
            win.show()?;
            hit::start(win.clone());
            media::start(app.handle().clone());
            calendar::start(app.handle().clone());

            // Dev aid: ISLAND_PEEK=home,music,focus opens each panel in turn (for screenshots).
            if let Ok(tabs) = std::env::var("ISLAND_PEEK") {
                let w = win.clone();
                std::thread::spawn(move || {
                    for tab in tabs.split(',') {
                        std::thread::sleep(std::time::Duration::from_millis(2500));
                        let _ = w.emit("peek", tab.trim());
                    }
                });
            }

            let settings_item = MenuItem::with_id(app, "settings", "Configuración", true, None::<&str>)?;
            let toggle = MenuItem::with_id(app, "toggle", "Mostrar / ocultar", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &toggle, &sep, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("icon"))
                .tooltip("Dynamic Island")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    let win = app.get_webview_window("island");
                    match event.id.as_ref() {
                        "quit" => app.exit(0),
                        "settings" => {
                            if let Some(w) = win {
                                let _ = w.show();
                                let _ = w.emit("open-tab", "settings");
                            }
                        }
                        "toggle" => {
                            if let Some(w) = win {
                                if w.is_visible().unwrap_or(false) {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.show();
                                }
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running Dynamic Island")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                #[cfg(target_os = "linux")]
                kwin::stop();
            }
        });
}
