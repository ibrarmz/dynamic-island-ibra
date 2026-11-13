//! Now playing. Windows: SMTC (event driven). Linux: MPRIS, coming in phase 2.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Serialize;
use std::sync::{mpsc::Sender, Mutex};
use tauri::{AppHandle, State};

#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

#[derive(Serialize, Clone, Default, PartialEq)]
pub struct Track {
    pub active: bool,
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// Seconds into the track at `measured_at`.
    pub position: f64,
    pub duration: f64,
    /// Epoch ms when `position` was true; the UI extrapolates from here.
    pub measured_at: u64,
}

impl Track {
    /// Equal for the UI's purposes: same track and state, and the position is
    /// where extrapolating the previous reading says it should be. Avoids
    /// waking the UI just because the timestamp moved.
    pub fn same_as(&self, new: &Track) -> bool {
        let fields_equal = self.active == new.active
            && self.playing == new.playing
            && self.title == new.title
            && self.artist == new.artist
            && self.album == new.album
            && self.app == new.app
            && (self.duration - new.duration).abs() < 0.5;
        if !fields_equal {
            return false;
        }
        let elapsed = new.measured_at.saturating_sub(self.measured_at) as f64 / 1000.0;
        let expected = self.position + if self.playing { elapsed } else { 0.0 };
        (expected - new.position).abs() < 1.5
    }
}

#[derive(Serialize, Clone, Default)]
pub struct Art {
    /// Small JPEG data URL, or None.
    pub data: Option<String>,
    /// Dominant colour of the cover, for the glass tint.
    pub color: Option<[u8; 3]>,
}

pub enum Msg {
    Refresh,
    Control(String),
}

#[derive(Default)]
pub struct MediaState {
    pub last: Mutex<(Track, Art)>,
    pub tx: Mutex<Option<Sender<Msg>>>,
}

#[derive(Serialize)]
pub struct Snapshot {
    track: Track,
    art: Art,
}

#[tauri::command]
pub fn media_snapshot(state: State<MediaState>) -> Snapshot {
    let (track, art) = state.last.lock().unwrap().clone();
    Snapshot { track, art }
}

#[tauri::command]
pub fn media_control(state: State<MediaState>, cmd: String) {
    if let Some(tx) = state.tx.lock().unwrap().as_ref() {
        let _ = tx.send(Msg::Control(cmd));
    }
}

pub fn start(app: AppHandle) {
    #[cfg(windows)]
    windows::start(app);
    #[cfg(target_os = "linux")]
    linux::start(app);
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = app;
}

/// Friendly player name from a Windows AppUserModelId or an MPRIS bus name.
pub fn pretty_app(id: &str) -> String {
    let l = id.to_lowercase();
    for (needle, name) in [
        ("spotify", "Spotify"),
        ("msedge", "Edge"),
        ("chromium", "Chromium"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
        ("brave", "Brave"),
        ("zunemusic", "Reproductor multimedia"),
        ("applemusic", "Apple Music"),
        ("vlc", "VLC"),
        ("tidal", "TIDAL"),
        ("deezer", "Deezer"),
        ("rhythmbox", "Rhythmbox"),
        ("elisa", "Elisa"),
        ("lollypop", "Lollypop"),
        ("amberol", "Amberol"),
        ("strawberry", "Strawberry"),
        ("mpv", "mpv"),
    ] {
        if l.contains(needle) {
            return name.to_string();
        }
    }
    // Windows: "Vendor.App_hash!App" / "app.exe"; MPRIS: "org.mpris.MediaPlayer2.app.instance123".
    let tail = id.rsplit('!').next().unwrap_or(id);
    let tail = tail.strip_prefix("org.mpris.MediaPlayer2.").unwrap_or(tail);
    let tail = tail.split('.').next().unwrap_or(tail);
    let tail = tail.trim_end_matches(".exe").trim_end_matches(".EXE");
    let mut c = tail.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Downscale a cover, pick its dominant colour, re-encode as a small JPEG.
pub fn process_art(bytes: &[u8]) -> Art {
    let Ok(img) = image::load_from_memory(bytes) else {
        return Art::default();
    };
    let cover = img.thumbnail(256, 256).to_rgb8();
    let tiny = image::imageops::thumbnail(&cover, 16, 16);
    let (mut r, mut g, mut b, mut w) = (0f64, 0f64, 0f64, 0f64);
    for p in tiny.pixels() {
        let [pr, pg, pb] = p.0.map(|c| c as f64);
        let max = pr.max(pg).max(pb);
        let min = pr.min(pg).min(pb);
        // Favour saturated, bright pixels.
        let weight = (max - min) / 255.0 * (max / 255.0) + 0.02;
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        w += weight;
    }
    let (r, g, b) = (r / w, g / w, b / w);
    let k = (220.0 / r.max(g).max(b).max(1.0)).min(1.6);
    let color = [(r * k) as u8, (g * k) as u8, (b * k) as u8];

    let mut jpeg = Vec::new();
    let encoded = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 82)
        .encode_image(&cover)
        .is_ok();
    Art {
        data: encoded.then(|| format!("data:image/jpeg;base64,{}", B64.encode(&jpeg))),
        color: Some(color),
    }
}
