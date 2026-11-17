//! User settings, stored as settings.json in the app's config folder.
//! The UI sends partial updates; they are merged, validated, saved and
//! broadcast as a `settings` event so every part of the app stays in sync.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

pub const TABS: [&str; 6] = ["claude", "music", "focus", "notes", "sys", "cal"];

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "left" | "center" | "right"
    pub position: String,
    /// ms the cursor must rest on the island to open it: idle / something active.
    pub dwell_idle: u32,
    pub dwell_busy: u32,
    /// Fade the island when the cursor is just passing over it.
    pub ghost: bool,
    /// Tuck the island away behind maximized / fullscreen windows.
    pub hide_when_covered: bool,
    pub focus_mins: u32,
    pub break_mins: u32,
    /// Dock sections shown (Inicio and Configuración are always there).
    pub tabs: Vec<String>,
    /// Open by itself when Claude asks for permission.
    pub claude_peek: bool,
    /// How see-through the island's background is, in percent (0 = solid).
    pub transparency: u32,
    /// Look up HD covers in the iTunes catalog (sends artist + title). Opt-in.
    pub hd_art: bool,
    /// iCal feeds (the links are secrets: they only live in this file).
    pub calendars: Vec<crate::calendar::Source>,
    /// Show the next event in the pill this many minutes before (0 = never).
    pub cal_notify_mins: u32,
    /// Grow the pill into a "join" card a minute before an event.
    pub cal_peek: bool,
    pub cal_hide_allday: bool,
    /// Only events with a meeting link reach the pill.
    pub cal_only_links: bool,
    /// Look for a newer version on GitHub at startup and every 12 h.
    pub check_updates: bool,
    /// Which sections this file already knows about. A section added in a later
    /// version is switched on once (files without the field are 0).
    #[serde(default)]
    pub tabs_v: u32,
}

/// Bump when a section is added to `TABS`, and list it in `sanitize`.
const TABS_V: u32 = 1;

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: "center".into(),
            dwell_idle: 400,
            dwell_busy: 600,
            ghost: true,
            hide_when_covered: true,
            focus_mins: 25,
            break_mins: 5,
            tabs: TABS.iter().map(|s| s.to_string()).collect(),
            claude_peek: true,
            transparency: 0,
            hd_art: false,
            calendars: Vec::new(),
            cal_notify_mins: 15,
            cal_peek: true,
            cal_hide_allday: false,
            cal_only_links: false,
            check_updates: true,
            tabs_v: TABS_V,
        }
    }
}

impl Settings {
    fn sanitize(mut self) -> Self {
        if !matches!(self.position.as_str(), "left" | "center" | "right") {
            self.position = "center".into();
        }
        self.dwell_idle = self.dwell_idle.clamp(0, 2000);
        self.dwell_busy = self.dwell_busy.clamp(0, 2000);
        self.focus_mins = self.focus_mins.clamp(1, 60);
        self.break_mins = self.break_mins.clamp(1, 60);
        // Past 70 % the text loses its backing and becomes hard to read.
        self.transparency = self.transparency.min(70);
        if self.tabs_v < 1 && !self.tabs.iter().any(|t| t == "cal") {
            self.tabs.push("cal".into());
        }
        self.tabs_v = TABS_V;
        self.tabs.retain(|t| TABS.contains(&t.as_str()));
        self.tabs.dedup();
        self.cal_notify_mins = self.cal_notify_mins.min(120);
        self.calendars.retain(|c| !c.id.is_empty() && c.url.starts_with("https://"));
        self
    }
}

pub struct SettingsState {
    pub current: Mutex<Settings>,
    path: Mutex<Option<PathBuf>>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self { current: Mutex::new(Settings::default()), path: Mutex::new(None) }
    }
}

/// Snapshot of the current settings from anywhere in the core.
pub fn get(app: &AppHandle) -> Settings {
    app.state::<SettingsState>().current.lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sections_show_up_once_after_an_update() {
        let old: Settings = serde_json::from_str(r#"{"tabs":["music","notes"]}"#).unwrap();
        let s = old.sanitize();
        assert_eq!(s.tabs, ["music", "notes", "cal"]);
        // Hidden afterwards: stays hidden.
        let hidden: Settings = serde_json::from_value(serde_json::json!({ "tabs": ["music"], "tabs_v": s.tabs_v })).unwrap();
        assert_eq!(hidden.sanitize().tabs, ["music"]);
        assert!(Settings::default().sanitize().tabs.contains(&"cal".to_string()));
    }
}

pub fn load(app: &AppHandle) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    let path = dir.join("settings.json");
    let loaded = fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .unwrap_or_default()
        .sanitize();
    let state = app.state::<SettingsState>();
    *state.current.lock().unwrap() = loaded;
    *state.path.lock().unwrap() = Some(path);
}

fn save(state: &SettingsState, s: &Settings) -> Result<(), String> {
    let Some(path) = state.path.lock().unwrap().clone() else {
        return Err("sin carpeta de configuración".into());
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Merge `patch` into the current settings, save, apply and broadcast.
pub fn update(app: &AppHandle, patch: Value) -> Result<Settings, String> {
    let state = app.state::<SettingsState>();
    let (before, after) = {
        let mut cur = state.current.lock().unwrap();
        let mut merged = serde_json::to_value(&*cur).map_err(|e| e.to_string())?;
        if let (Some(obj), Some(p)) = (merged.as_object_mut(), patch.as_object()) {
            for (k, v) in p {
                obj.insert(k.clone(), v.clone());
            }
        }
        let next: Settings = serde_json::from_value::<Settings>(merged).map_err(|e| e.to_string())?.sanitize();
        let before = std::mem::replace(&mut *cur, next.clone());
        (before, next)
    };
    if before != after {
        save(&state, &after)?;
        if before.position != after.position {
            if let Some(win) = app.get_webview_window("island") {
                crate::hit::place_window(&win);
            }
        }
        if before.focus_mins != after.focus_mins || before.break_mins != after.break_mins {
            crate::focus::apply_defaults(app, after.focus_mins, after.break_mins);
        }
        if before.calendars != after.calendars {
            crate::calendar::refresh_now(app);
        }
        if after.hd_art && !before.hd_art {
            crate::media::upgrade_current(app);
        }
        let _ = app.emit("settings", after.clone());
    }
    Ok(after)
}

#[tauri::command]
pub fn settings_get(state: State<SettingsState>) -> Settings {
    state.current.lock().unwrap().clone()
}

#[tauri::command]
pub fn settings_set(app: AppHandle, patch: Value) -> Result<Settings, String> {
    update(&app, patch)
}

/// Name the autostart plugin registers under (the product name), and the one
/// used before the rename to IBRAE.
const AUTOSTART_NAME: &str = "IBRAE";
pub const LEGACY_AUTOSTART_NAME: &str = "Dynamic Island";

/// Used by the uninstallers, without a running app: drop the "start with the
/// system" entry so it doesn't point at a deleted executable.
pub fn remove_autostart_entry() {
    remove_autostart_named(AUTOSTART_NAME);
    remove_autostart_named(LEGACY_AUTOSTART_NAME);
}

/// Whether a "start with the system" entry exists under `name`.
pub fn autostart_entry_exists(name: &str) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::{
            core::{w, HSTRING},
            Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_ANY},
        };
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            &HSTRING::from(name),
            RRF_RT_ANY,
            None,
            None,
            None,
        )
        .is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("HOME")
            .is_some_and(|h| PathBuf::from(h).join(".config/autostart").join(format!("{name}.desktop")).exists())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = name;
        false
    }
}

pub fn remove_autostart_named(name: &str) {
    #[cfg(windows)]
    unsafe {
        use windows::{
            core::{w, HSTRING},
            Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER},
        };
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            &HSTRING::from(name),
        );
    }
    #[cfg(target_os = "linux")]
    if let Some(home) = std::env::var_os("HOME") {
        let file = PathBuf::from(home).join(".config/autostart").join(format!("{name}.desktop"));
        let _ = fs::remove_file(file);
    }
}

/// Start with the system (registry Run key on Windows, ~/.config/autostart on Linux).
#[tauri::command]
pub fn autostart(app: AppHandle, enable: Option<bool>) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    match enable {
        Some(true) => launcher.enable().map_err(|e| e.to_string())?,
        Some(false) => launcher.disable().map_err(|e| e.to_string())?,
        None => {}
    }
    launcher.is_enabled().map_err(|e| e.to_string())
}
