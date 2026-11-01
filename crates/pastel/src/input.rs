//! The keyboard and pointer. Pastel takes input before clients do: bindings and
//! overlays are matched here, and what's left goes on through `InputRouter::route`.

use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_char, c_int};
use std::process::Command;

use luminaria::backend::{InputEvent, Modifiers};
use luminaria::util::{KeymapState, Keysym};
use pastel_config::{Action, Config};
use pastel_layout::{Direction, LayoutOp};
use pastel_render::Overlay;

use crate::state::{State, mod_key};
use crate::windows::surface_of;

const BTN_LEFT: u32 = 0x110;
/// Pixels of view per pixel of touchpad scroll with Mod held.
const SCROLL_SPEED: f64 = 2.0;

// Luminaria already links xkbcommon. Key names are the one thing it doesn't wrap.
const XKB_KEYSYM_CASE_INSENSITIVE: c_int = 1;
unsafe extern "C" {
    fn xkb_keysym_from_name(name: *const c_char, flags: c_int) -> u32;
}

fn keysym_named(name: &str) -> Option<Keysym> {
    let name = CString::new(name).ok()?;
    // SAFETY: `name` is a valid NUL-terminated string that outlives the call,
    // and xkb_keysym_from_name only reads it.
    let raw = unsafe { xkb_keysym_from_name(name.as_ptr(), XKB_KEYSYM_CASE_INSENSITIVE) };
    (raw != 0).then(|| Keysym::new(raw).to_lower())
}

fn is_modifier(keysym: Keysym) -> bool {
    // XKB_KEY_Shift_L to XKB_KEY_Hyper_R, and the ISO shifts.
    matches!(keysym.raw(), 0xffe1..=0xffee | 0xfe01..=0xfe13)
}

/// Keyboard state that outlives one event.
pub struct Keyboard {
    binds: HashMap<(u32, u32), Action>,
    /// Keys taken from clients on press, whose releases they mustn't see either.
    swallowed: HashSet<u32>,
    /// A fresh state: asked about a key, it answers with the unshifted symbol, so
    /// `Mod+Shift+1` matches though the key really produces `!`.
    base: Option<KeymapState>,
    mods: u32,
    /// Mod+drag is panning, from this pointer position.
    pan: Option<f64>,
}

impl Keyboard {
    pub fn new(config: &Config, mod_key: u32) -> Self {
        let mut binds = HashMap::new();
        for (combo, action) in config.binds.table(mod_key) {
            match keysym_named(&combo.key) {
                Some(keysym) => {
                    binds.insert((combo.mods, keysym.raw()), action);
                }
                None => eprintln!("pastel: no such key `{}`", combo.key),
            }
        }
        Self {
            binds,
            swallowed: HashSet::new(),
            base: None,
            mods: 0,
            pan: None,
        }
    }

    pub fn forget_held(&mut self) {
        self.swallowed.clear();
        self.mods = 0;
        self.pan = None;
    }

    fn lookup(&self, held: u32, keysym: Keysym, key: u32) -> Option<&Action> {
        let exact = self.binds.get(&(held, keysym.to_lower().raw()));
        exact.or_else(|| {
            let base = Keysym::new(self.base.as_ref()?.keysym(key));
            self.binds.get(&(held, base.to_lower().raw()))
        })
    }
}

impl State<'_> {
    pub fn input(&mut self, event: InputEvent) {
        match event {
            InputEvent::Key {
                key,
                pressed,
                keysym,
                modifiers,
            } => {
                self.keyboard.mods = modifiers.held();
                self.key(&event, key, pressed, keysym, modifiers);
                return;
            }
            InputEvent::Modifiers(modifiers) => self.keyboard.mods = modifiers.held(),
            InputEvent::KeymapChange(_) => self.keyboard.base = None,
            InputEvent::Motion { dx, .. } => self.pan_by(dx),
            InputEvent::MotionAbsolute { x, .. } => {
                if let Some(last) = self.keyboard.pan {
                    self.pan_by(x - last);
                    self.keyboard.pan = Some(x);
                }
            }
            InputEvent::Button { button, pressed } if self.pointer_binding(button, pressed) => {
                return;
            }
            InputEvent::Axis(axis) if self.mod_held() => {
                self.mod_scroll(axis.dy_steps + axis.dx_steps, axis.dx + axis.dy);
                return;
            }
            _ => {}
        }
        self.services.router.route(&event);
        self.pointer_moved();
    }

    fn mod_key(&self) -> u32 {
        mod_key(&self.config, self.nested)
    }

    fn mod_held(&self) -> bool {
        self.keyboard.mods & self.mod_key() != 0
    }

    fn key(
        &mut self,
        event: &InputEvent,
        key: u32,
        pressed: bool,
        keysym: Keysym,
        mods: Modifiers,
    ) {
        if !pressed {
            if !self.keyboard.swallowed.remove(&key) {
                self.services.router.route(event);
            }
            return;
        }
        if let Some(session) = self.services.session
            && session.switch_vt_for(keysym)
        {
            self.services.globals.seat().release_keys();
            self.keyboard.forget_held();
            return;
        }
        if self.hotkeys.is_modal() && !is_modifier(keysym) {
            self.keyboard.swallowed.insert(key);
            let now = self.now();
            self.hotkeys.on_key(keysym, now);
            self.dirty = true;
            return;
        }
        if self.keyboard.base.is_none() {
            self.keyboard.base =
                KeymapState::from_text(&self.services.globals.seat().keymap()).ok();
        }
        match self.keyboard.lookup(mods.held(), keysym, key).cloned() {
            Some(action) => {
                self.keyboard.swallowed.insert(key);
                self.run(action);
            }
            None => self.services.router.route(event),
        }
    }

    pub fn run(&mut self, action: Action) {
        let now = self.now();
        match action {
            Action::None => return,
            Action::Quit => self.quit = true,
            Action::Spawn(command) => {
                // `sh` exits at once, so the program is nobody's child.
                self.spawn(
                    Command::new("sh")
                        .args(["-c", "\"$@\" &", "sh"])
                        .args(&command),
                );
            }
            Action::CloseWindow => {
                let focused = self.desktop.focused_window();
                let toplevel =
                    focused.and_then(|id| self.services.shell().toplevel(surface_of(id)));
                if let Some(toplevel) = toplevel {
                    toplevel.close();
                }
            }
            Action::ToggleFullscreen => {
                self.desktop.toggle_fullscreen();
            }
            Action::ToggleOverlay => self.hotkeys.toggle(now),
            Action::ReloadConfig => self.reload_config(),
            Action::Layout(op) => {
                self.desktop.apply(op);
            }
            Action::FocusWorkspace(target) => {
                self.desktop.focus_workspace(target);
            }
            Action::MoveToWorkspace(target) => {
                self.desktop.move_to_workspace(target);
            }
            Action::FocusMonitor(direction) => {
                self.desktop.focus_monitor(direction);
            }
            Action::MoveToMonitor(direction) => {
                self.desktop.move_to_monitor(direction);
            }
        }
        self.dirty = true;
    }

    /// Mod+left-drag pans the view. True when the button is ours.
    fn pointer_binding(&mut self, button: u32, pressed: bool) -> bool {
        if button != BTN_LEFT {
            return false;
        }
        if pressed && self.mod_held() {
            let x = self.services.cursor.position().map_or(0.0, |p| p.0);
            self.keyboard.pan = Some(x);
            return true;
        }
        !pressed && self.keyboard.pan.take().is_some()
    }

    fn pan_by(&mut self, dx: f64) {
        if self.keyboard.pan.is_some() {
            self.desktop.apply(LayoutOp::Scroll(-dx));
            self.dirty = true;
        }
    }

    /// Mod+wheel steps between columns; a touchpad, with no steps, pans smoothly.
    fn mod_scroll(&mut self, steps: i32, pixels: f64) {
        let op = match steps {
            0 => LayoutOp::Scroll(pixels * SCROLL_SPEED),
            s if s > 0 => LayoutOp::Focus(Direction::Right),
            _ => LayoutOp::Focus(Direction::Left),
        };
        self.desktop.apply(op);
        self.dirty = true;
    }

    /// Keep the focused monitor under the pointer, and focus following it.
    fn pointer_moved(&mut self) {
        let Some((x, y)) = self.services.cursor.position() else {
            return;
        };
        self.desktop.focus_monitor_at(x, y);
        if !self.config.input.focus_follows_mouse || self.keyboard.pan.is_some() {
            return;
        }
        let hit = luminaria::shell::scene_hit_test(&self.scene, x, y);
        let id = pastel_layout::WindowId(hit.tag);
        if hit.hit && self.windows.contains(id) && self.desktop.focused_window() != Some(id) {
            self.desktop.focus_window(id);
            self.dirty = true;
        }
    }
}
