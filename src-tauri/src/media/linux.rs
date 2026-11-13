//! Linux backend: MPRIS over the D-Bus session bus (Spotify, Firefox,
//! Chromium, Rhythmbox, VLC, mpv…). Event driven: wakes on PropertiesChanged
//! from any player, plus a slow 5 s resync (MPRIS never signals Position).

use super::{pretty_app, process_art, Art, MediaState, Msg, Track};
use std::{
    collections::HashMap,
    io::Read,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};
use zbus::{
    blocking::{fdo::DBusProxy, Connection, MessageIterator, Proxy},
    message::Type as MessageType,
    proxy::CacheProperties,
    zvariant::OwnedValue,
    MatchRule,
};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

pub fn start(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    *app.state::<MediaState>().tx.lock().unwrap() = Some(tx.clone());
    thread::spawn(move || {
        if let Err(e) = run(&app, tx, rx) {
            eprintln!("media (MPRIS) stopped: {e}");
        }
    });
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// Any player's properties changing (or players appearing/leaving) → refresh.
fn watch(conn: Connection, tx: Sender<Msg>) {
    let rules = [
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .and_then(|b| b.member("PropertiesChanged"))
            .and_then(|b| b.path(PATH))
            .map(|b| b.build()),
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface("org.freedesktop.DBus")
            .and_then(|b| b.member("NameOwnerChanged"))
            .and_then(|b| b.arg0ns("org.mpris.MediaPlayer2"))
            .map(|b| b.build()),
    ];
    for rule in rules.into_iter().flatten() {
        let (conn, tx) = (conn.clone(), tx.clone());
        thread::spawn(move || {
            if let Ok(it) = MessageIterator::for_match_rule(rule, &conn, Some(64)) {
                for _ in it {
                    let _ = tx.send(Msg::Refresh);
                }
            }
        });
    }
}

fn player<'a>(conn: &'a Connection, name: &'a str) -> zbus::Result<Proxy<'a>> {
    zbus::blocking::proxy::Builder::new(conn)
        .destination(name)?
        .path(PATH)?
        .interface(PLAYER)?
        .cache_properties(CacheProperties::No)
        .build()
}

/// The player to show: the first one playing, else the first paused, else any.
fn pick(conn: &Connection) -> Option<String> {
    let names: Vec<String> = DBusProxy::new(conn)
        .ok()?
        .list_names()
        .ok()?
        .into_iter()
        .map(|n| n.to_string())
        .filter(|n| n.starts_with(PREFIX))
        .collect();
    let status = |n: &String| {
        player(conn, n)
            .and_then(|p| p.get_property::<String>("PlaybackStatus"))
            .unwrap_or_default()
    };
    let statuses: Vec<(String, String)> = names.into_iter().map(|n| { let s = status(&n); (n, s) }).collect();
    statuses
        .iter()
        .find(|(_, s)| s == "Playing")
        .or_else(|| statuses.iter().find(|(_, s)| s == "Paused"))
        .or_else(|| statuses.first())
        .map(|(n, _)| n.clone())
}

fn text(meta: &HashMap<String, OwnedValue>, key: &str) -> String {
    meta.get(key).and_then(|v| String::try_from(v.try_clone().ok()?).ok()).unwrap_or_default()
}

fn micros(meta: &HashMap<String, OwnedValue>, key: &str) -> f64 {
    let Some(v) = meta.get(key) else { return 0.0 };
    let v = || v.try_clone().ok();
    v().and_then(|v| i64::try_from(v).ok())
        .map(|n| n as f64)
        .or_else(|| v().and_then(|v| u64::try_from(v).ok()).map(|n| n as f64))
        .unwrap_or(0.0)
        / 1_000_000.0
}

fn read(conn: &Connection, name: &str) -> zbus::Result<(Track, String)> {
    let p = player(conn, name)?;
    let status: String = p.get_property("PlaybackStatus")?;
    let meta: HashMap<String, OwnedValue> = p.get_property("Metadata")?;
    let position = p.get_property::<i64>("Position").unwrap_or(0) as f64 / 1_000_000.0;
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.try_clone().ok()?).ok())
        .map(|a| a.join(", "))
        .unwrap_or_default();
    let duration = micros(&meta, "mpris:length");
    let track = Track {
        active: true,
        app: pretty_app(name),
        title: text(&meta, "xesam:title"),
        artist,
        album: text(&meta, "xesam:album"),
        playing: status == "Playing",
        position: if duration > 0.0 { position.clamp(0.0, duration) } else { position },
        duration,
        measured_at: now_ms(),
    };
    Ok((track, text(&meta, "mpris:artUrl")))
}

/// Minimal percent-decoding for file:// URLs.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_art(url: &str) -> Option<Art> {
    let bytes = if let Some(path) = url.strip_prefix("file://") {
        std::fs::read(decode(path)).ok()?
    } else if url.starts_with("https://") || url.starts_with("http://") {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(4)))
            .build()
            .new_agent();
        let mut buf = Vec::new();
        agent.get(url).call().ok()?.into_body().into_reader().take(8_000_000).read_to_end(&mut buf).ok()?;
        buf
    } else {
        return None;
    };
    Some(process_art(&bytes))
}

fn control(conn: &Connection, name: &str, cmd: &str) {
    let method = match cmd {
        "toggle" => "PlayPause",
        "next" => "Next",
        "prev" => "Previous",
        _ => return,
    };
    if let Ok(p) = player(conn, name) {
        let _ = p.call_method(method, &());
    }
}

fn run(app: &AppHandle, tx: Sender<Msg>, rx: Receiver<Msg>) -> zbus::Result<()> {
    let conn = Connection::session()?;
    watch(conn.clone(), tx);

    let mut art_key = String::new();
    loop {
        let name = pick(&conn);
        let (track, art_url) = name
            .as_deref()
            .and_then(|n| read(&conn, n).ok())
            .unwrap_or_default();

        let key = format!("{}|{}|{}|{}", track.app, track.title, track.artist, art_url);
        if key != art_key {
            art_key = key;
            let art = if art_url.is_empty() { Art::default() } else { read_art(&art_url).unwrap_or_default() };
            app.state::<MediaState>().last.lock().unwrap().1 = art.clone();
            let _ = app.emit("media-art", art);
        }

        let state = app.state::<MediaState>();
        let mut last = state.last.lock().unwrap();
        if !last.0.same_as(&track) {
            last.0 = track.clone();
            drop(last);
            let _ = app.emit("media", track);
        }

        let handle = |m: Msg| {
            if let (Msg::Control(cmd), Some(n)) = (m, name.as_deref()) {
                control(&conn, n, &cmd);
            }
        };
        if let Ok(m) = rx.recv_timeout(Duration::from_secs(5)) {
            handle(m);
        }
        // Players emit bursts of PropertiesChanged on a track change; coalesce them.
        thread::sleep(Duration::from_millis(150));
        while let Ok(m) = rx.try_recv() {
            handle(m);
        }
    }
}
