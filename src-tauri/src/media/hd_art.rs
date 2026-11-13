//! Optional HD covers ("Carátulas en HD" in Ajustes, off by default).
//!
//! Browsers hand Windows tiny thumbnails (Chrome: ~150×83 for YouTube), so the
//! cover looks blurry however it is displayed. When enabled, this looks the song
//! up in Apple's public iTunes catalog (no key, no account) and uses the official
//! 600×600 artwork. Only the cleaned-up artist and title are sent. A result is
//! used only when both artist and title match, so a wrong cover never shows up;
//! otherwise the player's own thumbnail stays.

use super::{process_art, Art, Track};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};

/// Per run: normalized "artist|song" → cover (None = looked up, nothing usable).
fn cache() -> &'static Mutex<HashMap<String, Option<Art>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Art>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub fn find(track: &Track) -> Option<Art> {
    let (artist, song) = split(track);
    if norm(&artist).is_empty() || norm(&song).is_empty() {
        return None;
    }
    let key = format!("{}|{}", norm(&artist), norm(&song));
    if let Some(hit) = cache().lock().unwrap().get(&key) {
        return hit.clone();
    }
    // A network error isn't cached, so the next play of the song tries again.
    let found = lookup(&artist, &song).ok()?;
    let mut c = cache().lock().unwrap();
    if c.len() > 300 {
        c.clear();
    }
    c.insert(key, found.clone());
    found
}

/// Lowercase letters and digits only, for loose comparisons.
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Drop video decorations: "(Official Video)", "[Audio]", "| Lyrics", "ft. X".
fn strip_extras(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let lower = out.to_lowercase();
    let cut = [" ft.", " ft ", " feat.", " feat ", " featuring ", " | ", " // "]
        .iter()
        .filter_map(|m| lower.find(m))
        .min()
        .unwrap_or(out.len());
    out[..cut].trim().trim_matches(['"', '\'', '“', '”']).trim().to_string()
}

/// YouTube channel names: "CUCOVEVO", "Cuco - Topic".
fn clean_artist(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_suffix(" - Topic").unwrap_or(s);
    let s = s.strip_suffix("VEVO").unwrap_or(s);
    strip_extras(s)
}

/// (artist, song). Video titles usually carry "Artist - Song"; music apps send
/// them as separate fields.
fn split(t: &Track) -> (String, String) {
    match t.title.split_once(" - ") {
        Some((a, s)) => (clean_artist(a), strip_extras(s)),
        None => (clean_artist(&t.artist), strip_extras(&t.title)),
    }
}

fn loosely_equal(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.contains(b) || b.contains(a))
}

fn lookup(artist: &str, song: &str) -> Result<Option<Art>, ()> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(6)))
        .build()
        .new_agent();
    let body = agent
        .get("https://itunes.apple.com/search")
        .query("term", format!("{artist} {song}"))
        .query("media", "music")
        .query("entity", "song")
        .query("limit", "10")
        .call()
        .map_err(|_| ())?
        .body_mut()
        .read_to_string()
        .map_err(|_| ())?;
    let json: Value = serde_json::from_str(&body).map_err(|_| ())?;

    let (na, ns) = (norm(artist), norm(song));
    let hit = json["results"].as_array().into_iter().flatten().find(|r| {
        let a = norm(r["artistName"].as_str().unwrap_or(""));
        let s = norm(&strip_extras(r["trackName"].as_str().unwrap_or("")));
        loosely_equal(&a, &na) && loosely_equal(&s, &ns)
    });
    let Some(small) = hit.and_then(|r| r["artworkUrl100"].as_str()) else {
        return Ok(None);
    };
    // iTunes serves any size by rewriting the dimensions in the URL.
    let url = small.replace("100x100bb", "600x600bb");
    let bytes = agent
        .get(&url)
        .call()
        .map_err(|_| ())?
        .body_mut()
        .with_config()
        .limit(5_000_000)
        .read_to_vec()
        .map_err(|_| ())?;
    let art = process_art(&bytes);
    Ok(art.data.is_some().then_some(art))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, artist: &str) -> Track {
        Track { title: title.into(), artist: artist.into(), active: true, ..Default::default() }
    }

    #[test]
    fn splits_youtube_titles() {
        assert_eq!(split(&track("CUCO - Lover Is a Day (Audio)", "CUCO")), ("CUCO".into(), "Lover Is a Day".into()));
        assert_eq!(
            split(&track("Bad Bunny - DtMF [Official Video] ft. Someone", "BadBunnyVEVO")),
            ("Bad Bunny".into(), "DtMF".into())
        );
    }

    #[test]
    fn keeps_music_app_fields() {
        assert_eq!(split(&track("Lover Is a Day", "Cuco - Topic")), ("Cuco".into(), "Lover Is a Day".into()));
    }

    /// Hits the network: cargo test live_lookup -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_lookup() {
        let art = find(&track("CUCO - Lover Is a Day (Audio)", "CUCO")).expect("no HD cover found");
        let data = art.data.expect("no image");
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data.split(',').nth(1).unwrap()).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        println!("HD cover: {}x{}, {} KB", img.width(), img.height(), bytes.len() / 1024);
        assert!(img.width() >= 600);
        // YouTube auto-generated "Artist - Topic" channel, title without the artist.
        assert!(find(&track("Runaway", "Kanye West - Topic")).is_some(), "Runaway / Kanye West - Topic");
        // A made-up song must not get someone else's cover.
        assert!(find(&track("Zzqx Nonexistent - Qwerty Song 123", "")).is_none());
    }

    #[test]
    fn loose_match() {
        assert!(loosely_equal(&norm("Lover Is a Day"), &norm("lover is a day")));
        assert!(!loosely_equal(&norm(""), &norm("x")));
    }
}
