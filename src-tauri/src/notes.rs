//! Quick notes: one plain-text file in the app's data folder
//! (%APPDATA%\dev.dynamicisland.island on Windows, ~/.local/share/… on Linux).

use std::{fs, path::PathBuf};
use tauri::{AppHandle, Manager};

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("notes.md"))
}

#[tauri::command]
pub fn notes_load(app: AppHandle) -> Result<String, String> {
    let p = path(&app)?;
    Ok(fs::read_to_string(p).unwrap_or_default())
}

/// Written to a temp file first, then renamed, so a crash never leaves half a note.
#[tauri::command]
pub fn notes_save(app: AppHandle, text: String) -> Result<(), String> {
    let p = path(&app)?;
    let tmp = p.with_extension("md.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &p).map_err(|e| e.to_string())
}
