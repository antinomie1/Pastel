//! Pastel's IPC protocol, for bars, shells and scripts.
//!
//! Pastel listens on a Unix socket, named in `PASTEL_SOCKET` for everything it
//! starts and found from `WAYLAND_DISPLAY` otherwise ([`socket_path`]). Each
//! message is one line of JSON. A client writes a [`Request`] and reads one
//! [`Reply`]. After [`Request::Subscribe`] the connection carries a
//! [`Reply::State`] at once and another each time anything in it changes; the
//! state is always whole, so a client never has to merge updates.
//!
//! [`Client`] is a small blocking client over the same types.

mod client;

pub use client::Client;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The environment variable Pastel sets for its children.
pub const SOCKET_ENV: &str = "PASTEL_SOCKET";

/// The socket of the Pastel instance serving `wayland_display`, under
/// `XDG_RUNTIME_DIR`.
pub fn socket_for(wayland_display: &str) -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let display = wayland_display.rsplit('/').next()?;
    Some(PathBuf::from(runtime).join(format!("pastel.{display}.sock")))
}

/// The socket to talk to: `PASTEL_SOCKET`, or the one of `WAYLAND_DISPLAY`.
pub fn socket_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(SOCKET_ENV) {
        return Some(path.into());
    }
    socket_for(&std::env::var("WAYLAND_DISPLAY").ok()?)
}

/// What a client asks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// The current [`State`].
    State,
    /// The state now and after every change, until the client disconnects.
    Subscribe,
    /// Runs a keybinding action, written as in the config:
    /// `"focus-workspace 2"`, `"spawn foot"`, `"close-window"`.
    Action(String),
    /// Shows a workspace on its monitor and focuses that monitor.
    FocusWorkspace(u64),
    /// Focuses a window, switching to its workspace.
    FocusWindow(u64),
    /// Asks a window to close.
    CloseWindow(u64),
}

/// What Pastel answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    /// Done.
    Ok,
    /// The whole desktop.
    State(State),
    /// The request was not understood or names nothing that exists.
    Error(String),
}

/// Everything a shell shows about the desktop.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Monitors, in the order Pastel arranges them.
    pub outputs: Vec<Output>,
    /// Workspaces of every monitor, by monitor, top to bottom.
    pub workspaces: Vec<Workspace>,
    /// Mapped windows, by workspace.
    pub windows: Vec<Window>,
}

/// A monitor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
    /// The connector name, such as `DP-1`: the `wl_output` name clients see.
    pub name: String,
    /// It has the keyboard.
    pub focused: bool,
    /// Logical position and size: x, y, width, height.
    pub rect: [i32; 4],
}

/// A workspace. Pastel keeps exactly one empty workspace at the bottom of each
/// monitor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    /// Stable for the workspace's life, unlike its index.
    pub id: u64,
    /// Position on its monitor, from 1.
    pub index: u32,
    /// The monitor it is on.
    pub output: String,
    /// Shown on its monitor.
    pub active: bool,
    /// Shown on the focused monitor.
    pub focused: bool,
    /// How many windows it holds.
    pub windows: u32,
}

/// A mapped toplevel window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// Stable for the window's life.
    pub id: u64,
    /// The xdg title.
    pub title: String,
    /// The xdg app id.
    pub app_id: String,
    /// The icon name the window set itself (xdg-toplevel-icon), empty without
    /// one: for windows whose app id names no desktop entry.
    #[serde(default)]
    pub icon: String,
    /// The workspace it is on.
    pub workspace: u64,
    /// It has the keyboard.
    pub focused: bool,
    /// Out of the tiling layout.
    pub floating: bool,
    /// Covers its monitor.
    pub fullscreen: bool,
}

/// One message as a line of JSON, newline included.
pub fn encode<T: Serialize>(message: &T) -> String {
    let mut line = serde_json::to_string(message).expect("protocol types always serialize");
    line.push('\n');
    line
}

/// Parses one line of JSON.
pub fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> Result<T, String> {
    serde_json::from_str(line.trim_end()).map_err(|e| e.to_string())
}
