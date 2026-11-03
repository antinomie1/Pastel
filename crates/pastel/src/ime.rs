//! Input methods and injected input.
//!
//! luminaria bridges text-input-v3 and input-method-v2 by itself; what is
//! Pastel's is the policy around it. While an input method (fcitx5, IBus) holds
//! the keyboard grab, keys that are not Pastel's bindings go to it rather than to
//! the focused window, and its candidate window is drawn under the text cursor.
//! Keys it does not use come back through a virtual keyboard, read with the
//! seat's keymap like real ones, which is what such input methods send.
//! Virtual pointers (wayvnc, KDE Connect) move and click like a mouse.

use luminaria::backend::{InputEvent, Modifiers, PointerAxis};
use luminaria::desktop::{VirtualKeyEvent, VirtualModifiersEvent, VirtualPointerEvent};
use luminaria::shell::SceneItem;
use luminaria::util::{KeymapState, Keysym, Rect};

use crate::state::State;

/// Axis motion of a virtual pointer, delivered at its `frame`.
#[derive(Default)]
pub struct VirtualAxis {
    dx: f64,
    dy: f64,
}

impl State<'_> {
    fn millis(&self) -> u32 {
        (self.now() * 1000.0) as u32
    }

    /// Where a key goes once Pastel's bindings passed on it: an input method
    /// holding the grab, or the focused window.
    pub(crate) fn route_key(&mut self, event: &InputEvent) {
        let grab = self
            .services
            .input_method
            .current()
            .filter(|im| im.has_keyboard_grab());
        match (grab, event) {
            (Some(im), InputEvent::Key { key, pressed, .. }) => {
                im.grab_send_key(self.millis(), *key, *pressed);
            }
            (Some(im), InputEvent::Modifiers(m)) => {
                im.grab_send_modifiers(m.depressed, m.latched, m.locked, m.group);
                self.services.router.route(event);
            }
            _ => self.services.router.route(event),
        }
    }

    /// A key an input method or another tool typed: straight to the focused window.
    pub(crate) fn virtual_key(&mut self, event: VirtualKeyEvent) {
        if self.keyboard.base.is_none() {
            self.keyboard.base =
                KeymapState::from_text(&self.services.globals.seat().keymap()).ok();
        }
        let keysym = self
            .keyboard
            .base
            .as_ref()
            .map_or(Keysym::new(0), |k| Keysym::new(k.keysym(event.key)));
        self.services.router.route(&InputEvent::Key {
            key: event.key,
            pressed: event.pressed,
            keysym,
            modifiers: self.keyboard.virtual_mods,
        });
    }

    pub(crate) fn virtual_modifiers(&mut self, event: VirtualModifiersEvent) {
        let modifiers = Modifiers {
            depressed: event.depressed,
            latched: event.latched,
            locked: event.locked,
            group: event.group,
        };
        self.keyboard.virtual_mods = modifiers;
        self.services
            .router
            .route(&InputEvent::Modifiers(modifiers));
    }

    /// A virtual pointer acts like a mouse, bindings included.
    pub(crate) fn virtual_pointer(&mut self, event: VirtualPointerEvent) {
        match event {
            VirtualPointerEvent::Motion { dx, dy, .. } => self.input(InputEvent::Motion { dx, dy }),
            VirtualPointerEvent::MotionAbsolute {
                x,
                y,
                x_extent,
                y_extent,
                ..
            } => {
                // The extent is the whole layout: what wayvnc and friends send.
                let monitors = self.desktop.monitors();
                let (mut left, mut top, mut right, mut bottom) =
                    (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for m in monitors {
                    left = left.min(m.rect.x);
                    top = top.min(m.rect.y);
                    right = right.max(m.rect.x + m.rect.w);
                    bottom = bottom.max(m.rect.y + m.rect.h);
                }
                if monitors.is_empty() || x_extent == 0 || y_extent == 0 {
                    return;
                }
                let x = left + f64::from(x) / f64::from(x_extent) * (right - left);
                let y = top + f64::from(y) / f64::from(y_extent) * (bottom - top);
                self.input(InputEvent::MotionAbsolute { x, y, output: None });
            }
            VirtualPointerEvent::Button {
                button, pressed, ..
            } => {
                self.input(InputEvent::Button { button, pressed });
            }
            VirtualPointerEvent::Axis { axis, value, .. } => match axis {
                0 => self.keyboard.virtual_axis.dy += value,
                _ => self.keyboard.virtual_axis.dx += value,
            },
            VirtualPointerEvent::Frame { .. } => {
                let VirtualAxis { dx, dy } = std::mem::take(&mut self.keyboard.virtual_axis);
                if dx != 0.0 || dy != 0.0 {
                    self.input(InputEvent::Axis(PointerAxis {
                        dx,
                        dy,
                        ..PointerAxis::default()
                    }));
                }
            }
        }
    }

    /// The input method's candidate windows, under the text cursor of the
    /// focused field, or above it when there is no room below.
    pub(crate) fn input_method_popups(&self, scene: &mut Vec<SceneItem>) {
        let Some(im) = self.services.input_method.current() else {
            return;
        };
        let popups = im.popup_surfaces();
        if popups.is_empty() {
            return;
        }
        let texts = self.services.text_input;
        let field = texts
            .focused()
            .and_then(|id| texts.text_input(id))
            .and_then(|t| t.state().focused_surface);
        let Some(field) = field.and_then(|f| self.services.screens.surface_box(f)) else {
            return;
        };
        for popup in popups {
            let (w, h) = (popup.surface_width(), popup.surface_height());
            if w <= 0 || h <= 0 {
                continue;
            }
            let cursor = self.services.input_method.popup_text_input_rectangle(popup);
            let x = field.x + cursor.x;
            let below = field.y + cursor.y + cursor.height;
            let monitor = self
                .desktop
                .monitors()
                .iter()
                .find(|m| m.rect.contains(f64::from(x), f64::from(below)));
            let bottom = monitor.map_or(i32::MAX, |m| (m.rect.y + m.rect.h) as i32);
            let right = monitor.map_or(i32::MAX, |m| (m.rect.x + m.rect.w) as i32);
            let y = if below + h > bottom {
                field.y + cursor.y - h
            } else {
                below
            };
            let x = x.min(right - w);
            let rect = Rect {
                x,
                y,
                width: w,
                height: h,
            };
            scene.push(SceneItem::surface(popup, rect, x, y));
        }
    }
}
