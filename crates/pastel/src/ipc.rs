//! The IPC socket: requests from bars and scripts, and the desktop's state for
//! subscribers. Protocol types live in `pastel-ipc`. Every socket is an `FdWatch`
//! on the loop, so a quiet socket costs nothing.

use std::io::{ErrorKind, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::str::FromStr;

use luminaria::core::{Display, FdWatch};
use pastel_config::Action;
use pastel_ipc::{Reply, Request, State as Snapshot};
use pastel_layout::{Desktop, WindowId, WorkspaceTarget};

use crate::state::State;
use crate::windows::surface_of;

/// The listening socket's tag; clients take the tags above it.
pub const LISTEN_TAG: u64 = 1 << 32;
/// A client that sends this much without a newline is cut off.
const MAX_LINE: usize = 64 * 1024;

pub fn owns(tag: u64) -> bool {
    tag >= LISTEN_TAG
}

struct Conn<'d> {
    // Declared first: the watch must stop before the stream closes its fd.
    _watch: FdWatch<'d>,
    tag: u64,
    stream: UnixStream,
    input: Vec<u8>,
    subscribed: bool,
}

pub struct Ipc<'d> {
    _watch: FdWatch<'d>,
    listener: UnixListener,
    display: &'d Display,
    path: PathBuf,
    clients: Vec<Conn<'d>>,
    next_tag: u64,
    /// The last state subscribers were sent, to send only changes.
    published: Option<Snapshot>,
}

impl Drop for Ipc<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl<'d> Ipc<'d> {
    /// Listens next to the Wayland socket, and tells children where.
    pub fn new(display: &'d Display, wayland_socket: &str) -> Option<Self> {
        let path = pastel_ipc::socket_for(wayland_socket)?;
        // A leftover from a crash; the Wayland socket name was free, so is this.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)
            .and_then(|l| l.set_nonblocking(true).map(|()| l))
            .inspect_err(|e| eprintln!("pastel: ipc socket {}: {e}", path.display()))
            .ok()?;
        let watch = FdWatch::new(display, listener.as_fd(), LISTEN_TAG).ok()?;
        // SAFETY: still single-threaded here; nothing reads the environment concurrently.
        unsafe { std::env::set_var(pastel_ipc::SOCKET_ENV, &path) };
        Some(Self {
            _watch: watch,
            listener,
            display,
            path,
            clients: Vec::new(),
            next_tag: LISTEN_TAG + 1,
            published: None,
        })
    }

    fn accept(&mut self) {
        while let Ok((stream, _)) = self.listener.accept() {
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            let tag = self.next_tag;
            let Ok(watch) = FdWatch::new(self.display, stream.as_fd(), tag) else {
                continue;
            };
            self.next_tag += 1;
            self.clients.push(Conn {
                _watch: watch,
                tag,
                stream,
                input: Vec::new(),
                subscribed: false,
            });
        }
    }

    /// Reads what arrived on `tag`: the complete requests, oldest first.
    fn readable(&mut self, tag: u64) -> Vec<Request> {
        if tag == LISTEN_TAG {
            self.accept();
            return Vec::new();
        }
        let Some(index) = self.clients.iter().position(|c| c.tag == tag) else {
            return Vec::new();
        };
        let conn = &mut self.clients[index];
        let mut buffer = [0u8; 4096];
        let mut closed = false;
        loop {
            match conn.stream.read(&mut buffer) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(n) => conn.input.extend_from_slice(&buffer[..n]),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => {
                    closed = true;
                    break;
                }
            }
        }
        let mut requests = Vec::new();
        while let Some(end) = conn.input.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = conn.input.drain(..=end).collect();
            let parsed = std::str::from_utf8(&line)
                .map_err(|e| e.to_string())
                .and_then(pastel_ipc::decode::<Request>);
            match parsed {
                Ok(request) => requests.push(request),
                Err(e) => send(conn, &Reply::Error(e)),
            }
        }
        if closed || conn.input.len() > MAX_LINE {
            self.clients.remove(index);
        }
        requests
    }

    fn reply(&mut self, tag: u64, reply: &Reply) {
        if let Some(conn) = self.clients.iter_mut().find(|c| c.tag == tag) {
            send(conn, reply);
        }
    }

    fn subscribe(&mut self, tag: u64, now: Snapshot) {
        if let Some(conn) = self.clients.iter_mut().find(|c| c.tag == tag) {
            conn.subscribed = true;
            send(conn, &Reply::State(now));
        }
    }

    fn has_subscribers(&self) -> bool {
        self.clients.iter().any(|c| c.subscribed)
    }

    /// Sends `now` to every subscriber if it differs from the last one sent.
    fn publish(&mut self, now: Snapshot) {
        if self.published.as_ref() == Some(&now) {
            return;
        }
        let line = pastel_ipc::encode(&Reply::State(now.clone()));
        self.clients
            .retain_mut(|conn| !conn.subscribed || write_line(conn, &line));
        self.published = Some(now);
    }
}

fn send(conn: &mut Conn<'_>, reply: &Reply) {
    write_line(conn, &pastel_ipc::encode(reply));
}

/// False if the client is gone or stopped reading. A local client drains a few
/// kilobytes long before the socket buffer fills, so a full one means stuck.
fn write_line(conn: &mut Conn<'_>, line: &str) -> bool {
    let _ = conn.stream.set_nonblocking(false);
    let _ = conn
        .stream
        .set_write_timeout(Some(std::time::Duration::from_millis(100)));
    let written = conn.stream.write_all(line.as_bytes()).is_ok();
    let _ = conn.stream.set_nonblocking(true);
    written
}

impl State<'_> {
    pub(crate) fn ipc_ready(&mut self, tag: u64) {
        let Some(ipc) = self.ipc.as_mut() else { return };
        for request in ipc.readable(tag) {
            match request {
                Request::Subscribe => {
                    let now = self.snapshot();
                    if let Some(ipc) = self.ipc.as_mut() {
                        ipc.subscribe(tag, now);
                    }
                }
                request => {
                    let reply = self.answer(request);
                    if let Some(ipc) = self.ipc.as_mut() {
                        ipc.reply(tag, &reply);
                    }
                }
            }
        }
    }

    fn answer(&mut self, request: Request) -> Reply {
        match request {
            Request::State | Request::Subscribe => return Reply::State(self.snapshot()),
            Request::Action(text) => match Action::from_str(&text) {
                Ok(action) => self.run(action),
                Err(e) => return Reply::Error(e),
            },
            Request::FocusWorkspace(id) => {
                if !focus_workspace(&mut self.desktop, id) {
                    return Reply::Error(format!("no workspace {id}"));
                }
                self.layer_focus = None;
            }
            Request::FocusWindow(id) => {
                if !self.desktop.focus_window(WindowId(id)) {
                    return Reply::Error(format!("no window {id}"));
                }
                self.layer_focus = None;
            }
            Request::CloseWindow(id) => {
                match self.services.shell().toplevel(surface_of(WindowId(id))) {
                    Some(toplevel) => toplevel.close(),
                    None => return Reply::Error(format!("no window {id}")),
                }
            }
        }
        self.dirty = true;
        Reply::Ok
    }

    /// Tells subscribers, if any, what changed. Called once a frame has settled.
    pub(crate) fn publish_ipc(&mut self) {
        if !self.ipc.as_ref().is_some_and(Ipc::has_subscribers) {
            return;
        }
        let now = self.snapshot();
        if let Some(ipc) = self.ipc.as_mut() {
            ipc.publish(now);
        }
    }

    /// The desktop as the protocol describes it.
    pub fn snapshot(&self) -> Snapshot {
        let mut state = Snapshot::default();
        let focused_monitor = self.desktop.focused_monitor().map(|m| m.id);
        let focused_window = self
            .desktop
            .focused_window()
            .filter(|_| self.layer_focus.is_none());
        let shell = self.services.shell();
        let mut placements = Vec::new();
        for monitor in self.desktop.monitors() {
            let name = self
                .outputs
                .iter()
                .find(|o| o.id().0 == monitor.id)
                .map(|o| self.services.screens.output_name(o))
                .unwrap_or_default();
            let r = monitor.rect;
            state.outputs.push(pastel_ipc::Output {
                name: name.clone(),
                focused: Some(monitor.id) == focused_monitor,
                rect: [r.x as i32, r.y as i32, r.w as i32, r.h as i32],
            });
            for (index, workspace) in monitor.workspaces.iter().enumerate() {
                let active = index == monitor.active;
                placements.clear();
                Desktop::place(monitor, workspace, &mut placements);
                let mut count = 0;
                for placement in &placements {
                    let id = placement.id;
                    if !self.windows.get(id).is_some_and(|w| w.mapped) {
                        continue;
                    }
                    let Some(toplevel) = shell.toplevel(surface_of(id)) else {
                        continue;
                    };
                    count += 1;
                    state.windows.push(pastel_ipc::Window {
                        id: id.0,
                        title: toplevel.title(),
                        app_id: toplevel.app_id(),
                        workspace: workspace.id,
                        focused: Some(id) == focused_window,
                        floating: placement.floating,
                        fullscreen: workspace.fullscreen == Some(id),
                    });
                }
                state.workspaces.push(pastel_ipc::Workspace {
                    id: workspace.id,
                    index: index as u32 + 1,
                    output: name.clone(),
                    active,
                    focused: active && Some(monitor.id) == focused_monitor,
                    windows: count,
                });
            }
        }
        state
    }
}

/// Shows workspace `id` on its monitor and focuses that monitor.
fn focus_workspace(desktop: &mut Desktop, id: u64) -> bool {
    let found = desktop.monitors().iter().find_map(|monitor| {
        let index = monitor.workspaces.iter().position(|w| w.id == id)?;
        Some((monitor.rect.center(), index))
    });
    let Some(((x, y), index)) = found else {
        return false;
    };
    desktop.focus_monitor_at(x, y);
    desktop.focus_workspace(WorkspaceTarget::Index(index));
    true
}
