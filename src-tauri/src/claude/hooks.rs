//! Adds / removes the island's hooks in ~/.claude/settings.json.
//! Other hooks and settings are left untouched; a backup is written first.

use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const EVENTS: [(&str, bool); 7] = [
    // (event, needs a tool matcher)
    ("PreToolUse", true),
    ("PostToolUse", true),
    ("UserPromptSubmit", false),
    ("Notification", false),
    ("Stop", false),
    ("SessionStart", false),
    ("SessionEnd", false),
];

fn settings_path() -> Result<PathBuf, String> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or("no se encontró la carpeta del usuario")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

/// Prefer the tiny `island-hook` next to the app (starts in a few ms);
/// fall back to the app binary's `hook` mode.
fn command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let fast = exe.with_file_name(if cfg!(windows) { "island-hook.exe" } else { "island-hook" });
    // Forward slashes: Claude Code runs hooks through a POSIX shell on Windows too.
    let quote = |p: &std::path::Path| format!("\"{}\"", p.to_string_lossy().replace('\\', "/"));
    Ok(if fast.exists() { quote(&fast) } else { format!("{} hook", quote(&exe)) })
}

fn is_ours(hook: &Value) -> bool {
    hook["command"].as_str().is_some_and(|c| {
        c.contains("island-hook") || (c.contains("dynamic-island") && c.trim_end().ends_with(" hook"))
    })
}

fn load() -> Result<(PathBuf, Value), String> {
    let path = settings_path()?;
    let value = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("settings.json no es JSON válido: {e}"))?,
        Err(_) => json!({}),
    };
    Ok((path, value))
}

fn save(path: &PathBuf, value: &Value) -> Result<(), String> {
    if path.exists() {
        fs::copy(path, path.with_extension("json.bak-dynamic-island")).map_err(|e| e.to_string())?;
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, text + "\n").map_err(|e| e.to_string())
}

/// Removes our entries; when `add` is set, appends a fresh one per event.
fn rewrite(add: bool) -> Result<(), String> {
    let (path, mut settings) = load()?;
    let cmd = command()?;
    if !settings.is_object() {
        return Err("settings.json no es un objeto JSON".into());
    }
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err("\"hooks\" en settings.json no es un objeto".into());
    };
    for (event, matcher) in EVENTS {
        let mut groups: Vec<Value> = hooks.get(event).and_then(Value::as_array).cloned().unwrap_or_default();
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                list.retain(|h| !is_ours(h));
            }
        }
        groups.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|l| !l.is_empty()));
        if add {
            // async: runs in the background, so Claude never waits on the island.
            let mut group = json!({ "hooks": [{ "type": "command", "command": cmd, "async": true, "timeout": 5 }] });
            if matcher {
                group["matcher"] = json!("*");
            }
            groups.push(group);
        }
        if groups.is_empty() {
            hooks.remove(event);
        } else {
            hooks.insert(event.into(), Value::Array(groups));
        }
    }
    save(&path, &settings)
}

pub fn install() -> Result<(), String> {
    rewrite(true)
}

pub fn uninstall() -> Result<(), String> {
    rewrite(false)
}

/// At startup: if the user connected Claude Code earlier but the hook now points
/// at a file that no longer exists (moved, reinstalled, dev build deleted) or
/// predates async hooks, rewrite it for this copy. Any island listens on the same
/// channel, so a hook pointing at another existing copy is left alone.
pub fn repair() {
    let Ok((_, settings)) = load() else { return };
    let hooks: Vec<&Value> = settings["hooks"]
        .as_object()
        .into_iter()
        .flat_map(|h| h.values())
        .flat_map(|groups| groups.as_array().into_iter().flatten())
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .filter(|h| is_ours(h))
        .collect();
    let stale = hooks.iter().any(|h| {
        let cmd = h["command"].as_str().unwrap_or("");
        let path = cmd.split('"').nth(1).unwrap_or("");
        h["async"] != Value::Bool(true) || path.is_empty() || !std::path::Path::new(path).exists()
    });
    if !hooks.is_empty() && stale {
        let _ = install();
    }
}

pub fn installed() -> Result<bool, String> {
    let (_, settings) = load()?;
    Ok(settings["hooks"]
        .as_object()
        .is_some_and(|h| h.values().flat_map(|g| g.as_array().into_iter().flatten()).any(|g| {
            g["hooks"].as_array().is_some_and(|l| l.iter().any(is_ours))
        })))
}
