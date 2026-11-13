//! What Claude Code is doing, fed by its hooks.
//!
//! Each hook event updates one session; the UI shows the most recently active
//! one. Tools map to phases: Read/Grep → LEER, Edit/Write → EDITAR,
//! Bash with test/build → PROBAR, Stop → LISTO.

pub mod hookclient;
pub mod hooks;
pub mod ipc;

use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};

const MAX_LINES: usize = 6;
const MAX_LINE_LEN: usize = 160;

#[derive(Serialize, Clone, Default)]
pub struct Diff {
    file: String,
    removed: Vec<String>,
    added: Vec<String>,
    add: usize,
    del: usize,
}

#[derive(Serialize, Clone)]
pub struct Session {
    id: String,
    project: String,
    prompt: String,
    /// idle | working | waiting | done
    status: &'static str,
    /// Spanish label shown in the UI (EDITANDO, PROBANDO…)
    label: &'static str,
    /// 0 read · 1 edit · 2 test · 3 done
    phase: usize,
    reached: [bool; 4],
    /// Current file / command / pattern.
    detail: String,
    tool: String,
    diff: Option<Diff>,
    started_at: u64,
    ended_at: Option<u64>,
    tools: u32,
    message: Option<String>,
    updated_at: u64,
    /// Hook start time of the newest event applied; older arrivals are stale.
    #[serde(skip)]
    last_ts: u64,
}

impl Session {
    fn new(id: String, now: u64) -> Self {
        Self {
            id,
            project: String::new(),
            prompt: String::new(),
            status: "idle",
            label: "LISTO",
            phase: 0,
            reached: [false; 4],
            detail: String::new(),
            tool: String::new(),
            diff: None,
            started_at: now,
            ended_at: None,
            tools: 0,
            message: None,
            updated_at: now,
            last_ts: 0,
        }
    }

    fn start_task(&mut self, prompt: &str, now: u64) {
        *self = Session {
            project: std::mem::take(&mut self.project),
            last_ts: self.last_ts,
            ..Session::new(std::mem::take(&mut self.id), now)
        };
        self.prompt = prompt.trim().to_string();
        self.status = "working";
        self.label = "PENSANDO";
    }

    fn tool_use(&mut self, tool: &str, input: &Value, now: u64) {
        if self.status != "working" {
            // Tools without a prompt we saw (e.g. resumed session): start timing now.
            if self.status != "waiting" {
                self.started_at = now;
                self.ended_at = None;
                self.reached = [false; 4];
            }
            self.status = "working";
        }
        self.message = None;
        self.tools += 1;
        self.tool = tool.to_string();

        let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
        let file = [s("file_path"), s("notebook_path"), s("path")].into_iter().find(|v| !v.is_empty()).unwrap_or("");

        let (phase, label, detail) = match tool {
            "Read" | "Grep" | "Glob" | "LS" | "WebFetch" | "WebSearch" | "NotebookRead" => {
                let d = [basename(file), s("pattern").into(), s("url").into(), s("query").into()]
                    .into_iter()
                    .find(|v| !v.is_empty())
                    .unwrap_or_default();
                (Some(0), "LEYENDO", d)
            }
            "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
                self.diff = Some(make_diff(tool, file, input));
                (Some(1), "EDITANDO", basename(file))
            }
            "Bash" | "PowerShell" => {
                let cmd = first_line(s("command"), 80);
                if is_check(&cmd) {
                    (Some(2), "PROBANDO", cmd)
                } else {
                    (None, "EJECUTANDO", cmd)
                }
            }
            "Task" | "Agent" => (None, "DELEGANDO", first_line(s("description"), 80)),
            "TodoWrite" => (None, "PLANEANDO", String::new()),
            other => (None, "TRABAJANDO", other.to_string()),
        };
        if let Some(p) = phase {
            self.phase = p;
            self.reached[p] = true;
        }
        self.label = label;
        self.detail = detail;
    }
}

#[derive(Default)]
pub struct ClaudeState {
    sessions: Mutex<HashMap<String, Session>>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn basename(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

fn first_line(s: &str, n: usize) -> String {
    truncate(s.lines().next().unwrap_or("").trim(), n)
}

/// Commands that check the work: tests, builds, linters, type checks.
fn is_check(cmd: &str) -> bool {
    let c = cmd.to_lowercase();
    [
        "test", "jest", "vitest", "pytest", "mocha", "cargo check", "cargo build", "cargo clippy",
        "npm run build", "pnpm build", "yarn build", "tsc", "lint", "dotnet build", "go vet", "go build",
        "mvn ", "gradle", "phpunit", "rspec",
    ]
    .iter()
    .any(|k| c.contains(k))
}

fn sample(text: &str) -> (Vec<String>, usize) {
    if text.is_empty() {
        return (Vec::new(), 0);
    }
    let lines: Vec<&str> = text.lines().collect();
    (lines.iter().take(MAX_LINES).map(|l| truncate(l, MAX_LINE_LEN)).collect(), lines.len())
}

fn make_diff(tool: &str, file: &str, input: &Value) -> Diff {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let (old, new) = match tool {
        "Write" => (String::new(), s(input, "content")),
        "MultiEdit" => input
            .get("edits")
            .and_then(|e| e.get(0))
            .map(|e| (s(e, "old_string"), s(e, "new_string")))
            .unwrap_or_default(),
        "NotebookEdit" => (String::new(), s(input, "new_source")),
        _ => (s(input, "old_string"), s(input, "new_string")),
    };
    let (removed, del) = sample(&old);
    let (added, add) = sample(&new);
    Diff { file: basename(file), removed, added, add, del }
}

fn latest(map: &HashMap<String, Session>) -> Option<Session> {
    map.values().max_by_key(|s| s.updated_at).cloned()
}

/// The island side: apply one hook event.
pub fn handle(app: &AppHandle, ev: Value) {
    let name = ev["hook_event_name"].as_str().unwrap_or("");
    let id = ev["session_id"].as_str().unwrap_or("default").to_string();
    let now = now_ms();
    let state = app.state::<ClaudeState>();
    let mut map = state.sessions.lock().unwrap();

    if name == "SessionEnd" {
        map.remove(&id);
        let _ = app.emit("claude", latest(&map));
        return;
    }

    let s = map.entry(id.clone()).or_insert_with(|| Session::new(id, now));
    // Async hooks may arrive out of order: ignore anything older than what we applied.
    let ts = ev["ts"].as_u64().unwrap_or(now);
    if ts < s.last_ts {
        return;
    }
    s.last_ts = ts;
    if let Some(cwd) = ev["cwd"].as_str() {
        s.project = basename(cwd.trim_end_matches(['/', '\\']));
    }
    s.updated_at = now;

    let mut peek = false;
    match name {
        "UserPromptSubmit" => s.start_task(ev["prompt"].as_str().unwrap_or(""), now),
        "PreToolUse" => s.tool_use(ev["tool_name"].as_str().unwrap_or(""), &ev["tool_input"], now),
        "PostToolUse" => {
            if s.status == "waiting" {
                s.status = "working";
                s.message = None;
            }
        }
        "Notification" => {
            let msg = ev["message"].as_str().unwrap_or("");
            // Only permission prompts matter; idle_prompt fires long after Stop.
            // Older Claude Code versions send no notification_type: fall back to the text.
            let is_permission = match ev["notification_type"].as_str() {
                Some(kind) => kind == "permission_prompt",
                None => !msg.to_lowercase().contains("waiting for your input"),
            };
            if is_permission {
                s.status = "waiting";
                s.label = "ESPERA";
                s.message = Some(msg.to_string());
                peek = true;
            }
        }
        "Stop" => {
            s.status = "done";
            s.label = "LISTO";
            s.phase = 3;
            s.reached = [true; 4];
            s.ended_at = Some(now);
            s.message = None;
        }
        _ => {}
    }
    let _ = app.emit("claude", latest(&map));
    drop(map);
    if peek {
        let _ = app.emit("peek", "claude");
    }
}

#[tauri::command]
pub fn claude_snapshot(state: State<ClaudeState>) -> Option<Session> {
    latest(&state.sessions.lock().unwrap())
}

/// action: "status" | "install" | "uninstall". Returns whether hooks are installed.
#[tauri::command]
pub fn claude_hooks(action: String) -> Result<bool, String> {
    match action.as_str() {
        "install" => hooks::install()?,
        "uninstall" => hooks::uninstall()?,
        _ => {}
    }
    hooks::installed()
}
