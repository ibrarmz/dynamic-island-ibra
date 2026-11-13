//! Memory used by the island itself: this process plus its children
//! (on Windows the WebView2 browser/renderer processes are descendants).

use serde::Serialize;
use std::{collections::HashSet, sync::Mutex};
#[cfg(not(windows))]
use sysinfo::Disks;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Created on the first request from the Sistema panel; CPU usage needs the
/// previous sample, so it is kept between calls.
#[derive(Default)]
pub struct SysState(Mutex<Option<System>>);

#[derive(Serialize)]
pub struct Battery {
    percent: u8,
    charging: bool,
}

#[derive(Serialize)]
pub struct Stats {
    /// 0–100 across all cores.
    cpu: f32,
    mem_used: u64,
    mem_total: u64,
    disk_used: u64,
    disk_total: u64,
    battery: Option<Battery>,
    uptime: u64,
}

/// Polled by the Sistema panel only while it is open. `async` so it runs on the
/// worker pool, never on the UI thread.
#[tauri::command]
pub async fn sys_stats(state: tauri::State<'_, SysState>) -> Result<Stats, ()> {
    let mut guard = state.0.lock().unwrap();
    if guard.is_none() {
        *guard = System::new().ok();
        // CPU usage is a delta between two samples: take a baseline so the first
        // reading the panel shows is real, not a meaningless spike.
        if let Some(sys) = guard.as_mut() {
            sys.refresh_cpu_usage();
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(200)));
        }
    }
    let (cpu, mem_used, mem_total) = match guard.as_mut() {
        Some(sys) => {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            // The very first sample has nothing to compare against and can be NaN.
            let cpu = sys.global_cpu_usage();
            (if cpu.is_finite() { cpu.clamp(0.0, 100.0) } else { 0.0 }, sys.used_memory(), sys.total_memory())
        }
        None => (0.0, 0, 0),
    };
    drop(guard);
    let (disk_used, disk_total) = system_disk();
    Ok(Stats {
        cpu,
        mem_used,
        mem_total,
        disk_used,
        disk_total,
        battery: battery(),
        uptime: System::uptime().unwrap_or(0),
    })
}

/// The disk holding the OS. Windows: ask for %SystemDrive% directly; listing all
/// disks would also query mapped network drives, which can hang for many seconds.
#[cfg(windows)]
fn system_disk() -> (u64, u64) {
    use windows::{core::HSTRING, Win32::Storage::FileSystem::GetDiskFreeSpaceExW};
    let root = format!("{}\\", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()));
    let (mut free, mut total) = (0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(&HSTRING::from(root), None, Some(&mut total), Some(&mut free)) };
    if ok.is_ok() { (total - free, total) } else { (0, 0) }
}

#[cfg(not(windows))]
fn system_disk() -> (u64, u64) {
    let Ok(disks) = Disks::new_with_refreshed_list() else {
        return (0, 0);
    };
    disks
        .iter()
        .find(|d| d.mount_point() == std::path::Path::new("/"))
        .map(|d| (d.total_space() - d.available_space(), d.total_space()))
        .unwrap_or((0, 0))
}

#[cfg(windows)]
fn battery() -> Option<Battery> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    // 128 = no system battery; 255 = unknown percentage.
    if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some(Battery { percent: s.BatteryLifePercent, charging: s.ACLineStatus == 1 })
}

#[cfg(target_os = "linux")]
fn battery() -> Option<Battery> {
    for entry in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let p = entry.path();
        let read = |f: &str| std::fs::read_to_string(p.join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
        if read("type") == "Battery" {
            let percent = read("capacity").parse().ok()?;
            let status = read("status");
            return Some(Battery { percent, charging: status == "Charging" || status == "Full" });
        }
    }
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
fn battery() -> Option<Battery> {
    None
}

#[tauri::command]
pub async fn island_memory() -> u64 {
    let Ok(mut sys) = System::new() else {
        return 0;
    };
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let me = Pid::from_u32(std::process::id());
    let mut family: HashSet<Pid> = HashSet::from([me]);
    // Walk the tree until no new descendants are found.
    loop {
        let before = family.len();
        for (pid, p) in sys.processes() {
            if let Some(parent) = p.parent() {
                if family.contains(&parent) {
                    family.insert(*pid);
                }
            }
        }
        if family.len() == before {
            break;
        }
    }
    let bytes: u64 = family
        .iter()
        .filter_map(|pid| sys.process(*pid).map(|p| (pid, p)))
        .map(|(pid, p)| private_bytes(pid.as_u32()).unwrap_or(p.memory()))
        .sum();
    bytes / (1024 * 1024)
}

/// Private (non-shared) memory. Working set would count the WebView2 DLLs
/// shared between its processes several times over.
#[cfg(windows)]
fn private_bytes(pid: u32) -> Option<u64> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        let ok = GetProcessMemoryInfo(
            handle,
            &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
            counters.cb,
        );
        let _ = CloseHandle(handle);
        ok.ok().map(|_| counters.PrivateUsage as u64)
    }
}

/// Linux: PSS (proportional set size) splits shared pages fairly between the
/// processes using them, so summing it over WebKitGTK's processes is honest.
#[cfg(target_os = "linux")]
fn private_bytes(pid: u32) -> Option<u64> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    let kb: u64 = text
        .lines()
        .find(|l| l.starts_with("Pss:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb * 1024)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn private_bytes(_pid: u32) -> Option<u64> {
    None
}
