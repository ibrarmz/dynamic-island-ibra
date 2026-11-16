//! Linux: trims WebKitGTK's memory. The WebView2 flags in tauri.conf.json
//! don't apply here, so these are the WebKitGTK equivalents that are safe.
//!
//! Tried and rejected (they break the transparent window: the island renders
//! offset, clipped and without its shape, or not at all):
//! - WEBKIT_DISABLE_DMABUF_RENDERER=1 (would save ~60 MB)
//! - HardwareAccelerationPolicy::Never
//! - hiding EGL drivers to keep Mesa out of the main process (~40 MB)

use tauri::WebviewWindow;

/// Before any WebKit process starts. Never overrides what the user set.
pub fn tune_env() {
    // The UI runs a few tiny handlers; JIT code and its tiers cost more memory
    // than interpreting them (like V8's lite mode on Windows).
    if std::env::var_os("JSC_useJIT").is_none() {
        std::env::set_var("JSC_useJIT", "0");
    }
}

/// After the webview exists.
pub fn tune(win: &WebviewWindow) {
    let _ = win.with_webview(|wv| {
        use webkit2gtk::{CacheModel, SettingsExt, WebContextExt, WebViewExt};
        let view = wv.inner();
        // One local page, nothing to cache between navigations.
        if let Some(ctx) = view.context() {
            ctx.set_cache_model(CacheModel::DocumentViewer);
        }
        // Features the island never uses.
        if let Some(s) = WebViewExt::settings(&view) {
            s.set_enable_page_cache(false);
            s.set_enable_webgl(false);
            s.set_enable_webaudio(false);
            s.set_enable_media(false);
            s.set_enable_media_stream(false);
            s.set_enable_html5_database(false);
            s.set_enable_offline_web_application_cache(false);
        }
    });
}
