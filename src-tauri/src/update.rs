//! Updates. Reads `latest.json` from the newest GitHub release at startup and
//! every 12 h (if "Buscar actualizaciones" is on) and tells the UI when there
//! is a newer version.
//!
//! Installing is only automatic where it can be done safely: the Windows
//! installer (.exe, NSIS) and the MSI, through tauri-plugin-updater, which
//! checks the download against the public key in tauri.conf.json before running
//! it. The portable .zip and the Linux packages just get a link to the release.

use serde::{Deserialize, Serialize};
use std::{sync::Mutex, thread, time::Duration};
use tauri::{AppHandle, Emitter, Manager, State};

const LATEST_JSON: &str = "https://github.com/ibrarmz/ibrae/releases/latest/download/latest.json";
pub const RELEASES: &str = "https://github.com/ibrarmz/ibrae/releases/latest";
const EVERY: Duration = Duration::from_secs(12 * 3600);

#[derive(Serialize, Clone, Default, PartialEq)]
pub struct UpdateStatus {
    pub current: String,
    /// Newer version available, if any.
    pub latest: Option<String>,
    /// Can install by itself (Windows installer / MSI); otherwise "Descargar".
    pub can_install: bool,
    pub installing: bool,
    /// Spanish, ready to show.
    pub error: Option<String>,
    /// Epoch ms of the last successful check (0 = never).
    pub checked_at: i64,
}

#[derive(Default)]
pub struct UpdateState {
    status: Mutex<UpdateStatus>,
}

#[derive(Deserialize)]
struct Latest {
    version: String,
}

pub fn start(app: AppHandle) {
    {
        let state = app.state::<UpdateState>();
        let mut s = state.status.lock().unwrap();
        s.current = app.package_info().version.to_string();
        s.can_install = can_install();
    }
    thread::spawn(move || {
        // Let the island settle first; nobody needs this in the first seconds.
        thread::sleep(Duration::from_secs(30));
        loop {
            if crate::settings::get(&app).check_updates {
                check(&app);
            }
            thread::sleep(EVERY);
        }
    });
}

/// "0.1.12" → (0, 1, 12). Anything after a '-' (pre-release) is ignored.
fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let v = v.split(['-', '+']).next()?;
    let mut it = v.split('.').map(|n| n.parse::<u64>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

fn newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

/// The newest published version, or None when that release has no latest.json
/// (versions built without the signing key).
fn fetch_latest() -> Result<Option<String>, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut res = agent.get(LATEST_JSON).call().map_err(|_| "Sin conexión con GitHub".to_string())?;
    match res.status().as_u16() {
        200..=299 => {}
        404 => return Ok(None),
        code => return Err(format!("GitHub respondió {code}")),
    }
    let body = res.body_mut().read_to_string().map_err(|_| "La respuesta se cortó".to_string())?;
    serde_json::from_str::<Latest>(&body).map(|l| Some(l.version)).map_err(|_| "Respuesta inesperada".to_string())
}

fn check(app: &AppHandle) -> UpdateStatus {
    let result = fetch_latest();
    let state = app.state::<UpdateState>();
    let snap = {
        let mut s = state.status.lock().unwrap();
        match result {
            Ok(v) => {
                s.latest = v.filter(|v| newer(v, &s.current));
                s.error = None;
                s.checked_at = chrono::Utc::now().timestamp_millis();
            }
            Err(e) => s.error = Some(e),
        }
        s.clone()
    };
    let _ = app.emit("update", snap.clone());
    snap
}

/// Installed with the .exe (its uninstaller sits next to us) or the MSI.
fn can_install() -> bool {
    #[cfg(windows)]
    {
        use tauri::utils::{config::BundleType, platform::bundle_type};
        let exe = std::env::current_exe().ok();
        let nsis = exe.as_ref().and_then(|e| e.parent()).is_some_and(|d| d.join("uninstall.exe").exists());
        // The build marks the plain exe as "msi" too (the last bundle made), and
        // that exe also goes into the portable .zip: only trust it in Program Files.
        let in_program_files = exe.as_ref().is_some_and(|e| {
            ["ProgramW6432", "ProgramFiles"]
                .iter()
                .filter_map(std::env::var_os)
                .any(|pf| e.starts_with(std::path::PathBuf::from(pf)))
        });
        nsis || (bundle_type() == Some(BundleType::Msi) && in_program_files)
    }
    #[cfg(not(windows))]
    false
}

#[cfg_attr(not(windows), allow(dead_code))]
fn set(app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) {
    let state = app.state::<UpdateState>();
    let snap = {
        let mut s = state.status.lock().unwrap();
        f(&mut s);
        s.clone()
    };
    let _ = app.emit("update", snap);
}

#[tauri::command]
pub fn update_status(state: State<UpdateState>) -> UpdateStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<UpdateStatus, String> {
    tauri::async_runtime::spawn_blocking(move || check(&app)).await.map_err(|e| e.to_string())
}

/// Download, verify and install; the app closes and the installer reopens it.
/// Where that isn't possible, open the release page instead.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    if !can_install() {
        return crate::calendar::open_url(RELEASES.into());
    }
    #[cfg(windows)]
    {
        use tauri_plugin_updater::UpdaterExt;
        set(&app, |s| {
            s.installing = true;
            s.error = None;
        });
        let result = async {
            let update = app
                .updater()
                .map_err(|e| e.to_string())?
                .check()
                .await
                .map_err(|_| "No se pudo consultar la actualización".to_string())?
                .ok_or_else(|| "Ya tienes la última versión".to_string())?;
            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|_| "No se pudo descargar o verificar la actualización".to_string())
        }
        .await;
        // Only reached when it failed: a successful install closes the app.
        if let Err(e) = &result {
            let e = e.clone();
            set(&app, |s| {
                s.installing = false;
                s.error = Some(e);
            });
        }
        return result;
    }
    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("0.1.13", "0.1.12"));
        assert!(newer("0.2.0", "0.1.99"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.1.12", "0.1.12"));
        assert!(!newer("0.1.9", "0.1.12"));
        assert!(!newer("basura", "0.1.12"));
    }
}
