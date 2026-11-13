//! Windows backend: System Media Transport Controls (the same source as the
//! volume flyout), so it works with Spotify, browsers, Apple Music, VLC, etc.
//! Event driven: we only wake up when Windows says something changed, plus a
//! slow 5 s resync for players that under-report timeline changes.

use super::{pretty_app, process_art, Art, MediaState, Msg, Track};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};
use windows::{
    core::Result,
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Storage::Streams::DataReader,
    Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
};

pub fn start(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    *app.state::<MediaState>().tx.lock().unwrap() = Some(tx.clone());
    thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        if let Err(e) = run(&app, tx, rx) {
            eprintln!("media backend stopped: {e}");
        }
    });
}

struct Hooked {
    session: Session,
    id: String,
    tokens: [i64; 3],
}

fn notify(tx: &Sender<Msg>) -> impl Fn() + Send + 'static {
    let tx = tx.clone();
    move || {
        let _ = tx.send(Msg::Refresh);
    }
}

fn run(app: &AppHandle, tx: Sender<Msg>, rx: Receiver<Msg>) -> Result<()> {
    let mgr = SessionManager::RequestAsync()?.join()?;
    let n = notify(&tx);
    mgr.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        n();
        Ok(())
    }))?;
    let n = notify(&tx);
    mgr.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        n();
        Ok(())
    }))?;

    let mut hooked: Option<Hooked> = None;
    let mut art_key = String::new();
    let mut art_missing = false;
    let mut art_retries = 0u8;

    loop {
        let session = mgr.GetCurrentSession().ok();
        let id = session
            .as_ref()
            .and_then(|s| s.SourceAppUserModelId().ok())
            .map(|h| h.to_string())
            .unwrap_or_default();

        // Re-subscribe when the active player changes.
        let hooked_id = hooked.as_ref().map(|h| h.id.as_str());
        let current_id = session.as_ref().map(|_| id.as_str());
        if hooked_id != current_id {
            if let Some(h) = hooked.take() {
                unhook(&h);
            }
            if let Some(s) = &session {
                hooked = hook(s, id.clone(), &tx).ok();
            }
        }

        let track = session
            .as_ref()
            .and_then(|s| read_track(s, &id).ok())
            .unwrap_or_default();

        let key = format!("{}|{}|{}|{}", track.app, track.title, track.artist, track.album);
        if key != art_key || (art_missing && art_retries < 5) {
            art_retries = if key != art_key { 0 } else { art_retries + 1 };
            art_key = key;
            let art = session.as_ref().and_then(read_art).unwrap_or_default();
            art_missing = track.active && art.data.is_none();
            super::publish_art(app, &track, art);
        }

        // Hold the lock only for the comparison: it must not be held across the
        // wait below, or every reader (UI snapshot, HD cover lookup) blocks.
        let changed = {
            let state = app.state::<MediaState>();
            let mut last = state.last.lock().unwrap();
            let changed = !last.0.same_as(&track);
            if changed {
                last.0 = track.clone();
            }
            changed
        };
        if changed {
            let _ = app.emit("media", track);
        }

        // Sleep until Windows reports a change (or a control command arrives).
        let wait = if art_missing && art_retries < 5 { 1 } else { 5 };
        if let Ok(Msg::Control(cmd)) = rx.recv_timeout(Duration::from_secs(wait)) {
            if let Some(s) = &session {
                control(s, &cmd);
            }
        }
        // A track change fires a burst of events; coalesce them.
        thread::sleep(Duration::from_millis(120));
        while let Ok(m) = rx.try_recv() {
            if let (Msg::Control(cmd), Some(s)) = (m, &session) {
                control(s, &cmd);
            }
        }
    }
}

fn hook(s: &Session, id: String, tx: &Sender<Msg>) -> Result<Hooked> {
    let n = notify(tx);
    let t1 = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
        n();
        Ok(())
    }))?;
    let n = notify(tx);
    let t2 = s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
        n();
        Ok(())
    }))?;
    let n = notify(tx);
    let t3 = s.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
        n();
        Ok(())
    }))?;
    Ok(Hooked { session: s.clone(), id, tokens: [t1, t2, t3] })
}

fn unhook(h: &Hooked) {
    let _ = h.session.RemoveMediaPropertiesChanged(h.tokens[0]);
    let _ = h.session.RemovePlaybackInfoChanged(h.tokens[1]);
    let _ = h.session.RemoveTimelinePropertiesChanged(h.tokens[2]);
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// 100 ns ticks â†’ seconds.
fn ticks_s(t: i64) -> f64 {
    t as f64 / 10_000_000.0
}

fn read_track(s: &Session, id: &str) -> Result<Track> {
    let props = s.TryGetMediaPropertiesAsync()?.join()?;
    let info = s.GetPlaybackInfo()?;
    let tl = s.GetTimelineProperties()?;
    let playing = info.PlaybackStatus()? == Status::Playing;
    let duration = ticks_s(tl.EndTime()?.Duration - tl.StartTime()?.Duration);
    let mut position = ticks_s(tl.Position()?.Duration);
    let now = now_ms();
    if playing {
        // FILETIME (100 ns since 1601) â†’ epoch ms.
        let updated = (tl.LastUpdatedTime()?.UniversalTime - 116_444_736_000_000_000) / 10_000;
        if updated > 0 && (updated as u64) <= now {
            position += (now - updated as u64) as f64 / 1000.0;
        }
    }
    if duration > 0.0 {
        position = position.clamp(0.0, duration);
    }
    Ok(Track {
        active: true,
        app: pretty_app(id),
        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        album: props.AlbumTitle()?.to_string(),
        playing,
        position,
        duration,
        measured_at: now,
    })
}

fn read_art(s: &Session) -> Option<Art> {
    let props = s.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 || size > 10_000_000 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    reader.LoadAsync(size).ok()?.join().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(process_art(&buf))
}

fn control(s: &Session, cmd: &str) {
    let op = match cmd {
        "toggle" => s.TryTogglePlayPauseAsync(),
        "next" => s.TrySkipNextAsync(),
        "prev" => s.TrySkipPreviousAsync(),
        _ => return,
    };
    if let Ok(op) = op {
        let _ = op.join();
    }
}

