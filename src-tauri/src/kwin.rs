//! KDE Plasma on Wayland: the island runs through XWayland, so X11 can't see
//! native Wayland windows (Konsole, Dolphin, Firefox…) and "a maximized window
//! is in front" never fires for them. KWin sees every window, so we load a
//! small KWin script over D-Bus that watches the active window and calls back
//! into the island's own D-Bus service with `Covered(bool)`. The same script
//! pins the island to the top edge, over a top panel (KWin would otherwise
//! place it below the panel).

use std::sync::OnceLock;
use tauri::WebviewWindow;
use zbus::{blocking::Connection, interface};

const BUS_NAME: &str = "dev.dynamicisland.Island";
const OBJ_PATH: &str = "/Island";
const PLUGIN: &str = "dynamic-island-covered";

static CONN: OnceLock<Connection> = OnceLock::new();

struct Island {
    win: WebviewWindow,
}

#[interface(name = "dev.dynamicisland.Island")]
impl Island {
    /// Called by the KWin script when the active window starts / stops
    /// covering the island's monitor (maximized or fullscreen).
    fn covered(&self, covered: bool) {
        crate::hit::set_covered(&self.win, covered);
    }
}

/// Plasma Wayland session? (Checked before main.rs forces GDK onto X11, which
/// leaves WAYLAND_DISPLAY in place.)
pub fn wanted() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|s| s.eq_ignore_ascii_case("KDE")))
}

/// Serve `Covered` and load the KWin script. Returns false if anything fails,
/// so the caller can fall back to the X11 check.
pub fn start(win: WebviewWindow) -> bool {
    match try_start(win) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("kwin: {e}");
            false
        }
    }
}

fn try_start(win: WebviewWindow) -> zbus::Result<()> {
    let conn = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJ_PATH, Island { win })?
        .build()?;

    // The file must be readable by KWin on the host; XDG_RUNTIME_DIR is shared
    // with distrobox / toolbox containers too.
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("{PLUGIN}.js"));
    std::fs::write(&path, script(std::process::id())).map_err(|e| zbus::Error::Failure(e.to_string()))?;

    // A previous run (crash, kill) may have left its copy loaded.
    let _ = scripting(&conn, "unloadScript", &(PLUGIN,));
    let id: i32 = scripting(&conn, "loadScript", &(path.to_string_lossy().as_ref(), PLUGIN))?.body().deserialize()?;
    if id < 0 {
        return Err(zbus::Error::Failure("KWin no cargó el script".into()));
    }
    // KWin 6 path; KWin 5 used "/<id>".
    let run = |p: String| conn.call_method(Some("org.kde.KWin"), p.as_str(), Some("org.kde.kwin.Script"), "run", &());
    run(format!("/Scripting/Script{id}")).or_else(|_| run(format!("/{id}")))?;

    let _ = CONN.set(conn);
    Ok(())
}

/// Unload the script on exit so KWin doesn't keep calling a service that's gone.
pub fn stop() {
    if let Some(conn) = CONN.get() {
        let _ = scripting(conn, "unloadScript", &(PLUGIN,));
    }
}

fn scripting<B>(conn: &Connection, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    conn.call_method(Some("org.kde.KWin"), "/Scripting", Some("org.kde.kwin.Scripting"), method, body)
}

fn script(pid: u32) -> String {
    format!(
        r#"// Loaded by Dynamic Island. Tells it when the active window is maximized or
// fullscreen on the island's monitor.
const ISLAND_PID = {pid};
let last = null;
let watched = null;

// GTK/WebKit also map a hidden helper window (off screen at -100,-100) from
// the same process; only the normal window is the island.
function isIsland(w) {{
    return !!w && w.normalWindow
        && (w.pid === ISLAND_PID || String(w.resourceClass).toLowerCase() === "dynamic-island");
}}

function island() {{
    const all = workspace.stackingOrder;
    for (let i = 0; i < all.length; i++) if (isIsland(all[i])) return all[i];
    return null;
}}

function islandOutput() {{
    const w = island();
    return w ? w.output : null;
}}

// KWin keeps X11 windows out of panels when it places them, so the island
// ends up below a top panel. Pin it to the very top of its monitor, over
// the panel (it already stacks above docks: it's keep-above).
let pinned = null;
function pin() {{
    const w = island();
    if (!w) return;
    if (w !== pinned) {{
        pinned = w;
        w.frameGeometryChanged.connect(pin);
        w.outputChanged.connect(pin);
    }}
    const top = w.output.geometry.y;
    const g = w.frameGeometry;
    if (g.y !== top) w.frameGeometry = {{ x: g.x, y: top, width: g.width, height: g.height }};
}}

function covers(w) {{
    if (!w || w.desktopWindow || w.dock || w.minimized || !w.normalWindow) return false;
    const out = islandOutput();
    if (out && w.output !== out) return false;
    if (w.fullScreen) return true;
    const a = workspace.clientArea(KWin.MaximizeArea, w);
    const g = w.frameGeometry;
    return g.x <= a.x + 1 && g.y <= a.y + 1
        && g.x + g.width >= a.x + a.width - 1 && g.y + g.height >= a.y + a.height - 1;
}}

function update() {{
    const w = workspace.activeWindow;
    // Clicking the island itself doesn't change what's behind it.
    if (isIsland(w)) return;
    const v = covers(w);
    if (v !== last) {{
        last = v;
        callDBus("{BUS_NAME}", "{OBJ_PATH}", "dev.dynamicisland.Island", "Covered", v);
    }}
}}

function watch(w) {{
    if (watched) {{
        watched.frameGeometryChanged.disconnect(update);
        watched.fullScreenChanged.disconnect(update);
        watched.minimizedChanged.disconnect(update);
        watched.outputChanged.disconnect(update);
    }}
    watched = isIsland(w) ? null : w;
    if (watched) {{
        watched.frameGeometryChanged.connect(update);
        watched.fullScreenChanged.connect(update);
        watched.minimizedChanged.connect(update);
        watched.outputChanged.connect(update);
    }}
}}

workspace.windowAdded.connect(function (w) {{ if (isIsland(w)) pin(); }});
workspace.windowActivated.connect(function (w) {{ watch(w); update(); }});
workspace.windowRemoved.connect(function (w) {{ if (w === watched) watched = null; update(); }});
pin();
watch(workspace.activeWindow);
update();
"#
    )
}
