//! The Claude Code side of the hook: read the event from stdin, keep only what
//! the island uses, and hand it over the local channel. Shared verbatim by the
//! tiny `island-hook` binary (std + serde_json only, so it starts in a few ms)
//! and by `dynamic-island hook`. Never fails loudly: if the island isn't
//! running, Claude Code must not notice.

use serde_json::{json, Map, Value};
use std::io::Read;

const MAX_FIELD: usize = 4000;

pub fn run() {
    // Taken first thing: hooks run async, so events can arrive out of order and
    // the island uses this to drop stale ones.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return;
    }
    if let Ok(ev) = serde_json::from_str::<Value>(&raw) {
        let mut msg = slim(&ev);
        msg["ts"] = json!(ts);
        let _ = send(msg.to_string().as_bytes());
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

/// Keep only the fields the island reads; cut long strings.
pub fn slim(ev: &Value) -> Value {
    let cut = |v: &Value| match v.as_str() {
        Some(s) if s.len() > MAX_FIELD => Value::String(truncate(s, MAX_FIELD)),
        _ => v.clone(),
    };
    let mut input = Map::new();
    if let Some(ti) = ev.get("tool_input").and_then(Value::as_object) {
        for k in [
            "file_path", "notebook_path", "path", "pattern", "url", "query", "command", "description",
            "old_string", "new_string", "content", "new_source",
        ] {
            if let Some(v) = ti.get(k) {
                input.insert(k.into(), cut(v));
            }
        }
        if let Some(first) = ti.get("edits").and_then(|e| e.get(0)) {
            input.insert(
                "edits".into(),
                json!([{ "old_string": cut(&first["old_string"]), "new_string": cut(&first["new_string"]) }]),
            );
        }
    }
    json!({
        "session_id": ev.get("session_id"),
        "cwd": ev.get("cwd"),
        "hook_event_name": ev.get("hook_event_name"),
        "tool_name": ev.get("tool_name"),
        "prompt": ev.get("prompt").map(cut),
        "message": ev.get("message").map(cut),
        "notification_type": ev.get("notification_type"),
        "tool_input": input,
    })
}

#[cfg(windows)]
pub const PIPE_NAME: &str = r"\\.\pipe\dynamic-island";

#[cfg(windows)]
pub fn send(msg: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    // ERROR_PIPE_BUSY (231): the island is between clients; retry briefly.
    for _ in 0..10 {
        match std::fs::OpenOptions::new().write(true).open(PIPE_NAME) {
            Ok(mut pipe) => return pipe.write_all(msg),
            Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(std::time::Duration::from_millis(15)),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join("dynamic-island.sock"),
        None => {
            extern "C" {
                fn getuid() -> u32;
            }
            PathBuf::from(format!("/tmp/dynamic-island-{}.sock", unsafe { getuid() }))
        }
    }
}

#[cfg(unix)]
pub fn send(msg: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::os::unix::net::UnixStream::connect(socket_path())?.write_all(msg)
}
