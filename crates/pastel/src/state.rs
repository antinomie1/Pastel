//! The compositor's state and the loop's two halves: [`State::handle`] takes in what
//! happened, [`State::frame`] brings clients and the screen up to date.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use luminaria::Event;
use luminaria::backend::{AutoBackend, Output, OutputEvent, Session};
use luminaria::core::Display;
use luminaria::desktop::{
    ForeignToplevelRequest, ForeignToplevelRequestKind, OutputManager, SessionLockManager,
};
use luminaria::gpu::{Cursor, SceneRenderer};
use luminaria::protocol::{
    ActivationEvent, CompositorEvent, CoreGlobals, IdleInhibitEvent, IdleInhibitManager,
    IdleInhibitorId, IdleNotifier, KeyboardInteractivity, LayerShell, LayerSurfaceEvent,
    PointerGestures, SurfaceId, Toplevel, ToplevelEvent, XdgShell,
};
use luminaria::shell::{
    InputRouter, LayerManager, OutputLayout, PopupTree, SceneHit, SceneItem, SceneKind,
    ScreenManager, scene_hit_test,
};
use luminaria::util::{KeymapState, Rect as PixelRect};
use pastel_config::Config;
use pastel_core::{FrameClock, Rect};
use pastel_layout::{Desktop, Placement, WindowHint};
use pastel_render::{Composer, HotkeyOverlay, Overlay, OverlayContext, color};

use crate::grab::Held;
use crate::input::Keyboard;
use crate::lock::LockState;
use crate::touch::TouchState;
use crate::watch::{CONFIG_TAG, ConfigWatch};
use crate::windows::{SceneHost, Windows, id_of, surface_of};
use crate::xwayland::{EXIT_TAG, SOCKET_TAG, Xwayland};

/// How long to wait for a `Present` before drawing the next animation frame
/// anyway. Animation is paced by the display; this only covers one being off.
const PRESENT_TIMEOUT: Duration = Duration::from_millis(32);

/// The parts of luminaria that everything else goes through.
pub struct Services<'a> {
    pub globals: &'a CoreGlobals<'a>,
    pub layer_shell: &'a LayerShell<'a>,
    pub backend: &'a AutoBackend<'a>,
    pub display: &'a Display,
    pub locks: &'a SessionLockManager<'a>,
    pub output_manager: &'a OutputManager<'a>,
    pub idle: &'a IdleNotifier<'a>,
    pub gestures: &'a PointerGestures<'a>,
    pub inhibitors: &'a IdleInhibitManager<'a>,
    pub layout: &'a OutputLayout,
    pub popups: &'a PopupTree,
    pub renderer: &'a SceneRenderer,
    pub screens: &'a ScreenManager<'a>,
    pub router: &'a InputRouter<'a>,
    pub cursor: &'a Cursor<'a>,
    pub session: Option<&'a Session<'a>>,
}

impl<'a> Services<'a> {
    pub fn shell(&self) -> &'a XdgShell<'a> {
        self.globals.shell()
    }
}

pub struct State<'a> {
    pub services: Services<'a>,
    pub(crate) config: Config,
    pub(crate) config_watch: Option<ConfigWatch<'a>>,
    xwayland: Option<Xwayland<'a>>,
    pub(crate) lock: LockState,
    pub(crate) held: Option<Held>,
    pub(crate) touching: TouchState,
    frame_clock: FrameClock,
    inhibitors: Vec<IdleInhibitorId>,
    config_path: Option<PathBuf>,
    socket: String,
    pub(crate) nested: bool,

    pub desktop: Desktop,
    pub windows: Windows,
    pub(crate) layers: LayerManager,
    /// Held for their `Present` events; ScreenManager owns everything else.
    pub(crate) outputs: Vec<Output>,

    pub(crate) composer: Composer,
    pub hotkeys: HotkeyOverlay,
    pub scene: Vec<SceneItem>,
    pub(crate) keyboard: Keyboard,
    placements: Vec<Placement>,

    /// Surfaces that asked for an activation token while holding the keyboard.
    activators: Vec<SurfaceId>,
    /// The layer surface with the keyboard, over any window.
    layer_focus: Option<SurfaceId>,
    focus: Option<SurfaceId>,

    epoch: Instant,
    /// Something changed and the next frame must show it.
    pub dirty: bool,
    animating: bool,
    presented: bool,
    last_frame: Instant,
    pub quit: bool,
}

pub(crate) fn logical(rect: PixelRect) -> Rect {
    let f = f64::from;
    Rect::new(f(rect.x), f(rect.y), f(rect.width), f(rect.height))
}

impl<'a> State<'a> {
    pub fn new(
        services: Services<'a>,
        config: Config,
        config_path: Option<PathBuf>,
        socket: String,
        nested: bool,
    ) -> Result<Self, String> {
        let desktop = Desktop::new(config.layout.clone()).ok_or("unknown layout")?;
        let layers = LayerManager::new().map_err(|e| e.to_string())?;
        let mod_key = mod_key(&config, nested);
        let keyboard = Keyboard::new(&config, mod_key);
        let mut hotkeys = HotkeyOverlay::new(&config, mod_key);
        hotkeys.set_open(config.overlay.at_startup, 0.0);
        let now = Instant::now();
        Ok(Self {
            services,
            composer: Composer::new(&config),
            hotkeys,
            config,
            config_watch: None,
            xwayland: None,
            lock: LockState::default(),
            held: None,
            touching: TouchState::default(),
            frame_clock: FrameClock::default(),
            inhibitors: Vec::new(),
            config_path,
            socket,
            nested,
            desktop,
            windows: Windows::default(),
            layers,
            outputs: Vec::new(),
            scene: Vec::new(),
            keyboard,
            placements: Vec::new(),
            layer_focus: None,
            activators: Vec::new(),
            focus: None,
            epoch: now,
            dirty: true,
            animating: false,
            presented: false,
            last_frame: now,
            quit: false,
        })
    }

    /// How many windows are on screen or could be: for tests and status.
    pub fn windows_mapped(&self) -> usize {
        self.windows.mapped()
    }

    /// Whether the last scene shows at least `count` client surfaces with a size.
    pub fn scene_has_surfaces(&self, count: usize) -> bool {
        let live = |item: &&SceneItem| item.kind == SceneKind::Surface && !item.rect.is_empty();
        self.scene.iter().filter(live).count() >= count
    }

    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// The name clients connect to.
    pub fn socket(&self) -> &str {
        &self.socket
    }

    pub(crate) fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// A CLOCK_MONOTONIC time, as `Present` gives them, on the clock of `now()`.
    fn mono_seconds(&self, sec: u64, nsec: u32) -> f64 {
        let mut then = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: a valid pointer to a timespec, for the duration of the call.
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut then) };
        let mono_now = then.tv_sec as f64 + then.tv_nsec as f64 / 1e9;
        self.now() + (sec as f64 + f64::from(nsec) / 1e9 - mono_now)
    }

    /// What's under a point: its monitor's, as an item past a shared edge is only
    /// drawn on its own.
    pub(crate) fn hit_at(&self, x: f64, y: f64) -> SceneHit {
        let monitor = self
            .desktop
            .monitors()
            .iter()
            .find(|m| m.rect.contains(x, y));
        let output = monitor.and_then(|m| self.outputs.iter().find(|o| o.id().0 == m.id));
        let key = output.map_or(0, Output::key);
        let shown = |item: &&SceneItem| item.only_on == 0 || item.only_on == key;
        let items: Vec<SceneItem> = self.scene.iter().filter(shown).copied().collect();
        scene_hit_test(&items, x, y)
    }

    /// How long the loop may sleep: for ever when nothing moves.
    pub fn timeout(&self) -> Option<Duration> {
        let animating = self.animating.then_some(PRESENT_TIMEOUT);
        match (animating, self.lock_timeout()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Start a program, detached so it leaves no zombie behind.
    pub(crate) fn spawn(&self, command: &mut Command) {
        let spawned = command
            .env("WAYLAND_DISPLAY", &self.socket)
            .env("XDG_CURRENT_DESKTOP", "pastel")
            .stdin(Stdio::null());
        // Off means off, even under a parent session that has an X server.
        match &self.xwayland {
            Some(xwayland) => spawned.env("DISPLAY", xwayland.display_name()),
            None => spawned.env_remove("DISPLAY"),
        };
        let spawned = spawned.status();
        if let Err(e) = spawned {
            eprintln!("pastel: spawn failed: {e}");
        }
    }

    pub fn start_xwayland(&mut self) {
        let services = &self.services;
        self.xwayland = Xwayland::start(services.display, &self.config.xwayland, &self.socket);
    }

    /// What X clients find in `DISPLAY`, while X11 is on.
    pub fn x11_display(&self) -> Option<String> {
        self.xwayland.as_ref().map(Xwayland::display_name)
    }

    /// The configured keyboard, for us and for clients. Under another compositor
    /// keys arrive already read with the parent's keymap, so that one stays.
    pub fn apply_keymap(&mut self) {
        if self.nested {
            return;
        }
        let names = &self.config.input.keymap;
        let compiled = KeymapState::from_names(&names.layout, &names.variant, &names.options);
        let keymap = match compiled {
            Ok(keymap) => keymap,
            Err(e) => {
                eprintln!("pastel: keeping the old keymap: {e}");
                return;
            }
        };
        let text = keymap.text();
        self.services.backend.set_keymap(&text);
        self.services.globals.seat().set_keymap(&text);
        self.keyboard.forget_base();
    }

    pub fn autostart(&self) {
        for line in &self.config.autostart {
            self.spawn(Command::new("sh").args(["-c", &format!("{line} &")]));
        }
    }

    pub fn reload_config(&mut self) {
        let loaded = match &self.config_path {
            Some(path) => Config::load(path),
            None => Config::parse(""),
        };
        let config = match loaded {
            Ok(config) => config,
            Err(e) => {
                eprintln!("pastel: keeping the old config: {e}");
                return;
            }
        };
        let mod_key = mod_key(&config, self.nested);
        self.desktop.set_options(config.layout.clone());
        self.composer.reconfigure(&config);
        self.hotkeys.reconfigure(&config, mod_key);
        self.keyboard = Keyboard::new(&config, mod_key);
        let outputs_changed = config.outputs != self.config.outputs;
        let keymap_changed = config.input.keymap != self.config.input.keymap;
        let xwayland_changed = config.xwayland != self.config.xwayland;
        self.services
            .screens
            .set_background(color(config.background));
        self.config = config;
        if keymap_changed {
            self.apply_keymap();
        }
        if outputs_changed {
            self.apply_output_config();
        }
        if xwayland_changed {
            // Letting go first, so a new path can have the same display.
            self.xwayland = None;
            self.start_xwayland();
        }
        self.dirty = true;
    }

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Input(input) => self.input(input),
            Event::Button {
                pressed: true, hit, ..
            } => self.click(hit.tag, hit.surface),
            Event::Output(id, OutputEvent::Present(shown)) => {
                self.presented = true;
                self.lock_presented(id);
                self.frame_clock.presented(
                    self.mono_seconds(shown.tv_sec, shown.tv_nsec),
                    f64::from(shown.refresh_ns) / 1e9,
                );
            }
            Event::Output(_, OutputEvent::ModeChanged) | Event::LayoutChanged => {
                self.arrange_monitors();
            }
            Event::OutputAdded(output) => self.output_added(output),
            Event::OutputConfig(request) => self.output_request(request),
            Event::OutputRemoved(id) => {
                self.outputs.retain(|o| o.id() != id);
                self.forget_lock_monitor(id);
                self.layers.close_output(id.0 as u32);
                self.desktop.remove_monitor(id.0);
                self.composer.remove_monitor(id.0);
                self.dirty = true;
            }
            Event::Toplevel(surface, event) => self.toplevel(surface, event),
            Event::Layer(surface, event) => self.layer(surface, event),
            Event::Activation(event) => self.activation(event),
            Event::SessionLock(event) => self.lock_event(event),
            Event::IdleInhibit(event) => self.idle_inhibit(event),
            Event::ForeignToplevel(request) => self.foreign_toplevel(request),
            Event::FdReady(tag @ (SOCKET_TAG | EXIT_TAG)) => {
                if let Some(xwayland) = &mut self.xwayland {
                    if tag == SOCKET_TAG {
                        xwayland.client_arrived();
                    } else {
                        xwayland.exited();
                    }
                }
            }
            Event::FdReady(CONFIG_TAG) => {
                if self.config_watch.as_mut().is_some_and(|w| w.changed()) {
                    self.reload_config();
                }
            }
            Event::Popup(..) => self.dirty = true,
            // A commit can change a window's size or geometry, which are in the scene.
            Event::Compositor(CompositorEvent::Commit(_)) => self.dirty = true,
            Event::SessionActive(active) => self.session_changed(active),
            Event::HostCloseRequest => self.quit = true,
            Event::ScreenFailed(message) => eprintln!("pastel: {message}"),
            _ => {}
        }
    }

    /// Tokens prove nothing by themselves, so the policy is here: one counts if
    /// the surface asking for it had the keyboard at the time. That is a click in a
    /// launcher or on a link; an app raising itself over what you're typing is not.
    fn activation(&mut self, event: ActivationEvent) {
        match event {
            ActivationEvent::TokenRequest(request) => {
                let held = self.layer_focus.or(self.focus);
                if let Some(surface) = request.surface.filter(|s| Some(*s) == held) {
                    if self.activators.len() == MAX_ACTIVATORS {
                        self.activators.remove(0);
                    }
                    self.activators.push(surface);
                }
            }
            ActivationEvent::Request(request) => {
                let trusted = request
                    .requesting_surface
                    .is_some_and(|surface| self.activators.contains(&surface));
                if request.token_valid && trusted {
                    self.layer_focus = None;
                    self.dirty |= self.desktop.focus_window(id_of(request.surface));
                }
            }
        }
    }

    fn idle_inhibit(&mut self, event: IdleInhibitEvent) {
        match event {
            IdleInhibitEvent::NewInhibitor { inhibitor, .. } => {
                self.inhibitors.push(inhibitor);
                self.dirty = true; // visibility is worked out with the scene
            }
            IdleInhibitEvent::InhibitorDestroy(id) => self.inhibitors.retain(|i| *i != id),
            IdleInhibitEvent::Changed(on) => self.services.idle.set_inhibited(on),
        }
    }

    /// Only what's on screen keeps the session awake: a video on another
    /// workspace shouldn't, and neither should anything behind the lock.
    fn update_inhibitors(&self) {
        let manager = self.services.inhibitors;
        for id in &self.inhibitors {
            let surface = manager.inhibitor_surface(*id);
            let shown = |item: &SceneItem| Some(item.surface) == surface;
            manager.set_visible(*id, self.scene.iter().any(shown));
        }
    }

    /// A taskbar or window switcher, which is on the user's side.
    fn foreign_toplevel(&mut self, request: ForeignToplevelRequest) {
        let id = id_of(request.surface);
        match request.kind {
            ForeignToplevelRequestKind::Activate => {
                self.layer_focus = None;
                self.desktop.focus_window(id);
            }
            ForeignToplevelRequestKind::Close => {
                if let Some(toplevel) = self.services.shell().toplevel(request.surface) {
                    toplevel.close();
                }
            }
            ForeignToplevelRequestKind::Fullscreen => self.desktop.set_fullscreen(id, true),
            ForeignToplevelRequestKind::Unfullscreen => self.desktop.set_fullscreen(id, false),
            // Columns have no minimized state, and maximize is a column width.
            _ => {}
        }
        self.dirty = true;
    }

    fn session_changed(&mut self, active: bool) {
        if active {
            self.services.screens.invalidate();
        } else {
            self.services.globals.seat().release_keys();
            self.keyboard.forget_held();
        }
    }

    fn toplevel(&mut self, surface: SurfaceId, event: ToplevelEvent) {
        let id = id_of(surface);
        let shell = self.services.shell();
        match event {
            ToplevelEvent::Created => {
                let Some(toplevel) = shell.toplevel(surface) else {
                    return;
                };
                let rule = self.config.rule_for(&toplevel.app_id(), &toplevel.title());
                let floating = rule.floating.unwrap_or(is_dialog(&toplevel));
                self.windows.insert(id, rule);
                let hint = WindowHint {
                    width: rule.default_width,
                    floating,
                };
                self.desktop.add_window(id, hint);
            }
            ToplevelEvent::Map => {
                let Some(window) = self.windows.get_mut(id) else {
                    return;
                };
                window.mapped = true;
                let rule = window.rule;
                let toplevel = shell.toplevel(surface);
                let dialog = toplevel.as_ref().is_some_and(is_dialog);
                let floating = rule.floating.unwrap_or(dialog);
                // A window that hid itself left the layout on unmap.
                let known = self.desktop.monitors().iter().flat_map(|m| &m.workspaces);
                if !known.into_iter().any(|w| w.contains(id)) {
                    let hint = WindowHint {
                        width: rule.default_width,
                        floating,
                    };
                    self.desktop.add_window(id, hint);
                }
                // Limits and parents can come after the role: this is the last say.
                self.desktop.set_floating(id, floating);
                if let Some(toplevel) = toplevel {
                    let size = toplevel.geometry();
                    if size.width > 0 && size.height > 0 {
                        let (w, h) = (f64::from(size.width), f64::from(size.height));
                        let parent = toplevel.parent().map(id_of);
                        self.desktop.fit_floating(id, w, h, parent);
                    }
                }
                self.desktop.focus_window(id);
            }
            ToplevelEvent::Unmap => {
                let Some(window) = self.windows.get_mut(id) else {
                    return;
                };
                window.mapped = false;
                window.configured = None;
                let rule = window.rule;
                self.desktop.remove_window(id);
                let now = self.now();
                let pixels = self.services.renderer.has_retained(surface);
                self.composer.close(id, rule, pixels, now);
            }
            ToplevelEvent::Destroy => {
                if let Some(window) = self.windows.remove(id) {
                    self.desktop.remove_window(id);
                    let now = self.now();
                    let pixels = self.services.renderer.has_retained(surface);
                    self.composer.close(id, window.rule, pixels, now);
                }
            }
            ToplevelEvent::RequestMove { .. } => {
                if self.may_grab(id) {
                    self.begin_move(id);
                }
            }
            ToplevelEvent::RequestResize { edges, .. } => {
                if self.may_grab(id) {
                    self.begin_resize(id, edges);
                }
            }
            ToplevelEvent::RequestFullscreen(on) => {
                self.desktop.set_fullscreen(id, on);
                self.answer_request(id);
            }
            // Not ours to grant, but every request is owed a configure.
            ToplevelEvent::RequestMaximize(_) | ToplevelEvent::RequestMinimize => {
                self.answer_request(id);
            }
            ToplevelEvent::IdentityChange => {
                let Some(toplevel) = shell.toplevel(surface) else {
                    return;
                };
                let rule = self.config.rule_for(&toplevel.app_id(), &toplevel.title());
                if let Some(window) = self.windows.get_mut(id) {
                    window.rule = rule;
                }
            }
            _ => return,
        }
        self.dirty = true;
    }

    fn answer_request(&mut self, id: pastel_layout::WindowId) {
        if let Some(window) = self.windows.get_mut(id) {
            window.configured = None;
        }
    }

    fn layer(&mut self, surface: SurfaceId, event: LayerSurfaceEvent) {
        let shell = self.services.layer_shell;
        match event {
            LayerSurfaceEvent::Created => self.place_layer(surface),
            LayerSurfaceEvent::Map => {
                self.layers.set_mapped(surface, true);
                let wants = shell
                    .layer_surface(surface)
                    .map(|l| l.keyboard_interactivity());
                if wants == Some(KeyboardInteractivity::Exclusive) {
                    self.layer_focus = Some(surface);
                }
            }
            LayerSurfaceEvent::Unmap => {
                self.layers.set_mapped(surface, false);
                self.clear_layer_focus(surface);
            }
            LayerSurfaceEvent::Destroy => {
                self.layers.remove(surface);
                self.clear_layer_focus(surface);
            }
            LayerSurfaceEvent::StateChange => {}
            LayerSurfaceEvent::NewPopup(popup) => {
                self.services
                    .popups
                    .set_parent(self.services.shell(), popup, surface);
                return;
            }
        }
        self.arrange_monitors();
    }

    /// On the output the client named: swaybg makes one surface per output, and
    /// piling them on the focused monitor would leave the others with no wallpaper
    /// for anything to show or blur. With no output named, the focused one.
    fn place_layer(&mut self, surface: SurfaceId) {
        let shell = self.services.layer_shell;
        let Some(layer) = shell.layer_surface(surface) else {
            return;
        };
        let output = if layer.has_output() {
            let named = self.services.screens.output_of(&layer, &self.outputs);
            named.map(|output| output.id().0)
        } else {
            self.desktop.focused_monitor().map(|monitor| monitor.id)
        };
        match output {
            Some(id) => {
                self.layers.add(shell, surface, id as u32);
            }
            // Its monitor left while the request was on its way, or there is none.
            None => layer.close(),
        }
    }

    fn clear_layer_focus(&mut self, surface: SurfaceId) {
        if self.layer_focus == Some(surface) {
            self.layer_focus = None;
        }
    }

    /// Outputs or panels changed: give every workspace its new area.
    pub(crate) fn arrange_monitors(&mut self) {
        for output in &self.outputs {
            let id = output.id().0;
            if self.desktop.monitor(id).is_none() {
                continue; // turned off
            }
            let full = self.services.layout.output_box(output);
            let usable = self.layers.arrange(id as u32, full, full);
            self.desktop
                .set_monitor_geometry(id, logical(full), logical(usable));
        }
        if let Some(monitor) = self.desktop.monitors().first() {
            let (w, h) = (monitor.usable.w as i32, monitor.usable.h as i32);
            self.services.shell().set_bounds((w, h));
        }
        self.size_lock_surfaces();
        self.services.output_manager.update();
        self.dirty = true;
    }

    /// Clicks give focus; a layer surface only if it can take it.
    pub(crate) fn click(&mut self, tag: u64, surface: Option<SurfaceId>) {
        if self.is_locked() {
            self.lock_click(surface);
            return;
        }
        let id = pastel_layout::WindowId(tag);
        if self.windows.contains(id) {
            self.layer_focus = None;
            self.desktop.focus_window(id);
        } else if let Some(target) = surface.and_then(|s| self.layers.keyboard_target(s)) {
            self.layer_focus = Some(target);
        }
        self.dirty = true;
    }

    /// Tell clients what the layout decided, sending only what changed.
    fn configure_windows(&mut self) {
        let shell = self.services.shell();
        let fractional = self.services.globals.fractional_scale();
        let Self {
            desktop,
            windows,
            placements,
            outputs,
            ..
        } = self;
        for monitor in desktop.monitors() {
            let scale = outputs
                .iter()
                .find(|o| o.id().0 == monitor.id)
                .map_or(1, Output::scale);
            for workspace in &monitor.workspaces {
                placements.clear();
                Desktop::place(monitor, workspace, placements);
                for placement in placements.iter() {
                    // 0 is "you say": a floating window's own size, while unknown.
                    let size = if placement.sized {
                        (placement.rect.w as i32, placement.rect.h as i32)
                    } else {
                        (0, 0)
                    };
                    let wanted = (size.0, size.1, placement.fullscreen, placement.floating);
                    let Some(window) = windows.get_mut(placement.id) else {
                        continue;
                    };
                    if window.scale != scale {
                        window.scale = scale;
                        let surface = surface_of(placement.id);
                        fractional.set_scale(surface, f64::from(scale));
                    }
                    if window.configured == Some(wanted) {
                        continue;
                    }
                    if let Some(toplevel) = shell.toplevel(surface_of(placement.id)) {
                        // Size first: luminaria's state setters each send a
                        // configure of their own, and none of them should
                        // carry the new state at the old size.
                        toplevel.configure(size);
                        toplevel.set_tiled(!placement.fullscreen && !placement.floating);
                        toplevel.set_fullscreen(placement.fullscreen);
                        window.configured = Some(wanted);
                    }
                }
            }
        }
    }

    /// Give the keyboard to whoever should have it.
    fn sync_focus(&mut self) {
        if self.is_locked() {
            let want = self.lock_keyboard_target();
            if want != self.focus {
                self.focus = want;
                let seat = self.services.globals.seat();
                self.services.shell().focus(seat, None);
                seat.set_keyboard_focus(want);
            }
            return;
        }
        let window = self
            .desktop
            .focused_window()
            .filter(|&id| self.windows.get(id).is_some_and(|w| w.mapped));
        let want = self.layer_focus.or(window.map(surface_of));
        if want == self.focus {
            return;
        }
        self.focus = want;
        let (shell, seat) = (self.services.shell(), self.services.globals.seat());
        if self.layer_focus.is_some() {
            shell.focus(seat, None);
            seat.set_keyboard_focus(want);
        } else {
            shell.focus(seat, want);
        }
    }

    /// Bring clients and the screen up to date. Does nothing unless something
    /// changed, or an animation is due its next frame.
    pub fn frame(&mut self) {
        self.check_lock_deadline();
        let timed_out = self.last_frame.elapsed() >= PRESENT_TIMEOUT;
        if !self.dirty && !(self.animating && (self.presented || timed_out)) {
            return;
        }
        if self.dirty {
            self.configure_windows();
            self.sync_focus();
        }
        self.dirty = false;
        self.presented = false;
        self.last_frame = Instant::now();

        if self.is_locked() {
            self.animating = false;
            self.lock_scene();
            self.services.screens.set_scene(&self.scene);
            self.update_inhibitors();
            return;
        }
        // Where things should be when this frame is seen, not when it's drawn.
        let now = self.frame_clock.next_frame(self.now());
        let host = SceneHost {
            windows: &self.windows,
            shell: self.services.shell(),
            layers: &self.layers,
            popups: self.services.popups,
            outputs: &self.outputs,
        };
        let mut animating = self
            .composer
            .compose(&mut self.desktop, &host, now, &mut self.scene);
        if let Some(monitor) = self.desktop.focused_monitor() {
            let context = OverlayContext {
                renderer: self.services.renderer,
                monitor: monitor.rect,
                scale: self
                    .outputs
                    .iter()
                    .find(|o| o.id().0 == monitor.id)
                    .map_or(1, Output::scale),
                now,
            };
            animating |= self.hotkeys.draw(&context, &mut self.scene);
        }
        self.animating = animating;
        self.services.screens.set_scene(&self.scene);
        self.update_inhibitors();
    }
}

/// How many remembered activators, oldest forgotten first. A launcher that closed
/// after asking still counts, but this mustn't grow for as long as a session lasts.
const MAX_ACTIVATORS: usize = 8;

/// A window that belongs to another, or can only be one size, isn't a column.
fn is_dialog(toplevel: &Toplevel<'_>) -> bool {
    let (min, max) = (toplevel.min_size(), toplevel.max_size());
    toplevel.parent().is_some() || (min == max && min.0 > 0 && min.1 > 0)
}

pub(crate) fn mod_key(config: &Config, nested: bool) -> u32 {
    if nested {
        config.input.nested_mod_key
    } else {
        config.input.mod_key
    }
}
