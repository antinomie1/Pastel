//! The compositor's state and the loop's two halves: [`State::handle`] takes in what
//! happened, [`State::frame`] brings clients and the screen up to date.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use luminaria::Event;
use luminaria::backend::{Output, OutputEvent, Session};
use luminaria::gpu::{Cursor, SceneRenderer};
use luminaria::protocol::{
    CompositorEvent, CoreGlobals, KeyboardInteractivity, LayerShell, LayerSurfaceEvent, SurfaceId,
    ToplevelEvent, XdgShell,
};
use luminaria::shell::{
    InputRouter, LayerManager, OutputLayout, PopupTree, SceneItem, SceneKind, ScreenManager,
};
use luminaria::util::Rect as PixelRect;
use pastel_config::Config;
use pastel_core::Rect;
use pastel_layout::{Desktop, Placement, WindowHint};
use pastel_render::{Composer, HotkeyOverlay, Overlay, OverlayContext};

use crate::input::Keyboard;
use crate::windows::{SceneHost, Windows, id_of, surface_of};

/// How long to wait for a `Present` before drawing the next animation frame
/// anyway. Animation is paced by the display; this only covers one being off.
const PRESENT_TIMEOUT: Duration = Duration::from_millis(32);

/// The parts of luminaria that everything else goes through.
pub struct Services<'a> {
    pub globals: &'a CoreGlobals<'a>,
    pub layer_shell: &'a LayerShell<'a>,
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
    config_path: Option<PathBuf>,
    socket: String,
    pub(crate) nested: bool,

    pub desktop: Desktop,
    pub windows: Windows,
    layers: LayerManager,
    /// Held for their `Present` events; ScreenManager owns everything else.
    outputs: Vec<Output>,

    composer: Composer,
    pub hotkeys: HotkeyOverlay,
    pub scene: Vec<SceneItem>,
    pub(crate) keyboard: Keyboard,
    placements: Vec<Placement>,

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

fn logical(rect: PixelRect) -> Rect {
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

    /// The name clients connect to.
    pub fn socket(&self) -> &str {
        &self.socket
    }

    pub(crate) fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// How long the loop may sleep: for ever when nothing moves.
    pub fn timeout(&self) -> Option<Duration> {
        self.animating.then_some(PRESENT_TIMEOUT)
    }

    /// Start a program, detached so it leaves no zombie behind.
    pub(crate) fn spawn(&self, command: &mut Command) {
        let spawned = command
            .env("WAYLAND_DISPLAY", &self.socket)
            .env("XDG_CURRENT_DESKTOP", "pastel")
            .stdin(Stdio::null())
            .status();
        if let Err(e) = spawned {
            eprintln!("pastel: spawn failed: {e}");
        }
    }

    pub fn autostart(&self) {
        for line in &self.config.autostart {
            self.spawn(Command::new("sh").args(["-c", &format!("{line} &")]));
        }
    }

    pub(crate) fn reload_config(&mut self) {
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
        self.config = config;
        self.dirty = true;
    }

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Input(input) => self.input(input),
            Event::Button { pressed: true, hit, .. } => self.click(hit.tag, hit.surface),
            Event::Output(_, OutputEvent::Present(_)) => self.presented = true,
            Event::Output(_, OutputEvent::ModeChanged) | Event::LayoutChanged => {
                self.arrange_monitors();
            }
            Event::OutputAdded(output) => {
                let rect = logical(self.services.layout.output_box(&output));
                self.desktop.add_monitor(output.id().0, rect);
                self.outputs.push(output);
                self.arrange_monitors();
            }
            Event::OutputRemoved(id) => {
                self.outputs.retain(|o| o.id() != id);
                self.layers.close_output(id.0 as u32);
                self.desktop.remove_monitor(id.0);
                self.composer.remove_monitor(id.0);
                self.dirty = true;
            }
            Event::Toplevel(surface, event) => self.toplevel(surface, event),
            Event::Layer(surface, event) => self.layer(surface, event),
            Event::Popup(..) => self.dirty = true,
            // A commit can change a window's size or geometry, which are in the scene.
            Event::Compositor(CompositorEvent::Commit(_)) => self.dirty = true,
            Event::SessionActive(active) => self.session_changed(active),
            Event::HostCloseRequest => self.quit = true,
            Event::ScreenFailed(message) => eprintln!("pastel: {message}"),
            _ => {}
        }
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
                let Some(toplevel) = shell.toplevel(surface) else { return };
                let rule = self.config.rule_for(&toplevel.app_id(), &toplevel.title());
                self.windows.insert(id, rule);
                self.desktop.add_window(id, WindowHint { width: rule.default_width });
            }
            ToplevelEvent::Map => {
                let Some(window) = self.windows.get_mut(id) else { return };
                window.mapped = true;
                let scale = f64::from(self.services.layout.primary_scale());
                self.services.globals.fractional_scale().set_scale(surface, scale);
                let rule = window.rule;
                // A window that hid itself left the layout on unmap.
                let known = self.desktop.monitors().iter().flat_map(|m| &m.workspaces);
                if !known.into_iter().any(|w| w.layout.contains(id)) {
                    self.desktop.add_window(id, WindowHint { width: rule.default_width });
                }
                self.desktop.focus_window(id);
            }
            ToplevelEvent::Unmap => {
                let Some(window) = self.windows.get_mut(id) else { return };
                window.mapped = false;
                window.configured = None;
                let rule = window.rule;
                self.desktop.remove_window(id);
                let now = self.now();
                self.composer.close(id, rule, now);
            }
            ToplevelEvent::Destroy => {
                if let Some(window) = self.windows.remove(id) {
                    self.desktop.remove_window(id);
                    let now = self.now();
                    self.composer.close(id, window.rule, now);
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
                let Some(toplevel) = shell.toplevel(surface) else { return };
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
            LayerSurfaceEvent::Created => {
                if let Some(monitor) = self.desktop.focused_monitor() {
                    self.layers.add(shell, surface, monitor.id as u32);
                }
            }
            LayerSurfaceEvent::Map => {
                self.layers.set_mapped(surface, true);
                let wants = shell.layer_surface(surface).map(|l| l.keyboard_interactivity());
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
                self.services.popups.set_parent(self.services.shell(), popup, surface);
                return;
            }
        }
        self.arrange_monitors();
    }

    fn clear_layer_focus(&mut self, surface: SurfaceId) {
        if self.layer_focus == Some(surface) {
            self.layer_focus = None;
        }
    }

    /// Outputs or panels changed: give every workspace its new area.
    fn arrange_monitors(&mut self) {
        for output in &self.outputs {
            let full = self.services.layout.output_box(output);
            let id = output.id().0;
            let usable = self.layers.arrange(id as u32, full, full);
            self.desktop.set_monitor_geometry(id, logical(full), logical(usable));
        }
        if let Some(monitor) = self.desktop.monitors().first() {
            let (w, h) = (monitor.usable.w as i32, monitor.usable.h as i32);
            self.services.shell().set_bounds((w, h));
        }
        self.dirty = true;
    }

    /// Clicks give focus; a layer surface only if it can take it.
    fn click(&mut self, tag: u64, surface: Option<SurfaceId>) {
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
        let Self { desktop, windows, placements, .. } = self;
        for monitor in desktop.monitors() {
            for workspace in &monitor.workspaces {
                placements.clear();
                Desktop::place(monitor, workspace, placements);
                for placement in placements.iter() {
                    let size = (placement.rect.w as i32, placement.rect.h as i32);
                    let wanted = (size.0, size.1, placement.fullscreen);
                    let Some(window) = windows.get_mut(placement.id) else { continue };
                    if window.configured == Some(wanted) {
                        continue;
                    }
                    if let Some(toplevel) = shell.toplevel(surface_of(placement.id)) {
                        // Size first: luminaria's state setters each send a
                        // configure of their own, and none of them should
                        // carry the new state at the old size.
                        toplevel.configure(size);
                        toplevel.set_tiled(!placement.fullscreen);
                        toplevel.set_fullscreen(placement.fullscreen);
                        window.configured = Some(wanted);
                    }
                }
            }
        }
    }

    /// Give the keyboard to whoever should have it.
    fn sync_focus(&mut self) {
        let window = self.desktop.focused_window().filter(|&id| {
            self.windows.get(id).is_some_and(|w| w.mapped)
        });
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

        let now = self.now();
        let host = SceneHost {
            windows: &self.windows,
            shell: self.services.shell(),
            layers: &self.layers,
            popups: self.services.popups,
        };
        let mut animating =
            self.composer.compose(&mut self.desktop, &host, now, &mut self.scene);
        if let Some(monitor) = self.desktop.focused_monitor() {
            let context =
                OverlayContext { renderer: self.services.renderer, monitor: monitor.rect, now };
            animating |= self.hotkeys.draw(&context, &mut self.scene);
        }
        self.animating = animating;
        self.services.screens.set_scene(&self.scene);
    }
}

pub(crate) fn mod_key(config: &Config, nested: bool) -> u32 {
    if nested { config.input.nested_mod_key } else { config.input.mod_key }
}
