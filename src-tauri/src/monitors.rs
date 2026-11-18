//! Which monitor the island lives on ("Pantalla" in Ajustes).
//!
//! `settings.monitor` is "primary", "cursor" (follows the monitor the cursor
//! rests on) or a monitor key. Keys must survive reconnecting cables and
//! reordering displays, so on Windows they are the monitor's device path
//! (derived from its EDID) and the label is its real name ("DELL U2720Q");
//! elsewhere they are the name the system gives the output ("HDMI-1").

use serde::Serialize;
use tauri::WebviewWindow;

pub const PRIMARY: &str = "primary";
pub const CURSOR: &str = "cursor";

/// A monitor as the island needs it. Rect and scale are physical px.
#[derive(Clone, Debug, PartialEq)]
pub struct Mon {
    pub key: String,
    pub label: String,
    pub primary: bool,
    pub rect: (i32, i32, i32, i32),
    pub scale: f64,
}

impl Mon {
    pub fn contains(&self, (x, y): (f64, f64)) -> bool {
        let (mx, my, mw, mh) = self.rect;
        x >= mx as f64 && x < (mx + mw) as f64 && y >= my as f64 && y < (my + mh) as f64
    }
}

/// The monitors right now. Asks the windowing system, so it isn't free: callers
/// cache it (see `hit.rs`).
pub fn snapshot(win: &WebviewWindow) -> Vec<Mon> {
    let primary_name = win.primary_monitor().ok().flatten().and_then(|m| m.name().cloned());
    let names = platform::names();
    let mut out: Vec<Mon> = win
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let sys = m.name().cloned().unwrap_or_else(|| format!("monitor-{i}"));
            let (key, name) = names
                .iter()
                .find(|n| n.system == sys)
                .map(|n| (n.key.clone(), n.label.clone()))
                .unwrap_or_else(|| (sys.clone(), String::new()));
            let (w, h) = (m.size().width as i32, m.size().height as i32);
            let name = if name.is_empty() { format!("Pantalla {}", i + 1) } else { name };
            Mon {
                key,
                label: format!("{name} · {w}×{h}"),
                primary: primary_name.as_deref() == Some(sys.as_str()),
                rect: (m.position().x, m.position().y, w, h),
                scale: m.scale_factor(),
            }
        })
        .collect();
    // Two identical monitors: tell them apart.
    for i in 0..out.len() {
        if out.iter().filter(|m| m.label == out[i].label).count() > 1 {
            out[i].label = format!("{} ({})", out[i].label, i + 1);
        }
    }
    if !out.iter().any(|m| m.primary) {
        if let Some(first) = out.first_mut() {
            first.primary = true;
        }
    }
    out
}

/// Where the island should be, given the setting and (for "cursor") the
/// monitor it is following.
pub fn choose<'a>(mons: &'a [Mon], setting: &str, following: Option<&str>) -> Option<&'a Mon> {
    let primary = || mons.iter().find(|m| m.primary).or_else(|| mons.first());
    match setting {
        PRIMARY => primary(),
        CURSOR => following.and_then(|k| mons.iter().find(|m| m.key == k)).or_else(primary),
        // A chosen monitor that is unplugged: the primary until it comes back.
        key => mons.iter().find(|m| m.key == key).or_else(primary),
    }
}

#[derive(Serialize)]
pub struct MonitorOption {
    key: String,
    label: String,
    primary: bool,
}

/// For the "Pantalla" selector in Ajustes.
#[tauri::command]
pub fn monitors_list(window: WebviewWindow) -> Vec<MonitorOption> {
    snapshot(&window)
        .into_iter()
        .map(|m| MonitorOption { key: m.key, label: m.label, primary: m.primary })
        .collect()
}

/// System name (what tauri reports) → stable key and friendly label.
struct Named {
    system: String,
    key: String,
    label: String,
}

#[cfg(windows)]
mod platform {
    use super::Named;
    use windows::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
        DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };

    fn wide(s: &[u16]) -> String {
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        String::from_utf16_lossy(&s[..end])
    }

    /// GDI device name ("\\.\DISPLAY1", what tauri calls the monitor) → the
    /// monitor's device path and friendly name, through the DisplayConfig API.
    pub fn names() -> Vec<Named> {
        unsafe {
            let (mut np, mut nm) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm).is_err() {
                return Vec::new();
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
            if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), None)
                .is_err()
            {
                return Vec::new();
            }
            paths.truncate(np as usize);
            paths
                .iter()
                .filter_map(|p| {
                    let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                            size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                            adapterId: p.sourceInfo.adapterId,
                            id: p.sourceInfo.id,
                        },
                        ..Default::default()
                    };
                    if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                        return None;
                    }
                    let mut tgt = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                            size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                            adapterId: p.targetInfo.adapterId,
                            id: p.targetInfo.id,
                        },
                        ..Default::default()
                    };
                    let (key, label) = if DisplayConfigGetDeviceInfo(&mut tgt.header) == 0 {
                        (wide(&tgt.monitorDevicePath), wide(&tgt.monitorFriendlyDeviceName))
                    } else {
                        (String::new(), String::new())
                    };
                    let system = wide(&src.viewGdiDeviceName);
                    // Laptop panels often have no friendly name.
                    let label = if label.is_empty() && key.to_ascii_uppercase().contains("DISPLAY#") {
                        "Pantalla integrada".to_string()
                    } else {
                        label
                    };
                    let key = if key.is_empty() { system.clone() } else { key };
                    Some(Named { system, key, label })
                })
                .collect()
        }
    }
}

#[cfg(not(windows))]
mod platform {
    /// Output names ("HDMI-1", "eDP-1") are already stable and readable.
    pub fn names() -> Vec<super::Named> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(key: &str, primary: bool, x: i32) -> Mon {
        Mon { key: key.into(), label: key.into(), primary, rect: (x, 0, 1920, 1080), scale: 1.0 }
    }

    #[test]
    fn chooses_by_setting_with_primary_fallback() {
        let mons = [mon("a", true, 0), mon("b", false, 1920)];
        assert_eq!(choose(&mons, PRIMARY, None).unwrap().key, "a");
        assert_eq!(choose(&mons, "b", None).unwrap().key, "b");
        // Unplugged monitor → primary.
        assert_eq!(choose(&mons, "gone", None).unwrap().key, "a");
        assert_eq!(choose(&mons, CURSOR, Some("b")).unwrap().key, "b");
        assert_eq!(choose(&mons, CURSOR, Some("gone")).unwrap().key, "a");
        assert_eq!(choose(&mons, CURSOR, None).unwrap().key, "a");
    }

    #[test]
    fn contains_uses_half_open_rects() {
        let m = mon("b", false, 1920);
        assert!(m.contains((1920.0, 0.0)));
        assert!(!m.contains((1919.0, 10.0)));
        assert!(!m.contains((3840.0, 10.0)));
    }
}
