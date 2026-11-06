//! The keyboard and pointer. Pastel takes input before clients do: bindings and
//! overlays are matched here, and what's left goes on through `InputRouter::route`.

use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_char, c_int};
use std::process::Command;

use luminaria::backend::{InputEvent, Modifiers};
use luminaria::util::{KeymapState, Keysym};
use pastel_config::{Action, Config};
use pastel_layout::{Direction, Edges, LayoutOp, WindowId};
use pastel_render::Overlay;

use crate::state::{State, mod_key};
use crate::windows::surface_of;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
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
    pub(crate) base: Option<KeymapState>,
    mods: u32,
    /// What virtual keyboards last said the modifiers are.
    pub(crate) virtual_mods: luminaria::backend::Modifiers,
    /// Virtual pointer scrolling, until its frame.
    pub(crate) virtual_axis: crate::ime::VirtualAxis,
    /// The seat's keymap while a virtual keyboard's own is in use.
    pub(crate) physical_keymap: Option<String>,
    /// That virtual keyboard's keymap, to read its keys with.
    pub(crate) virtual_keymap: Option<KeymapState>,
    /// Mod+drag is panning, from this pointer position.
    pan: Option<f64>,
    /// Mouse buttons down right now.
    buttons: u32,
    /// The press that started a drag was ours, so its release is too.
    drag_button: Option<u32>,
    /// The active layout, as xkb's group.
    pub(crate) group: u32,
    /// The keymap's layout names, read when first asked for after a change.
    pub(crate) layouts: std::cell::RefCell<Option<Vec<String>>>,
}

/// The layout names of an xkb keymap's text, in group order: its symbols'
/// `name[GroupN]="..."` lines.
pub fn layout_names(keymap: &str) -> Vec<String> {
    let Some(start) = keymap.find("xkb_symbols") else {
        return Vec::new();
    };
    let mut names: Vec<(usize, String)> = keymap[start..]
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("name[")?;
            let (group, rest) = line.split_once(']')?;
            let digits = group.trim_start_matches(|c: char| !c.is_ascii_digit());
            let (_, quoted) = rest.split_once('"')?;
            let (name, _) = quoted.rsplit_once('"')?;
            Some((digits.parse().ok()?, name.to_owned()))
        })
        .collect();
    names.sort_by_key(|(group, _)| *group);
    names.into_iter().map(|(_, name)| name).collect()
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
            virtual_mods: Default::default(),
            virtual_axis: Default::default(),
            physical_keymap: None,
            virtual_keymap: None,
            pan: None,
            buttons: 0,
            drag_button: None,
            group: 0,
            layouts: Default::default(),
        }
    }

    pub fn buttons_held(&self) -> u32 {
        self.buttons
    }

    /// The keymap changed: the unshifted symbols come from the new one.
    pub fn forget_base(&mut self) {
        self.base = None;
    }

    pub fn forget_held(&mut self) {
        self.swallowed.clear();
        self.mods = 0;
        self.pan = None;
        self.buttons = 0;
        self.drag_button = None;
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
    /// Another layout became active: shells show it.
    fn layout_group(&mut self, group: u32) {
        if self.keyboard.group != group {
            self.keyboard.group = group;
            self.publish_ipc();
        }
    }

    /// The active layout's name.
    pub fn keyboard_layout(&self) -> String {
        let mut layouts = self.keyboard.layouts.borrow_mut();
        let names = layouts.get_or_insert_with(|| {
            let keymap = (self.keyboard.physical_keymap.clone())
                .unwrap_or_else(|| self.services.globals.seat().keymap());
            layout_names(&keymap)
        });
        let group = self.keyboard.group as usize;
        names
            .get(group)
            .or(names.first())
            .cloned()
            .unwrap_or_default()
    }

    pub fn input(&mut self, event: InputEvent) {
        self.services.idle.notify_activity();
        if let InputEvent::Button { pressed, .. } = event {
            let held = &mut self.keyboard.buttons;
            *held = if pressed {
                *held + 1
            } else {
                held.saturating_sub(1)
            };
        }
        if self.touch(&event) {
            return;
        }
        match event {
            InputEvent::Gesture(gesture) => {
                self.gesture(gesture);
                return;
            }
            InputEvent::Tablet(input) => {
                self.tablet(&input);
                return;
            }
            InputEvent::Key {
                key,
                pressed,
                keysym,
                modifiers,
            } => {
                self.restore_physical_keymap();
                self.keyboard.mods = modifiers.held();
                self.layout_group(modifiers.group);
                self.key(&event, key, pressed, keysym, modifiers);
                return;
            }
            InputEvent::Modifiers(modifiers) => {
                self.keyboard.mods = modifiers.held();
                self.layout_group(modifiers.group);
            }
            InputEvent::KeymapChange(_) => {
                self.keyboard.base = None;
                self.keyboard.layouts.replace(None);
                self.publish_ipc();
            }
            InputEvent::Motion { dx, .. } => self.pan_by(dx),
            InputEvent::MotionAbsolute { x, .. } => {
                if let Some(last) = self.keyboard.pan {
                    self.pan_by(x - last);
                    self.keyboard.pan = Some(x);
                }
            }
            InputEvent::Button { button, pressed }
                if !self.is_locked() && self.pointer_binding(button, pressed) =>
            {
                self.end_grab_with_last_button();
                return;
            }
            InputEvent::Axis(axis) if self.mod_held() && !self.is_locked() => {
                self.mod_scroll(axis.dy_steps + axis.dx_steps, axis.dx + axis.dy);
                return;
            }
            _ => {}
        }
        self.route_key(&event);
        self.pointer_moved();
        self.drag_moved();
        self.end_grab_with_last_button();
    }

    fn end_grab_with_last_button(&mut self) {
        if self.keyboard.buttons == 0 && self.held.is_some() {
            self.grab_ended();
        }
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
                self.route_key(event);
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
        // What's typed here is a password. Nothing but the lock reads it.
        if self.is_locked() {
            self.services.router.route(event);
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
            None => self.route_key(event),
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
            Action::ToggleFloating => {
                self.desktop.toggle_floating();
            }
            Action::FocusFloating => {
                self.desktop.switch_floating_focus();
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

    /// With Mod: left drags a floating window or pans the view, right resizes a
    /// window by the corner nearest the pointer. True when the button is ours.
    fn pointer_binding(&mut self, button: u32, pressed: bool) -> bool {
        if !pressed {
            let pan = button == BTN_LEFT && self.keyboard.pan.take().is_some();
            return pan
                || self
                    .keyboard
                    .drag_button
                    .take_if(|b| *b == button)
                    .is_some();
        }
        if !self.mod_held() || (button != BTN_LEFT && button != BTN_RIGHT) {
            return false;
        }
        let Some((x, y)) = self.services.cursor.position() else {
            return false;
        };
        let hit = self.hit_at(x, y);
        let window = Some(WindowId(hit.tag)).filter(|id| self.windows.contains(*id));
        match (button, window) {
            (BTN_LEFT, Some(id)) if self.desktop.floating(id).is_some() => {
                self.keyboard.drag_button = Some(button);
                self.begin_move(id);
            }
            (BTN_LEFT, _) => self.keyboard.pan = Some(x),
            (_, Some(id)) => {
                self.keyboard.drag_button = Some(button);
                if let Some(rect) = self.desktop.rect_of(id) {
                    self.begin_resize_at(id, Edges::nearest(rect, x, y));
                }
            }
            _ => return false,
        }
        true
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
        self.grab_moved();
        let Some((x, y)) = self.services.cursor.position() else {
            return;
        };
        self.desktop.focus_monitor_at(x, y);
        // Focusing what a drag passes over would scroll it away from under the
        // pointer before the drop.
        let no_follow = !self.config.input.focus_follows_mouse
            || self.is_locked()
            || self.held.is_some()
            || self.dragging();
        if no_follow || self.keyboard.pan.is_some() {
            return;
        }
        let hit = self.hit_at(x, y);
        let id = pastel_layout::WindowId(hit.tag);
        if hit.hit && self.windows.contains(id) && self.desktop.focused_window() != Some(id) {
            self.desktop.focus_window(id);
            self.dirty = true;
        }
    }
}
