//! Pomodoro timer. Lives in Rust so it keeps running if the UI reloads,
//! and wakes up exactly once when the session ends (no ticking).

use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

#[derive(Serialize, Clone)]
pub struct Focus {
    pub mode: &'static str, // "focus" | "break"
    pub focus_mins: u32,
    pub break_mins: u32,
    pub running: bool,
    /// Epoch ms when the running session ends.
    pub end_at: u64,
    /// Remaining ms while paused / idle.
    pub remaining_ms: u64,
    pub sessions: u32,
}

impl Focus {
    fn mins(&self) -> u32 {
        if self.mode == "focus" { self.focus_mins } else { self.break_mins }
    }
}

pub struct FocusState {
    inner: Mutex<Focus>,
    generation: AtomicU64,
}

impl Default for FocusState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Focus {
                mode: "focus",
                focus_mins: 25,
                break_mins: 5,
                running: false,
                end_at: 0,
                remaining_ms: 25 * 60_000,
                sessions: 0,
            }),
            generation: AtomicU64::new(0),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn publish(app: &AppHandle, f: &Focus) {
    let _ = app.emit("focus", f.clone());
}

#[tauri::command]
pub fn focus_get(state: State<FocusState>) -> Focus {
    state.inner.lock().unwrap().clone()
}

#[tauri::command]
pub fn focus_toggle(app: AppHandle, state: State<FocusState>) -> Focus {
    let mut f = state.inner.lock().unwrap();
    let gen = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    if f.running {
        f.remaining_ms = f.end_at.saturating_sub(now_ms());
        f.running = false;
    } else {
        f.running = true;
        f.end_at = now_ms() + f.remaining_ms;
        let wait = f.remaining_ms;
        let app2 = app.clone();
        // One sleeping thread per run; a stale wake-up is ignored via `generation`.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(wait));
            complete(&app2, gen);
        });
    }
    publish(&app, &f);
    f.clone()
}

fn complete(app: &AppHandle, gen: u64) {
    let state = app.state::<FocusState>();
    if state.generation.load(Ordering::SeqCst) != gen {
        return; // paused, reset or restarted meanwhile
    }
    let mut f = state.inner.lock().unwrap();
    if !f.running {
        return;
    }
    let finished_focus = f.mode == "focus";
    if finished_focus {
        f.sessions += 1;
    }
    f.mode = if finished_focus { "break" } else { "focus" };
    f.running = false;
    f.remaining_ms = f.mins() as u64 * 60_000;
    publish(app, &f);
    let _ = app.emit("peek", "focus");
    let (title, body) = if finished_focus {
        ("Focus terminado", format!("Tómate {} minutos de descanso.", f.break_mins))
    } else {
        ("Descanso terminado", "Listo para otra sesión de focus.".to_string())
    };
    let _ = app.notification().builder().title(title).body(body).show();
}

#[tauri::command]
pub fn focus_reset(app: AppHandle, state: State<FocusState>) -> Focus {
    let mut f = state.inner.lock().unwrap();
    state.generation.fetch_add(1, Ordering::SeqCst);
    f.running = false;
    f.remaining_ms = f.mins() as u64 * 60_000;
    publish(&app, &f);
    f.clone()
}

#[tauri::command]
pub fn focus_mode(app: AppHandle, state: State<FocusState>, mode: String) -> Focus {
    let mut f = state.inner.lock().unwrap();
    state.generation.fetch_add(1, Ordering::SeqCst);
    f.mode = if mode == "break" { "break" } else { "focus" };
    f.running = false;
    f.remaining_ms = f.mins() as u64 * 60_000;
    publish(&app, &f);
    f.clone()
}

/// Set from the ruler: also becomes the saved default for that mode.
#[tauri::command]
pub fn focus_minutes(app: AppHandle, state: State<FocusState>, minutes: u32) -> Focus {
    let (snapshot, key) = {
        let mut f = state.inner.lock().unwrap();
        if f.running {
            return f.clone();
        }
        let m = minutes.clamp(1, 60);
        let key = if f.mode == "focus" { f.focus_mins = m; "focus_mins" } else { f.break_mins = m; "break_mins" };
        f.remaining_ms = m as u64 * 60_000;
        publish(&app, &f);
        (f.clone(), key)
    };
    // Lock released first: saving settings calls back into apply_defaults.
    let _ = crate::settings::update(&app, serde_json::json!({ key: minutes.clamp(1, 60) }));
    snapshot
}

/// New default lengths (startup or Configuración). A running session is left alone.
pub fn apply_defaults(app: &AppHandle, focus_mins: u32, break_mins: u32) {
    let state = app.state::<FocusState>();
    let mut f = state.inner.lock().unwrap();
    f.focus_mins = focus_mins;
    f.break_mins = break_mins;
    if !f.running {
        f.remaining_ms = f.mins() as u64 * 60_000;
    }
    publish(app, &f);
}
