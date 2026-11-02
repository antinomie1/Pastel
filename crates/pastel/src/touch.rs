//! Touchscreens and touchpad gestures, neither of which go through the router.

use std::collections::HashMap;

use luminaria::backend::{GestureEvent, GestureKind, GesturePhase, InputEvent, Output};
use luminaria::protocol::SurfaceId;
use pastel_layout::Swipe;

use crate::state::State;

/// How many fingers swipe at the desktop, leaving others to the client.
const DESKTOP_FINGERS: u32 = 3;
/// Connectors of a laptop's own panel, the likeliest home of a touchscreen.
const BUILT_IN: [&str; 3] = ["eDP", "LVDS", "DSI"];

#[derive(Default)]
pub struct TouchState {
    /// Each finger down on a surface: where the surface's origin was.
    origins: HashMap<i32, (f64, f64)>,
    swipe: Option<Swipe>,
    /// The client a gesture started over, which sees all of it.
    gesture_target: Option<SurfaceId>,
    serial: u32,
}

impl State<'_> {
    /// The monitor the panel is in front of, which the event doesn't say.
    fn touch_output(&self) -> Option<&Output> {
        let screens = self.services.screens;
        let on_desktop = |o: &&Output| self.desktop.monitor(o.id().0).is_some();
        let named = |wanted: &dyn Fn(&str) -> bool| {
            let mut enabled = self.outputs.iter().filter(on_desktop);
            enabled.find(|o| wanted(&screens.output_name(o)))
        };
        let configured = self.config.input.touch_output.as_deref();
        let focused = self.desktop.focused_monitor().map(|m| m.id);
        configured
            .and_then(|want| named(&|name| name == want))
            .or_else(|| named(&|name| BUILT_IN.iter().any(|p| name.starts_with(p))))
            .or_else(|| self.outputs.iter().find(|o| Some(o.id().0) == focused))
    }

    /// From the panel's 0 to 1, which rotate with it, onto the monitor.
    fn touch_point(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let output = self.touch_output()?;
        let rect = self.desktop.monitor(output.id().0)?.rect;
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        let (x, y) = output.transform().unit_point(x, y);
        Some((rect.x + x * rect.w, rect.y + y * rect.h))
    }

    /// True if `event` was touch, which is finished with here.
    pub(crate) fn touch(&mut self, event: &InputEvent) -> bool {
        let seat = self.services.globals.seat();
        match *event {
            InputEvent::TouchDown { id, x, y } => {
                let Some((x, y)) = self.touch_point(x, y) else {
                    return true;
                };
                let hit = self.hit_at(x, y);
                if let Some(surface) = hit.surface {
                    self.touching.origins.insert(id, (x - hit.sx, y - hit.sy));
                    seat.touch_down(surface, id, hit.sx, hit.sy);
                    self.click(hit.tag, hit.surface); // what a tap means, as a click
                }
            }
            InputEvent::TouchMotion { id, x, y } => {
                let origin = self.touching.origins.get(&id).copied();
                if let (Some((ox, oy)), Some((x, y))) = (origin, self.touch_point(x, y)) {
                    seat.touch_motion(id, x - ox, y - oy);
                }
            }
            InputEvent::TouchUp { id } => {
                if self.touching.origins.remove(&id).is_some() {
                    seat.touch_up(id);
                }
            }
            InputEvent::TouchFrame => seat.touch_frame(),
            InputEvent::TouchCancel { id } => {
                if self.touching.origins.remove(&id).is_some() {
                    seat.touch_cancel();
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn gesture(&mut self, event: GestureEvent) {
        let ours = event.kind == GestureKind::Swipe
            && (self.touching.swipe.is_some()
                || (event.phase == GesturePhase::Begin
                    && event.fingers == DESKTOP_FINGERS
                    && !self.is_locked()));
        if ours {
            self.desktop_swipe(event);
        } else {
            self.forward_gesture(event);
        }
    }

    fn desktop_swipe(&mut self, event: GestureEvent) {
        match event.phase {
            GesturePhase::Begin => self.touching.swipe = Some(Swipe::default()),
            GesturePhase::Update => {
                let op = self
                    .touching
                    .swipe
                    .as_mut()
                    .and_then(|swipe| swipe.update(event.dx, event.dy));
                if let Some(op) = op {
                    self.desktop.apply(op);
                    self.dirty = true;
                }
            }
            GesturePhase::End => {
                let target = self.touching.swipe.take();
                let target = target.and_then(|swipe| swipe.finish(event.cancelled));
                if let Some(target) = target {
                    self.desktop.focus_workspace(target);
                    self.dirty = true;
                }
            }
        }
    }

    fn forward_gesture(&mut self, event: GestureEvent) {
        let time = (self.now() * 1000.0) as u32;
        if event.phase == GesturePhase::Begin {
            let hit = self
                .services
                .cursor
                .position()
                .map(|(x, y)| self.hit_at(x, y));
            self.touching.gesture_target = hit.and_then(|hit| hit.surface);
            self.touching.serial = self.touching.serial.wrapping_add(1);
        }
        let Some(surface) = self.touching.gesture_target else {
            return;
        };
        let serial = self.touching.serial;
        let gestures = self.services.gestures;
        match (event.kind, event.phase) {
            (GestureKind::Swipe, GesturePhase::Begin) => {
                gestures.send_swipe_begin(surface, serial, time, event.fingers);
            }
            (GestureKind::Swipe, GesturePhase::Update) => {
                gestures.send_swipe_update(time, event.dx, event.dy);
            }
            (GestureKind::Swipe, GesturePhase::End) => {
                gestures.send_swipe_end(serial, time, event.cancelled);
            }
            (GestureKind::Pinch, GesturePhase::Begin) => {
                gestures.send_pinch_begin(surface, serial, time, event.fingers);
            }
            (GestureKind::Pinch, GesturePhase::Update) => {
                let (dx, dy, scale, rotation) = (event.dx, event.dy, event.scale, event.rotation);
                gestures.send_pinch_update(time, dx, dy, scale, rotation);
            }
            (GestureKind::Pinch, GesturePhase::End) => {
                gestures.send_pinch_end(serial, time, event.cancelled);
            }
            (GestureKind::Hold, GesturePhase::Begin) => {
                gestures.send_hold_begin(surface, serial, time, event.fingers);
            }
            (GestureKind::Hold, _) => gestures.send_hold_end(serial, time, event.cancelled),
        }
        if event.phase == GesturePhase::End {
            self.touching.gesture_target = None;
        }
    }
}
