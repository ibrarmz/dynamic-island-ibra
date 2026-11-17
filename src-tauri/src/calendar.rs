//! Calendar: read-only iCal (.ics) feeds, e.g. Google Calendar's "secret
//! address in iCal format" (also Outlook, iCloud, Proton…). No account, no
//! OAuth: the link itself is the key, and it is only stored on this PC.
//!
//! A worker thread downloads each feed every 5 minutes (or right away when the
//! list changes), expands recurring events around today and pushes the result
//! as a `calendar` event. Nothing touches the network while no calendar is set.

use chrono::{DateTime, Duration as CDur, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use rrule::{RRule, RRuleSet, Tz, Unvalidated};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    sync::{
        mpsc::{self, Sender},
        Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

/// A calendar the user added (stored in settings).
#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(default)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub url: String,
    pub color: String,
}

/// One occurrence, as the UI shows it. Times are epoch ms.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Event {
    pub id: String,
    pub cal: String,
    pub color: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// Meet / Zoom / Teams / Webex link, if the event has one.
    pub link: Option<String>,
}

#[derive(Serialize, Clone, PartialEq, Default)]
pub struct FeedStatus {
    pub id: String,
    /// Spanish, ready to show; None when the last download worked.
    pub error: Option<String>,
    /// Epoch ms of the last good download (0 = never).
    pub updated: i64,
}

#[derive(Serialize, Clone, PartialEq, Default)]
pub struct Snapshot {
    pub events: Vec<Event>,
    pub feeds: Vec<FeedStatus>,
}

#[derive(Default)]
pub struct CalState {
    last: Mutex<Snapshot>,
    tx: Mutex<Option<Sender<()>>>,
}

pub const COLORS: [&str; 6] = ["#7fb2ff", "#8fd6a0", "#ffb37a", "#d6a2ff", "#ff9c94", "#7fd8d8"];
const EVERY: Duration = Duration::from_secs(5 * 60);

pub fn start(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    *app.state::<CalState>().tx.lock().unwrap() = Some(tx);
    thread::spawn(move || {
        // Last good download per calendar id, kept across failures.
        let mut cache: HashMap<String, (String, i64)> = HashMap::new();
        loop {
            refresh(&app, &mut cache);
            let _ = rx.recv_timeout(EVERY);
            // Several changes in a row (add + rename…): one refresh.
            thread::sleep(Duration::from_millis(200));
            while rx.try_recv().is_ok() {}
        }
    });
}

/// Download again now (the list of calendars changed, or the user asked).
pub fn refresh_now(app: &AppHandle) {
    if let Some(tx) = app.state::<CalState>().tx.lock().unwrap().as_ref() {
        let _ = tx.send(());
    }
}

fn refresh(app: &AppHandle, cache: &mut HashMap<String, (String, i64)>) {
    let sources = crate::settings::get(app).calendars;
    cache.retain(|id, _| sources.iter().any(|s| &s.id == id));
    let mut feeds = Vec::new();
    for s in &sources {
        let error = match fetch(&s.url) {
            Ok(text) => {
                cache.insert(s.id.clone(), (text, Utc::now().timestamp_millis()));
                None
            }
            Err(e) => Some(e),
        };
        feeds.push(FeedStatus { id: s.id.clone(), error, updated: cache.get(&s.id).map_or(0, |c| c.1) });
    }

    let today = Local::now().date_naive();
    let from = local_midnight(today - CDur::days(1));
    let to = local_midnight(today + CDur::days(2));
    let mut events: Vec<Event> = sources
        .iter()
        .filter_map(|s| cache.get(&s.id).map(|(text, _)| (s, text)))
        .flat_map(|(s, text)| {
            let mut ev = expand(&parse(text).events, from, to);
            for e in &mut ev {
                e.id = format!("{}|{}", s.id, e.id);
                e.cal = s.id.clone();
                e.color = s.color.clone();
            }
            ev
        })
        .collect();
    // All-day first, then by time.
    events.sort_by(|a, b| {
        b.all_day.cmp(&a.all_day).then(a.start.cmp(&b.start)).then(a.end.cmp(&b.end)).then(a.title.cmp(&b.title))
    });

    let snap = Snapshot { events, feeds };
    let state = app.state::<CalState>();
    let changed = {
        let mut last = state.last.lock().unwrap();
        let changed = *last != snap;
        if changed {
            *last = snap.clone();
        }
        changed
    };
    if changed {
        let _ = app.emit("calendar", snap);
    }
}

fn local_midnight(d: NaiveDate) -> DateTime<Utc> {
    let naive = d.and_time(NaiveTime::MIN);
    Local
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive).with_timezone(&Local))
        .with_timezone(&Utc)
}

// ---------------------------------------------------------------- download

/// Spanish error messages: they go straight to the UI.
fn fetch(url: &str) -> Result<String, String> {
    let url = normalize_url(url)?;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut res = agent.get(&url).call().map_err(|_| "Sin conexión o el servidor no responde".to_string())?;
    match res.status().as_u16() {
        200..=299 => {}
        // Google's "public address" only works for public calendars; the one
        // people mean to copy is the secret address further down the page.
        401 | 403 | 404 if is_google_public(&url) => {
            return Err("Ese es el link público y tu calendario no es público. Copia la «Dirección secreta en formato iCal» (más abajo, en la misma página)".into())
        }
        401 | 403 | 404 | 410 => return Err("El link ya no es válido (¿lo restableciste?)".into()),
        code => return Err(format!("El servidor respondió {code}")),
    }
    let mut buf = Vec::new();
    res.body_mut()
        .as_reader()
        .take(15_000_000)
        .read_to_end(&mut buf)
        .map_err(|_| "La descarga se cortó".to_string())?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    if !text.contains("BEGIN:VCALENDAR") {
        return Err("Ese link no es un calendario iCal".into());
    }
    Ok(text)
}

fn is_google_public(url: &str) -> bool {
    url.contains("calendar.google.com/calendar/ical/") && url.contains("/public/")
}

/// `webcal://` is what calendar apps hand out; it is plain https.
pub fn normalize_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    let url = match url.strip_prefix("webcal://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_string(),
    };
    if !url.starts_with("https://") || url.len() < 12 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Pega un link que empiece con https:// o webcal://".into());
    }
    Ok(url)
}

// ---------------------------------------------------------------- parsing

#[derive(Clone, Debug)]
struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

#[derive(Default, Debug)]
struct VEvent {
    uid: String,
    summary: String,
    start: Option<Prop>,
    end: Option<Prop>,
    duration: Option<String>,
    rrule: Option<String>,
    exdates: Vec<Prop>,
    recurrence_id: Option<Prop>,
    cancelled: bool,
    /// Fields that may hold a meeting link, best first.
    link_fields: Vec<String>,
}

#[derive(Default)]
struct Parsed {
    name: Option<String>,
    events: Vec<VEvent>,
}

/// Lines of the file with folded continuations joined back (RFC 5545 §3.1).
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(last) = out.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    out
}

/// `NAME;P1=a;P2="b:c":value`
fn parse_prop(line: &str) -> Option<Prop> {
    let mut quoted = false;
    let colon = line.char_indices().find(|&(_, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        c == ':' && !quoted
    })?;
    let (head, value) = (&line[..colon.0], &line[colon.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_uppercase(), v.trim().trim_matches('"').to_string()))
        .collect();
    Some(Prop { name, params, value: value.to_string() })
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    let mut cur: Option<VEvent> = None;
    // Components nested inside an event (VALARM) are skipped.
    let mut nested = 0;
    for line in unfold(text) {
        let Some(p) = parse_prop(&line) else { continue };
        match (p.name.as_str(), p.value.to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") => {
                cur = Some(VEvent::default());
                nested = 0;
                continue;
            }
            ("END", "VEVENT") => {
                if let Some(ev) = cur.take() {
                    parsed.events.push(ev);
                }
                continue;
            }
            ("BEGIN", _) if cur.is_some() => nested += 1,
            ("END", _) if cur.is_some() => nested -= 1,
            _ => {}
        }
        let Some(ev) = cur.as_mut() else {
            if p.name == "X-WR-CALNAME" && !p.value.trim().is_empty() {
                parsed.name = Some(unescape(p.value.trim()));
            }
            continue;
        };
        if nested > 0 {
            continue;
        }
        match p.name.as_str() {
            "UID" => ev.uid = p.value.clone(),
            "SUMMARY" => ev.summary = unescape(&p.value),
            "DTSTART" => ev.start = Some(p),
            "DTEND" => ev.end = Some(p),
            "DURATION" => ev.duration = Some(p.value),
            "RRULE" => ev.rrule = Some(p.value),
            "EXDATE" => ev.exdates.push(p),
            "RECURRENCE-ID" => ev.recurrence_id = Some(p),
            "STATUS" => ev.cancelled = p.value.eq_ignore_ascii_case("CANCELLED"),
            "X-GOOGLE-CONFERENCE" => ev.link_fields.insert(0, p.value),
            "URL" | "LOCATION" | "DESCRIPTION" | "X-MICROSOFT-SKYPETEAMSMEETINGURL" => {
                ev.link_fields.push(unescape(&p.value))
            }
            _ => {}
        }
    }
    parsed
}

/// The calendar's own name, for a newly added link.
pub fn calendar_name(text: &str) -> Option<String> {
    parse(text).name
}

/// Times: `20251117T103000Z` (UTC), `TZID=America/Mexico_City:20251117T103000`,
/// floating `20251117T103000` (local) or a date `20251117` (all day, local).
fn parse_time(value: &str, tzid: Option<&str>) -> Option<(DateTime<Tz>, bool)> {
    let v = value.trim();
    if v.len() == 8 {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        let dt = Tz::LOCAL.from_local_datetime(&d.and_time(NaiveTime::MIN)).earliest()?;
        return Some((dt, true));
    }
    if let Some(utc) = v.strip_suffix('Z') {
        let n = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some((Tz::UTC.from_utc_datetime(&n), false));
    }
    let n = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    // Unknown names (Outlook's "Pacific Standard Time"…) fall back to local time.
    let tz = tzid
        .map(|t| t.trim_start_matches('/'))
        .and_then(|t| t.parse::<chrono_tz::Tz>().ok())
        .map(Tz::from)
        .unwrap_or(Tz::LOCAL);
    let dt = tz.from_local_datetime(&n).earliest()?;
    Some((dt, false))
}

fn prop_time(p: &Prop) -> Option<(DateTime<Tz>, bool)> {
    parse_time(&p.value, p.param("TZID"))
}

/// `P1D`, `PT1H30M`, `-PT15M`, `P1W`.
fn parse_duration(s: &str) -> Option<CDur> {
    let s = s.trim();
    let (neg, s) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let s = s.strip_prefix('+').unwrap_or(s).strip_prefix('P')?;
    let mut total = CDur::zero();
    let mut num = String::new();
    for c in s.chars() {
        match c {
            '0'..='9' => num.push(c),
            'T' => {}
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match c {
                    'W' => CDur::weeks(n),
                    'D' => CDur::days(n),
                    'H' => CDur::hours(n),
                    'M' => CDur::minutes(n),
                    'S' => CDur::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(if neg { -total } else { total })
}

const MEETING_HOSTS: [&str; 7] = [
    "meet.google.com",
    "zoom.us",
    "zoom.com",
    "teams.microsoft.com",
    "teams.live.com",
    "webex.com",
    "whereby.com",
];

/// First video-call link in the given texts.
fn find_link(fields: &[String]) -> Option<String> {
    for text in fields {
        let mut rest = text.as_str();
        while let Some(i) = rest.find("https://") {
            let tail = &rest[i..];
            let end = tail
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '\\' | '|'))
                .unwrap_or(tail.len());
            let url = tail[..end].trim_end_matches(['.', ',', ';', ':']);
            let host = url["https://".len()..].split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
            if MEETING_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}"))) {
                return Some(url.to_string());
            }
            rest = &tail[end.max(1)..];
        }
    }
    None
}

// ---------------------------------------------------------------- expansion

/// Occurrences of `events` overlapping [from, to).
fn expand(events: &[VEvent], from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Event> {
    // Single occurrences moved or cancelled by a separate VEVENT (same UID +
    // RECURRENCE-ID): the series skips them.
    let overridden: HashSet<(String, i64)> = events
        .iter()
        .filter_map(|e| {
            let (t, _) = prop_time(e.recurrence_id.as_ref()?)?;
            Some((e.uid.clone(), t.timestamp()))
        })
        .collect();

    let mut out = Vec::new();
    for ev in events {
        if ev.cancelled {
            continue;
        }
        let Some((start, all_day)) = ev.start.as_ref().and_then(prop_time) else { continue };
        let length = ev
            .end
            .as_ref()
            .and_then(prop_time)
            .map(|(end, _)| end.signed_duration_since(start))
            .or_else(|| ev.duration.as_deref().and_then(parse_duration))
            .unwrap_or(if all_day { CDur::days(1) } else { CDur::zero() })
            .max(CDur::zero());
        let link = find_link(&ev.link_fields);
        // Calendars shared as "free/busy only" (Google's public address on many
        // work accounts) name every event "Busy".
        let title = match ev.summary.trim() {
            "" => "(Sin título)".to_string(),
            "Busy" => "Ocupado".to_string(),
            t => t.to_string(),
        };

        let starts: Vec<DateTime<Tz>> = match (&ev.rrule, &ev.recurrence_id) {
            (Some(rule), None) => occurrences(rule, &ev.exdates, start, from - length, to)
                .into_iter()
                .filter(|t| !overridden.contains(&(ev.uid.clone(), t.timestamp())))
                .collect(),
            _ => vec![start],
        };
        for s in starts {
            let (s_utc, e_utc) = (s.with_timezone(&Utc), s.with_timezone(&Utc) + length);
            // Zero-length events still show if they start inside the window.
            if s_utc < to && (e_utc > from || (length.is_zero() && s_utc >= from)) {
                out.push(Event {
                    id: format!("{}|{}", ev.uid, s_utc.timestamp()),
                    cal: String::new(),
                    color: String::new(),
                    title: title.clone(),
                    start: s_utc.timestamp_millis(),
                    end: e_utc.timestamp_millis(),
                    all_day,
                    link: link.clone(),
                });
            }
        }
    }
    out
}

fn occurrences(rule: &str, exdates: &[Prop], start: DateTime<Tz>, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<DateTime<Tz>> {
    // UNTIL is applied here, converted to UTC, because the crate is strict about
    // its time zone and feeds aren't (all-day UNTIL dates, local times…).
    let mut until = None;
    let rest: Vec<&str> = rule
        .split(';')
        .filter(|part| match part.split_once('=') {
            Some((k, v)) if k.eq_ignore_ascii_case("UNTIL") => {
                until = parse_time(v, None).map(|(t, all_day)| {
                    let t = t.with_timezone(&Tz::UTC);
                    // A date-only UNTIL includes that whole day.
                    if all_day { t + CDur::days(1) - CDur::seconds(1) } else { t }
                });
                false
            }
            _ => true,
        })
        .collect();
    let Ok(mut parsed) = rest.join(";").parse::<RRule<Unvalidated>>() else { return vec![start] };
    if let Some(u) = until {
        if u < start {
            return vec![];
        }
        parsed = parsed.until(u);
    }
    let Ok(valid) = parsed.validate(start) else { return vec![start] };
    let mut set = RRuleSet::new(start).rrule(valid);
    for ex in exdates {
        for v in ex.value.split(',') {
            if let Some((t, _)) = parse_time(v, ex.param("TZID")) {
                set = set.exdate(t);
            }
        }
    }
    let tz = start.timezone();
    set.after(from.with_timezone(&tz) - CDur::seconds(1))
        .before(to.with_timezone(&tz))
        .all(500)
        .dates
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn cal_snapshot(state: State<'_, CalState>) -> Result<Snapshot, ()> {
    Ok(state.last.lock().unwrap().clone())
}

/// Check the link really is a calendar, then save it (and show the Calendar tab).
#[tauri::command]
pub async fn cal_add(app: AppHandle, url: String) -> Result<crate::settings::Settings, String> {
    let url = normalize_url(&url)?;
    let current = crate::settings::get(&app);
    if current.calendars.iter().any(|c| c.url == url) {
        return Err("Ese calendario ya está agregado".into());
    }
    let text = {
        let url = url.clone();
        tauri::async_runtime::spawn_blocking(move || fetch(&url)).await.map_err(|e| e.to_string())??
    };
    let n = current.calendars.len();
    let name = calendar_name(&text).unwrap_or_else(|| format!("Calendario {}", n + 1));
    let used: Vec<&str> = current.calendars.iter().map(|c| c.color.as_str()).collect();
    let color = COLORS.iter().find(|c| !used.contains(c)).unwrap_or(&COLORS[n % COLORS.len()]).to_string();
    let id = format!("c{}", Utc::now().timestamp_millis());

    let mut calendars = current.calendars.clone();
    calendars.push(Source { id, name, url, color });
    let mut tabs = current.tabs.clone();
    if !tabs.iter().any(|t| t == "cal") {
        tabs.push("cal".into());
    }
    crate::settings::update(&app, serde_json::json!({ "calendars": calendars, "tabs": tabs }))
}

#[tauri::command]
pub fn cal_remove(app: AppHandle, id: String) -> Result<crate::settings::Settings, String> {
    let mut calendars = crate::settings::get(&app).calendars;
    calendars.retain(|c| c.id != id);
    crate::settings::update(&app, serde_json::json!({ "calendars": calendars }))
}

/// Asked by the connect screen while it is open: the copied text, but only if
/// it is an iCal link not added yet. Nothing else from the clipboard ever
/// leaves this function.
#[tauri::command]
pub async fn cal_clipboard(app: AppHandle) -> Option<String> {
    let text = tauri::async_runtime::spawn_blocking(|| {
        // One clipboard handle for the whole run (on Linux each one is an X connection).
        static CLIP: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);
        let mut clip = CLIP.lock().unwrap();
        if clip.is_none() {
            *clip = arboard::Clipboard::new().ok();
        }
        clip.as_mut()?.get_text().ok()
    })
    .await
    .ok()??;
    let url = ical_link(&text)?;
    let added = crate::settings::get(&app).calendars.iter().any(|c| c.url == url);
    (!added).then_some(url)
}

/// A calendar feed link: webcal://, a .ics file, or Google's /calendar/ical/ path.
fn ical_link(text: &str) -> Option<String> {
    let t = text.trim();
    if t.len() > 2000 {
        return None;
    }
    let url = normalize_url(t).ok()?;
    let lower = url.to_ascii_lowercase();
    let path = lower.split(['?', '#']).next().unwrap_or("");
    let webcal = t.get(..9).is_some_and(|p| p.eq_ignore_ascii_case("webcal://"));
    (webcal || path.ends_with(".ics") || path.contains("calendar.google.com/calendar/ical/")).then_some(url)
}

#[tauri::command]
pub fn cal_refresh(app: AppHandle) {
    refresh_now(&app);
}

/// Open a meeting link in the default browser. https only.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Link no válido".into());
    }
    #[cfg(windows)]
    unsafe {
        use windows::{
            core::{w, HSTRING, PCWSTR},
            Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        };
        let r = ShellExecuteW(None, w!("open"), &HSTRING::from(url.as_str()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        if r.0 as isize <= 32 {
            return Err("No se pudo abrir el navegador".into());
        }
    }
    #[cfg(not(windows))]
    std::process::Command::new("xdg-open").arg(&url).spawn().map_err(|_| "No se pudo abrir el navegador".to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r
X-WR-CALNAME:Trabajo\r
BEGIN:VTIMEZONE\r
TZID:America/Mexico_City\r
BEGIN:STANDARD\r
DTSTART:19700101T000000\r
END:STANDARD\r
END:VTIMEZONE\r
BEGIN:VEVENT\r
UID:daily@x\r
DTSTART;TZID=America/Mexico_City:20251103T103000\r
DTEND;TZID=America/Mexico_City:20251103T104500\r
RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;UNTIL=20251231T060000Z\r
EXDATE;TZID=America/Mexico_City:20251118T103000\r
SUMMARY:Daily del equipo\r
X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r
DESCRIPTION:Únete con Google Meet: https://meet.google.com/abc-defg-hij\\n\r
  Más opciones\r
BEGIN:VALARM\r
ACTION:DISPLAY\r
DESCRIPTION:This is an event reminder\r
END:VALARM\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:daily@x\r
RECURRENCE-ID;TZID=America/Mexico_City:20251119T103000\r
DTSTART;TZID=America/Mexico_City:20251119T120000\r
DTEND;TZID=America/Mexico_City:20251119T121500\r
SUMMARY:Daily del equipo (movida)\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:allday@x\r
DTSTART;VALUE=DATE:20251117\r
DTEND;VALUE=DATE:20251118\r
SUMMARY:Entrega v0.2\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:zoom@x\r
DTSTART:20251117T180000Z\r
DURATION:PT1H\r
SUMMARY:Revisión\\, diseño\r
LOCATION:https://us02web.zoom.us/j/123456?pwd=abc.\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:gone@x\r
DTSTART:20251117T190000Z\r
STATUS:CANCELLED\r
SUMMARY:Cancelada\r
END:VEVENT\r
END:VCALENDAR\r
";

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn window() -> Vec<Event> {
        // Nov 17 – 20 2025 (Mexico City is UTC-6 all year).
        expand(&parse(ICS).events, utc("2025-11-17T00:00:00Z"), utc("2025-11-21T00:00:00Z"))
    }

    #[test]
    fn reads_name() {
        assert_eq!(calendar_name(ICS).as_deref(), Some("Trabajo"));
    }

    #[test]
    fn expands_recurrence_with_exdate_and_override() {
        let ev = window();
        let mut daily: Vec<(String, i64)> = ev
            .iter()
            .filter(|e| e.title.starts_with("Daily"))
            .map(|e| (e.title.clone(), e.start))
            .collect();
        daily.sort_by_key(|d| d.1);
        let at = |s: &str| utc(s).timestamp_millis();
        assert_eq!(
            daily,
            vec![
                ("Daily del equipo".into(), at("2025-11-17T16:30:00Z")),
                // 18th: EXDATE. 19th: moved to 12:00 local.
                ("Daily del equipo (movida)".into(), at("2025-11-19T18:00:00Z")),
                ("Daily del equipo".into(), at("2025-11-20T16:30:00Z")),
            ]
        );
        let first = ev.iter().find(|e| e.title == "Daily del equipo").unwrap();
        assert_eq!(first.end - first.start, 15 * 60_000);
        assert_eq!(first.link.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
    }

    #[test]
    fn all_day_duration_links_and_cancelled() {
        let ev = window();
        let all = ev.iter().find(|e| e.title == "Entrega v0.2").unwrap();
        assert!(all.all_day);
        assert_eq!(all.end - all.start, 24 * 3_600_000);
        let zoom = ev.iter().find(|e| e.title == "Revisión, diseño").unwrap();
        assert_eq!(zoom.end - zoom.start, 3_600_000);
        assert_eq!(zoom.link.as_deref(), Some("https://us02web.zoom.us/j/123456?pwd=abc"));
        assert!(!ev.iter().any(|e| e.title == "Cancelada"));
    }

    #[test]
    fn until_in_the_past_has_no_occurrences() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nDTSTART;TZID=Europe/Madrid:20240101T090000\nDURATION:PT30M\nRRULE:FREQ=DAILY;UNTIL=20240105\nSUMMARY:Vieja\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(expand(&parse(ics).events, utc("2025-11-17T00:00:00Z"), utc("2025-11-19T00:00:00Z")).is_empty());
    }

    #[test]
    fn urls() {
        assert_eq!(normalize_url(" webcal://p01-caldav.icloud.com/x.ics ").unwrap(), "https://p01-caldav.icloud.com/x.ics");
        assert!(normalize_url("http://x.com/a.ics").is_err());
        assert!(normalize_url("javascript:alert(1)").is_err());
    }

    /// A real export, kept out of the repo: ICS_FILE=path cargo test -p ibrae real_export -- --ignored --nocapture
    /// Prints only counts and times, never titles.
    #[test]
    #[ignore]
    fn real_export() {
        let text = std::fs::read_to_string(std::env::var("ICS_FILE").expect("ICS_FILE")).unwrap();
        let parsed = parse(&text);
        let today = Local::now().date_naive();
        for day in 0..3 {
            let d = today + CDur::days(day);
            let ev = expand(&parsed.events, local_midnight(d), local_midnight(d + CDur::days(1)));
            let times: Vec<String> = ev
                .iter()
                .filter(|e| !e.all_day)
                .map(|e| {
                    let s = Utc.timestamp_millis_opt(e.start).unwrap().with_timezone(&Local);
                    format!("{}({}m{})", s.format("%H:%M"), (e.end - e.start) / 60_000, if e.link.is_some() { ",link" } else { "" })
                })
                .collect();
            println!("{d}: {} eventos, {} todo el día: {}", ev.len(), ev.iter().filter(|e| e.all_day).count(), times.join(" "));
        }
    }

    /// Hits the network: cargo test -p ibrae live_google -- --ignored
    #[test]
    #[ignore]
    fn live_google_public_of_a_private_calendar() {
        let err = fetch("https://calendar.google.com/calendar/ical/zzq-no-existe-123%40gmail.com/public/basic.ics").unwrap_err();
        assert!(err.contains("Dirección secreta"), "{err}");
    }

    #[test]
    fn spots_ical_links_only() {
        let g = "https://calendar.google.com/calendar/ical/abc%40gmail.com/private-123/basic.ics";
        assert_eq!(ical_link(&format!("  {g}\n")).as_deref(), Some(g));
        assert!(ical_link("https://outlook.office365.com/owa/calendar/x/y/calendar.ics").is_some());
        assert_eq!(ical_link("webcal://p01-caldav.icloud.com/published/2/xyz").as_deref(), Some("https://p01-caldav.icloud.com/published/2/xyz"));
        assert!(ical_link("https://github.com/ibrarmz").is_none());
        assert!(ical_link("mi contraseña es 1234").is_none());
        assert!(ical_link("https://x.com/a.ics y más texto").is_none());
    }
}
