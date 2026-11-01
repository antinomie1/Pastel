//! A real Wayland client on its own thread, for driving the compositor end to end.
//! Windows are solid-colour shm surfaces that resize to whatever they're configured.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_noop, event_created_child};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

pub enum Command {
    Open { app_id: String, argb: u32 },
    Close(usize),
    Quit,
}

/// What a window has been told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seen {
    pub size: (i32, i32),
    pub configures: usize,
    pub activated: bool,
    pub fullscreen: bool,
    pub close_requests: usize,
}

/// What every window has been told, shared with the test thread.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub windows: Vec<Seen>,
    pub keys: Vec<u32>,
}

pub struct Client {
    commands: Sender<Command>,
    pub report: Arc<Mutex<Report>>,
    thread: Option<JoinHandle<()>>,
}

impl Client {
    pub fn connect(socket: &str) -> Self {
        let dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
        let stream = UnixStream::connect(format!("{dir}/{socket}")).expect("connect");
        let report = Arc::new(Mutex::new(Report::default()));
        let (commands, inbox) = channel();
        let shared = report.clone();
        let thread = std::thread::spawn(move || run(stream, inbox, shared));
        Self { commands, report, thread: Some(thread) }
    }

    pub fn open(&self, app_id: &str, argb: u32) {
        let command = Command::Open { app_id: app_id.into(), argb };
        self.commands.send(command).expect("client alive");
    }

    pub fn close(&self, index: usize) {
        self.commands.send(Command::Close(index)).expect("client alive");
    }

    pub fn seen(&self, index: usize) -> Seen {
        self.report.lock().expect("report").windows.get(index).cloned().unwrap_or_default()
    }

    pub fn keys(&self) -> Vec<u32> {
        self.report.lock().expect("report").keys.clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Window {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    argb: u32,
    pending: (i32, i32),
    states: (bool, bool),
}

struct App {
    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    wm: xdg_wm_base::XdgWmBase,
    windows: HashMap<usize, Window>,
    next: usize,
    report: Arc<Mutex<Report>>,
    qh: QueueHandle<App>,
}

impl App {
    fn seen(&self, index: usize, update: impl FnOnce(&mut Seen)) {
        let mut report = self.report.lock().expect("report");
        if report.windows.len() <= index {
            report.windows.resize(index + 1, Seen::default());
        }
        update(&mut report.windows[index]);
    }

    fn open(&mut self, app_id: &str, argb: u32) {
        let index = self.next;
        self.next += 1;
        let surface = self.compositor.create_surface(&self.qh, ());
        let xdg = self.wm.get_xdg_surface(&surface, &self.qh, index);
        let toplevel = xdg.get_toplevel(&self.qh, index);
        toplevel.set_app_id(app_id.into());
        surface.commit();
        self.seen(index, |_| {});
        let window = Window { surface, xdg, toplevel, argb, pending: (0, 0), states: (false, false) };
        self.windows.insert(index, window);
    }

    fn close(&mut self, index: usize) {
        if let Some(window) = self.windows.remove(&index) {
            window.toplevel.destroy();
            window.xdg.destroy();
            window.surface.destroy();
        }
    }

    /// Draw one frame at the size just configured. Unfocused windows are dimmed
    /// so a screenshot shows the focus without any decorations to go by.
    fn draw(&mut self, index: usize) {
        let Some(window) = self.windows.get(&index) else { return };
        let (w, h) = window.pending;
        let (w, h) = (if w > 0 { w } else { 300 }, if h > 0 { h } else { 200 });
        let dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
        let path = format!("{dir}/pastel-test-{}-{index}-{w}x{h}", std::process::id());
        let mut file =
            File::options().create(true).read(true).write(true).truncate(true).open(&path).expect("shm");
        let _ = std::fs::remove_file(&path);
        let pixel = window.argb.to_le_bytes();
        let bytes: Vec<u8> = pixel.iter().copied().cycle().take((w * h * 4) as usize).collect();
        file.write_all(&bytes).expect("write shm");
        let pool = self.shm.create_pool(file.as_fd(), w * h * 4, &self.qh, ());
        let buffer =
            pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, &self.qh, ());
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, w, h);
        window.surface.commit();
        pool.destroy();
    }
}

fn run(stream: UnixStream, commands: Receiver<Command>, report: Arc<Mutex<Report>>) {
    stream.set_nonblocking(true).expect("nonblocking");
    let connection = Connection::from_socket(stream).expect("connection");
    let (globals, mut queue) = registry_queue_init::<App>(&connection).expect("registry");
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=9, ()).expect("wl_seat");
    let _keyboard = seat.get_keyboard(&qh, ());
    let mut app = App {
        compositor: globals.bind(&qh, 1..=6, ()).expect("wl_compositor"),
        shm: globals.bind(&qh, 1..=1, ()).expect("wl_shm"),
        wm: globals.bind(&qh, 1..=6, ()).expect("xdg_wm_base"),
        windows: HashMap::new(),
        next: 0,
        report,
        qh,
    };
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Open { app_id, argb } => app.open(&app_id, argb),
                Command::Close(index) => app.close(index),
                Command::Quit => return,
            }
        }
        queue.dispatch_pending(&mut app).expect("dispatch");
        if queue.flush().is_err() {
            return;
        }
        if let Some(guard) = queue.prepare_read() {
            let _ = guard.read();
        }
        queue.dispatch_pending(&mut app).expect("dispatch");
        std::thread::sleep(Duration::from_millis(1));
    }
}

impl Dispatch<xdg_surface::XdgSurface, usize> for App {
    fn event(
        app: &mut Self,
        xdg: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg.ack_configure(serial);
            let Some(window) = app.windows.get(index) else { return };
            let (size, (activated, fullscreen)) = (window.pending, window.states);
            app.seen(*index, |seen| {
                seen.size = size;
                seen.configures += 1;
                seen.activated = activated;
                seen.fullscreen = fullscreen;
            });
            app.draw(*index);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, usize> for App {
    fn event(
        app: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, states } => {
                let has = |state: xdg_toplevel::State| {
                    let bytes = (state as u32).to_ne_bytes();
                    states.chunks_exact(4).any(|chunk| chunk == bytes)
                };
                let flags =
                    (has(xdg_toplevel::State::Activated), has(xdg_toplevel::State::Fullscreen));
                if let Some(window) = app.windows.get_mut(index) {
                    window.pending = (width, height);
                    window.states = flags;
                }
            }
            xdg_toplevel::Event::Close => app.seen(*index, |seen| seen.close_requests += 1),
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for App {
    fn event(
        _: &mut Self,
        wm: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key { key, state: WEnum::Value(state), .. } = event
            && state == wl_keyboard::KeyState::Pressed
        {
            app.report.lock().expect("report").keys.push(key);
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(App, wl_callback::WlCallback, []);
}

delegate_noop!(App: ignore wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_surface::WlSurface);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_buffer::WlBuffer);
delegate_noop!(App: ignore wl_seat::WlSeat);
