//! One-time move from the old "Dynamic Island" name (≤ 0.1.x) to IBRAE.
//!
//! The data folder (the app identifier) never changed, so notes, settings and
//! calendars carry over by themselves. What changed is the install folder, the
//! executable and the "start with the system" entry. On Windows, the first time
//! IBRAE runs on a PC that still has the old install, it:
//! 1. notes whether Claude Code was connected and whether it started with Windows,
//! 2. backs up the data folder (the old uninstaller never deletes it when run
//!    silently, but this costs nothing),
//! 3. runs the old uninstaller silently (it removes its files, shortcuts,
//!    hooks and autostart entry),
//! 4. reconnects Claude Code and "start with Windows" for IBRAE if they were on.
//!
//! On Linux the old package stays (apt/dnf need root); the old autostart entry
//! is moved to IBRAE so only one island starts.

use tauri::AppHandle;

/// Executable name before the rename; a still-running one owns the Claude channel.
pub const LEGACY_EXE: &str = "dynamic-island";

pub fn start(app: AppHandle) {
    std::thread::spawn(move || run(&app));
}

/// Close a running pre-rename island so this one can take over the channel.
/// Returns whether one was found.
pub fn stop_legacy_island() -> bool {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};
    let Ok(sys) = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing())) else {
        return false;
    };
    let names: &[&str] = if cfg!(windows) { &["dynamic-island.exe"] } else { &[LEGACY_EXE] };
    let mut found = false;
    for p in sys.processes().values() {
        if names.iter().any(|n| p.name().eq_ignore_ascii_case(n)) {
            found |= p.kill();
        }
    }
    found
}

fn run(app: &AppHandle) {
    let had_autostart = crate::settings::autostart_entry_exists(crate::settings::LEGACY_AUTOSTART_NAME);

    #[cfg(windows)]
    if let Some(old) = legacy_install_dir() {
        let had_hooks = crate::claude::hooks::installed().unwrap_or(false);
        let had_desktop = shortcut_path("Desktop", "Dynamic Island").is_some_and(|p| p.exists());
        let backup = backup_data(app);
        let uninstaller = old.join("uninstall.exe");
        if uninstaller.exists() {
            // `_?=` runs it in place and makes us wait for it to finish.
            let _ = std::process::Command::new(&uninstaller)
                .arg("/S")
                .arg(format!("_?={}", old.display()))
                .status();
        }
        // Run in place, the uninstaller can't delete itself.
        let _ = std::fs::remove_dir_all(&old);
        if let Some(b) = backup {
            restore_data_if_missing(app, &b);
        }
        if had_hooks {
            let _ = crate::claude::hooks::install();
        }
        // The installer skips shortcuts when it runs as an update, and the old
        // uninstaller just removed "Dynamic Island" from the Start menu.
        ensure_shortcut(shortcut_path("StartMenu", "IBRAE"));
        if had_desktop {
            ensure_shortcut(shortcut_path("Desktop", "IBRAE"));
        }
    }

    if had_autostart {
        crate::settings::remove_autostart_named(crate::settings::LEGACY_AUTOSTART_NAME);
        use tauri_plugin_autostart::ManagerExt;
        let _ = app.autolaunch().enable();
    }
}

/// `%LOCALAPPDATA%\Dynamic Island`, if the old installer put it there and it
/// isn't where this copy runs from.
#[cfg(windows)]
fn legacy_install_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Dynamic Island");
    if !dir.join(format!("{LEGACY_EXE}.exe")).exists() {
        return None;
    }
    let here = std::env::current_exe().ok()?.parent()?.to_path_buf();
    (here != dir).then_some(dir)
}

#[cfg(windows)]
fn data_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    app.path().app_data_dir().ok()
}

/// Copy of the data folder in the temp folder (notes, settings, calendars).
#[cfg(windows)]
fn backup_data(app: &AppHandle) -> Option<std::path::PathBuf> {
    let src = data_dir(app)?;
    if !src.exists() {
        return None;
    }
    let dst = std::env::temp_dir().join(format!("ibrae-migration-{}", std::process::id()));
    copy_dir(&src, &dst).ok()?;
    Some(dst)
}

#[cfg(windows)]
fn restore_data_if_missing(app: &AppHandle, backup: &std::path::Path) {
    if let Some(dst) = data_dir(app) {
        if !dst.exists() {
            let _ = copy_dir(backup, &dst);
        }
    }
    let _ = std::fs::remove_dir_all(backup);
}

#[cfg(windows)]
fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

/// `<Start menu programs | Desktop>\<name>.lnk` for the current user.
#[cfg(windows)]
fn shortcut_path(place: &str, name: &str) -> Option<std::path::PathBuf> {
    let dir = match place {
        "StartMenu" => std::path::PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Windows\Start Menu\Programs"),
        _ => std::path::PathBuf::from(std::env::var_os("USERPROFILE")?).join("Desktop"),
    };
    Some(dir.join(format!("{name}.lnk")))
}

/// Create a shortcut to this executable if there isn't one. One-time, so the
/// shell's COM object through PowerShell is good enough.
#[cfg(windows)]
fn ensure_shortcut(path: Option<std::path::PathBuf>) {
    use std::os::windows::process::CommandExt;
    let (Some(lnk), Ok(exe)) = (path, std::env::current_exe()) else { return };
    if lnk.exists() {
        return;
    }
    let q = |p: &std::path::Path| p.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.Save()",
        q(&lnk),
        q(&exe),
        q(exe.parent().unwrap_or(&exe))
    );
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}
