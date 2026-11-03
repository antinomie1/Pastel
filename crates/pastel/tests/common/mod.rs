//! A real Wayland client on its own thread, for driving the compositor end to end.
//! Windows are solid-colour shm surfaces that resize to whatever they're configured.
#![allow(dead_code)]

mod activation;
mod capture;
mod devices;
mod foreign;
mod lock;
mod outputs;
mod wallpaper;

use std::collections::HashMap;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::net::Shutdown;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_region, wl_registry, wl_seat,
    wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface, wl_touch,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop, event_created_child,
};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1;

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

pub use capture::Captured;
pub use devices::Devices;
pub use wallpaper::Pattern;
use wallpaper::Wallpaper;

pub enum Command {
    Open {
        app_id: String,
        argb: u32,
    },
    /// A window with limits on its size, or transient for another.
    OpenWith {
        app_id: String,
        argb: u32,
        fixed: Option<(i32, i32)>,
        parent: Option<usize>,
    },
    /// Its title bar was grabbed.
    Move(usize),
    /// Its edge was grabbed.
    Resize(usize, xdg_toplevel::ResizeEdge),
    Close(usize),
    /// New pixels in the same `wl_buffer`, as a terminal reusing its shm does.
    Repaint(usize, u32),
    /// Quit any wallpaper client and start a new one with this picture.
    Wallpaper(Pattern),
    /// Quit the wallpaper client.
    NoWallpaper,
    /// Ask something of the only output, as wlr-randr does.
    Randr(outputs::Ask),
    /// Listen for the fractional scale this window is meant to draw at.
    WatchScale(usize),
    /// Copy the first output, as grim does.
    Capture,
    /// Ask to hear when the seat has been idle this long, in milliseconds.
    WatchIdle(u32),
    /// Keep the session awake from a subsurface of this window, as a video does.
    InhibitIdleInSubsurface(usize),
    /// Follow the window list, as a taskbar does.
    WatchWindows,
    /// Ask from the taskbar to focus the nth window it heard about.
    TaskbarActivate(usize),
    /// Ask from the taskbar to close the nth window it heard about.
    TaskbarClose(usize),
    /// Ask for the session lock, with a surface on every output.
    Lock,
    Unlock,
    /// Window `.0` asks for a token, and uses it to activate window `.1`.
    Activate(usize, usize),
    /// Lock the pointer to this window, and listen for relative motion.
    LockPointer(usize),
    /// Keep the pointer inside this window.
    ConfinePointer(usize),
    /// Bind the tablet seat, as a drawing program does.
    WatchTablets,
    /// Ask for the gamma control of the output, as wlsunset does.
    AskGamma,
    /// Type a key through a virtual keyboard with this keymap, as wtype does.
    VirtualType(String, u32),
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
    /// The interface of every global the compositor offers.
    pub globals: Vec<String>,
    /// Every size a wallpaper surface has been configured to.
    pub wallpapers: Vec<(i32, i32)>,
    /// `locked` arrived, and no unlock since.
    pub locked: bool,
    /// Every size a lock surface has been configured to.
    pub lock_sizes: Vec<(u32, u32)>,
    pub preferred_scale: Option<u32>,
    /// `idled` and `resumed` as they came: true for idled.
    pub idle: Vec<bool>,
    /// The app id of each window the taskbar was told about.
    pub taskbar: Vec<String>,
    pub captured: Option<Captured>,
    pub capture_failed: bool,
    pub randr: Vec<bool>,
    /// What wl_touch said: the point down, and each `up`.
    pub touches: Vec<(f64, f64)>,
    pub touch_ups: usize,
    /// The name of every head announced.
    pub heads: Vec<String>,
    pub devices: Devices,
}

pub struct Client {
    commands: Sender<Command>,
    pub report: Arc<Mutex<Report>>,
    socket: UnixStream,
    thread: Option<JoinHandle<()>>,
}

impl Client {
    pub fn connect(socket: &str) -> Self {
        let dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
        let stream = UnixStream::connect(format!("{dir}/{socket}")).expect("connect");
        let report = Arc::new(Mutex::new(Report::default()));
        let (commands, inbox) = channel();
        let shared = report.clone();
        let socket = stream.try_clone().expect("clone socket");
        let thread = std::thread::spawn(move || run(stream, inbox, shared));
        Self {
            commands,
            report,
            socket,
            thread: Some(thread),
        }
    }

    pub fn open(&self, app_id: &str, argb: u32) {
        let command = Command::Open {
            app_id: app_id.into(),
            argb,
        };
        self.commands.send(command).expect("client alive");
    }

    /// A window that can only be this size, as a settings dialog might be.
    pub fn open_fixed(&self, app_id: &str, argb: u32, size: (i32, i32)) {
        let command = Command::OpenWith {
            app_id: app_id.into(),
            argb,
            fixed: Some(size),
            parent: None,
        };
        self.commands.send(command).expect("client alive");
    }

    pub fn open_dialog(&self, app_id: &str, argb: u32, parent: usize) {
        let command = Command::OpenWith {
            app_id: app_id.into(),
            argb,
            fixed: None,
            parent: Some(parent),
        };
        self.commands.send(command).expect("client alive");
    }

    pub fn request_move(&self, index: usize) {
        self.commands
            .send(Command::Move(index))
            .expect("client alive");
    }

    pub fn request_resize_right(&self, index: usize) {
        let edge = xdg_toplevel::ResizeEdge::Right;
        self.commands
            .send(Command::Resize(index, edge))
            .expect("client alive");
    }

    pub fn close(&self, index: usize) {
        self.commands
            .send(Command::Close(index))
            .expect("client alive");
    }

    pub fn repaint(&self, index: usize, argb: u32) {
        self.commands
            .send(Command::Repaint(index, argb))
            .expect("client alive");
    }

    /// What `swaybg -i ...` does, as a kill and a restart is how it changes.
    pub fn wallpaper(&self, pattern: Pattern) {
        self.commands
            .send(Command::Wallpaper(pattern))
            .expect("client alive");
    }

    pub fn no_wallpaper(&self) {
        self.commands
            .send(Command::NoWallpaper)
            .expect("client alive");
    }

    pub fn randr(&self, scale: f64, test: bool) {
        self.commands
            .send(Command::Randr(outputs::Ask::Scale(scale, test)))
            .expect("client alive");
    }

    pub fn randr_off(&self) {
        self.commands
            .send(Command::Randr(outputs::Ask::Off))
            .expect("client alive");
    }

    pub fn watch_scale(&self, index: usize) {
        self.commands
            .send(Command::WatchScale(index))
            .expect("client alive");
    }

    /// The scale each window was last told, in 120ths.
    pub fn preferred_scale(&self) -> Option<u32> {
        self.report.lock().expect("report").preferred_scale
    }

    /// Every verdict so far: true for `succeeded`.
    pub fn randr_answers(&self) -> Vec<bool> {
        self.report.lock().expect("report").randr.clone()
    }

    pub fn heads(&self) -> Vec<String> {
        self.report.lock().expect("report").heads.clone()
    }

    pub fn touches(&self) -> (Vec<(f64, f64)>, usize) {
        let report = self.report.lock().expect("report");
        (report.touches.clone(), report.touch_ups)
    }

    pub fn capture(&self) {
        self.commands.send(Command::Capture).expect("client alive");
    }

    pub fn captured(&self) -> Option<Captured> {
        let report = self.report.lock().expect("report");
        assert!(!report.capture_failed, "the capture failed");
        report.captured.clone()
    }

    pub fn watch_idle(&self, ms: u32) {
        self.commands
            .send(Command::WatchIdle(ms))
            .expect("client alive");
    }

    pub fn inhibit_idle_in_subsurface(&self, index: usize) {
        self.commands
            .send(Command::InhibitIdleInSubsurface(index))
            .expect("client alive");
    }

    pub fn idle(&self) -> Vec<bool> {
        self.report.lock().expect("report").idle.clone()
    }

    pub fn watch_windows(&self) {
        self.commands
            .send(Command::WatchWindows)
            .expect("client alive");
    }

    pub fn taskbar_activate(&self, index: usize) {
        self.commands
            .send(Command::TaskbarActivate(index))
            .expect("client alive");
    }

    pub fn taskbar_close(&self, index: usize) {
        self.commands
            .send(Command::TaskbarClose(index))
            .expect("client alive");
    }

    pub fn taskbar(&self) -> Vec<String> {
        self.report.lock().expect("report").taskbar.clone()
    }

    /// What swaylock does.
    pub fn lock(&self) {
        self.commands.send(Command::Lock).expect("client alive");
    }

    pub fn unlock(&self) {
        self.commands.send(Command::Unlock).expect("client alive");
    }

    pub fn locked(&self) -> bool {
        self.report.lock().expect("report").locked
    }

    pub fn lock_sizes(&self) -> Vec<(u32, u32)> {
        self.report.lock().expect("report").lock_sizes.clone()
    }

    /// What a launcher does: ask for a token from `from`, then hand it to `target`.
    pub fn activate(&self, from: usize, target: usize) {
        self.commands
            .send(Command::Activate(from, target))
            .expect("client alive");
    }

    pub fn wallpaper_sizes(&self) -> Vec<(i32, i32)> {
        self.report.lock().expect("report").wallpapers.clone()
    }

    pub fn seen(&self, index: usize) -> Seen {
        self.report
            .lock()
            .expect("report")
            .windows
            .get(index)
            .cloned()
            .unwrap_or_default()
    }

    pub fn send(&self, command: Command) {
        self.commands.send(command).expect("client alive");
    }

    pub fn devices(&self) -> Devices {
        self.report.lock().expect("report").devices.clone()
    }

    pub fn keys(&self) -> Vec<u32> {
        self.report.lock().expect("report").keys.clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        // The thread may still be blocked on a compositor that has stopped
        // dispatching; without this, joining it would wait for ever.
        let _ = self.socket.shutdown(Shutdown::Both);
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
    drawn: Option<(File, wl_buffer::WlBuffer, (i32, i32))>,
}

struct App {
    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    wm: xdg_wm_base::XdgWmBase,
    windows: HashMap<usize, Window>,
    wallpapers: Vec<Wallpaper>,
    session_lock: Option<lock::Locker>,
    taskbar: Vec<foreign::Handle>,
    capturing: capture::Capturing,
    randr: outputs::Randr,
    seat: wl_seat::WlSeat,
    globals: GlobalList,
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
        self.open_with(app_id, argb, None, None);
    }

    fn open_with(
        &mut self,
        app_id: &str,
        argb: u32,
        fixed: Option<(i32, i32)>,
        parent: Option<usize>,
    ) {
        let index = self.next;
        self.next += 1;
        let surface = self.compositor.create_surface(&self.qh, ());
        let xdg = self.wm.get_xdg_surface(&surface, &self.qh, index);
        let toplevel = xdg.get_toplevel(&self.qh, index);
        toplevel.set_app_id(app_id.into());
        if let Some((w, h)) = fixed {
            toplevel.set_min_size(w, h);
            toplevel.set_max_size(w, h);
        }
        if let Some(parent) = parent.and_then(|p| self.windows.get(&p)) {
            toplevel.set_parent(Some(&parent.toplevel));
        }
        surface.commit();
        self.seen(index, |_| {});
        let window = Window {
            surface,
            xdg,
            toplevel,
            argb,
            pending: (0, 0),
            states: (false, false),
            drawn: None,
        };
        self.windows.insert(index, window);
    }

    /// A file of `bytes` for shm: unique across the process, as tests run in
    /// parallel and two that shared a name would draw into each other's windows.
    fn shm_file(bytes: &[u8]) -> File {
        let dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
        let unique = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = format!("{dir}/pastel-test-{}-{unique}", std::process::id());
        let mut file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .expect("shm");
        let _ = std::fs::remove_file(&path);
        file.write_all(bytes).expect("write shm");
        file
    }

    fn watch_scale(&mut self, index: usize) {
        let qh = self.qh.clone();
        let Some(window) = self.windows.get(&index) else {
            return;
        };
        let manager: wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("fractional scale");
        let _ = manager.get_fractional_scale(&window.surface, &qh, ());
    }

    fn inhibit_idle_in_subsurface(&mut self, index: usize) {
        let qh = self.qh.clone();
        let Some(window) = self.windows.get(&index) else {
            return;
        };
        let child = self.compositor.create_surface(&qh, ());
        let subcompositor: wl_subcompositor::WlSubcompositor =
            self.globals.bind(&qh, 1..=1, ()).expect("subcompositor");
        let _ = subcompositor.get_subsurface(&child, &window.surface, &qh, ());
        let manager: zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("idle inhibit");
        let _ = manager.create_inhibitor(&child, &qh, ());
        child.commit();
        window.surface.commit();
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
        let qh = self.qh.clone();
        let Some(window) = self.windows.get_mut(&index) else {
            return;
        };
        let (w, h) = window.pending;
        let (w, h) = (if w > 0 { w } else { 300 }, if h > 0 { h } else { 200 });
        let pixel = window.argb.to_le_bytes();
        let bytes: Vec<u8> = pixel
            .iter()
            .copied()
            .cycle()
            .take((w * h * 4) as usize)
            .collect();
        let file = Self::shm_file(&bytes);
        let pool = self.shm.create_pool(file.as_fd(), w * h * 4, &qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, &qh, ());
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, w, h);
        window.surface.commit();
        pool.destroy();
        window.drawn = Some((file, buffer, (w, h)));
    }

    fn repaint(&mut self, index: usize, argb: u32) {
        let Some(window) = self.windows.get_mut(&index) else {
            return;
        };
        let Some((file, buffer, (w, h))) = &mut window.drawn else {
            return;
        };
        let (w, h) = (*w, *h);
        let bytes: Vec<u8> = argb
            .to_le_bytes()
            .iter()
            .copied()
            .cycle()
            .take((w * h * 4) as usize)
            .collect();
        file.seek(SeekFrom::Start(0)).expect("seek shm");
        file.write_all(&bytes).expect("write shm");
        window.surface.attach(Some(&*buffer), 0, 0);
        window.surface.damage_buffer(0, 0, w, h);
        window.surface.commit();
    }
}

fn run(stream: UnixStream, commands: Receiver<Command>, report: Arc<Mutex<Report>>) {
    stream.set_nonblocking(true).expect("nonblocking");
    let connection = Connection::from_socket(stream).expect("connection");
    let Ok((globals, mut queue)) = registry_queue_init::<App>(&connection) else {
        return; // hung up on before it got started
    };
    let qh = queue.handle();
    let offered = globals.contents().clone_list();
    report.lock().expect("report").globals = offered.into_iter().map(|g| g.interface).collect();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=9, ()).expect("wl_seat");
    let _keyboard = seat.get_keyboard(&qh, ());
    let _touch = seat.get_touch(&qh, ());
    let mut app = App {
        compositor: globals.bind(&qh, 1..=6, ()).expect("wl_compositor"),
        shm: globals.bind(&qh, 1..=1, ()).expect("wl_shm"),
        wm: globals.bind(&qh, 1..=6, ()).expect("xdg_wm_base"),
        windows: HashMap::new(),
        wallpapers: Vec::new(),
        session_lock: None,
        taskbar: Vec::new(),
        capturing: capture::Capturing::default(),
        randr: outputs::Randr::default(),
        seat,
        globals,
        next: 0,
        report,
        qh,
    };
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Open { app_id, argb } => app.open(&app_id, argb),
                Command::OpenWith {
                    app_id,
                    argb,
                    fixed,
                    parent,
                } => app.open_with(&app_id, argb, fixed, parent),
                Command::Move(index) => {
                    if let Some(window) = app.windows.get(&index) {
                        window.toplevel._move(&app.seat, 0);
                    }
                }
                Command::Resize(index, edge) => {
                    if let Some(window) = app.windows.get(&index) {
                        window.toplevel.resize(&app.seat, 0, edge);
                    }
                }
                Command::Close(index) => app.close(index),
                Command::Repaint(index, argb) => app.repaint(index, argb),
                Command::Wallpaper(pattern) => app.start_wallpaper(pattern),
                Command::NoWallpaper => app.stop_wallpaper(),
                Command::Activate(from, target) => app.activate(from, target),
                Command::Randr(ask) => app.randr(ask),
                Command::Capture => app.capture(),
                Command::WatchIdle(ms) => app.watch_idle(ms),
                Command::WatchScale(index) => app.watch_scale(index),
                Command::InhibitIdleInSubsurface(index) => app.inhibit_idle_in_subsurface(index),
                Command::WatchWindows => app.watch_windows(),
                Command::TaskbarActivate(index) => app.taskbar_activate(index),
                Command::TaskbarClose(index) => app.taskbar_close(index),
                Command::LockPointer(index) => app.lock_pointer(index),
                Command::ConfinePointer(index) => app.confine_pointer(index),
                Command::WatchTablets => app.watch_tablets(),
                Command::AskGamma => app.ask_gamma(),
                Command::VirtualType(keymap, key) => app.virtual_type(&keymap, key),
                Command::Lock => app.lock(),
                Command::Unlock => app.unlock(),
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
            let Some(window) = app.windows.get(index) else {
                return;
            };
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
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                let has = |state: xdg_toplevel::State| {
                    let bytes = (state as u32).to_ne_bytes();
                    states.as_chunks::<4>().0.contains(&bytes)
                };
                let flags = (
                    has(xdg_toplevel::State::Activated),
                    has(xdg_toplevel::State::Fullscreen),
                );
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
        if let wl_keyboard::Event::Keymap { .. } = event {
            app.keymap_arrived();
        }
        if let wl_keyboard::Event::Key {
            key,
            state: WEnum::Value(state),
            ..
        } = event
            && state == wl_keyboard::KeyState::Pressed
        {
            app.report.lock().expect("report").keys.push(key);
        }
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            app.report.lock().expect("report").preferred_scale = Some(scale);
        }
    }
}

impl Dispatch<wl_touch::WlTouch, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut report = app.report.lock().expect("report");
        match event {
            wl_touch::Event::Down { x, y, .. } => report.touches.push((x, y)),
            wl_touch::Event::Motion { x, y, .. } => report.touches.push((x, y)),
            wl_touch::Event::Up { .. } => report.touch_ups += 1,
            _ => {}
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
delegate_noop!(App: ignore wl_region::WlRegion);
delegate_noop!(App: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);
delegate_noop!(App: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(App: ignore wl_subsurface::WlSubsurface);
delegate_noop!(App: ignore zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1);
delegate_noop!(App: ignore zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1);
delegate_noop!(App: ignore wl_output::WlOutput);
delegate_noop!(App: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
