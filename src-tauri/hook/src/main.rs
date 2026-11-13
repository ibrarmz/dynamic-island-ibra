//! Tiny hook client for Claude Code: `island-hook` reads one hook event from
//! stdin and forwards it to the running island. Its own crate (std + serde_json
//! only) so it builds in seconds and starts fast; the code is shared with the
//! app's `dynamic-island hook` fallback.

#[path = "../../src/claude/hookclient.rs"]
mod hookclient;

fn main() {
    hookclient::run();
}
