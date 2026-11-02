//! A window following the pointer: a client's title bar, or Mod and a mouse button.
//! The arithmetic is `pastel_layout::Grab`; this is when it starts and stops.

use luminaria::protocol::SurfaceId;
use luminaria::util::edge;
use pastel_layout::{Edges, Grab, GrabKind, LayoutOp, WindowId};

use crate::state::State;
use crate::windows::surface_of;

/// However small a client says it can go, a window stays big enough to take hold of.
const MIN_SIZE: f64 = 50.0;

/// What a drag is doing.
pub enum Held {
    Window(Grab),
    /// A tiled window's side, which is the column's width.
    Column {
        x: f64,
        left: bool,
    },
}

fn edges(bits: u32) -> Edges {
    Edges {
        left: bits & edge::LEFT != 0,
        right: bits & edge::RIGHT != 0,
        top: bits & edge::TOP != 0,
        bottom: bits & edge::BOTTOM != 0,
    }
}

impl State<'_> {
    pub fn is_grabbing(&self) -> bool {
        self.held.is_some()
    }

    /// A client's asking is honoured only with a button down: it comes after a
    /// click on its title bar, and otherwise a window could chase the pointer.
    pub(crate) fn may_grab(&self, id: WindowId) -> bool {
        self.keyboard.buttons_held() > 0
            && self.held.is_none()
            && !self.is_locked()
            && !self.desktop.is_fullscreen(id)
    }

    /// Tiled windows float away, as nothing else would mean what a drag means.
    pub(crate) fn begin_move(&mut self, id: WindowId) {
        let Some((position, rect)) = self.grab_start(id) else {
            return;
        };
        self.desktop.set_floating(id, true);
        self.desktop.set_floating_rect(id, rect);
        self.hold(Grab {
            id,
            kind: GrabKind::Move,
            start: position,
            rect,
        });
    }

    pub(crate) fn begin_resize(&mut self, id: WindowId, bits: u32) {
        let edges = edges(bits);
        self.begin_resize_at(id, edges);
    }

    pub(crate) fn begin_resize_at(&mut self, id: WindowId, edges: Edges) {
        let Some((position, rect)) = self.grab_start(id) else {
            return;
        };
        if self.desktop.floating(id).is_some() {
            if let Some(toplevel) = self.services.shell().toplevel(surface_of(id)) {
                toplevel.set_resizing(true);
            }
            self.hold(Grab {
                id,
                kind: GrabKind::Resize(edges),
                start: position,
                rect,
            });
        } else if edges.left || edges.right {
            self.desktop.focus_window(id);
            self.held = Some(Held::Column {
                x: position.0,
                left: edges.left,
            });
        }
    }

    fn grab_start(&self, id: WindowId) -> Option<((f64, f64), pastel_core::Rect)> {
        Some((self.services.cursor.position()?, self.desktop.rect_of(id)?))
    }

    fn hold(&mut self, grab: Grab) {
        self.desktop.focus_window(grab.id);
        self.desktop.set_interactive(grab.id, true);
        self.held = Some(Held::Window(grab));
        self.dirty = true;
    }

    pub(crate) fn grab_moved(&mut self) {
        let Some((x, y)) = self.services.cursor.position() else {
            return;
        };
        match &mut self.held {
            Some(Held::Window(grab)) => {
                let grab = *grab;
                let (min, max) = self.size_limits(surface_of(grab.id));
                let rect = grab.rect_at(x, y, min, max);
                self.desktop.set_floating_rect(grab.id, rect);
            }
            Some(Held::Column { x: last, left }) => {
                let (delta, left) = (x - *last, *left);
                *last = x;
                if let Some(monitor) = self.desktop.focused_monitor() {
                    let share = delta / monitor.usable.w;
                    let share = if left { -share } else { share };
                    self.desktop.apply(LayoutOp::AdjustWidth(share));
                }
            }
            None => return,
        }
        self.dirty = true;
    }

    fn size_limits(&self, surface: SurfaceId) -> ((f64, f64), (f64, f64)) {
        let toplevel = self.services.shell().toplevel(surface);
        let (min, max) = toplevel.map_or(((0, 0), (0, 0)), |t| (t.min_size(), t.max_size()));
        let floor = |size: i32| f64::from(size).max(MIN_SIZE);
        (
            (floor(min.0), floor(min.1)),
            (f64::from(max.0), f64::from(max.1)),
        )
    }

    pub(crate) fn grab_ended(&mut self) {
        if let Some(Held::Window(grab)) = self.held.take() {
            self.desktop.set_interactive(grab.id, false);
            self.desktop.rehome_floating(grab.id);
            if let Some(toplevel) = self.services.shell().toplevel(surface_of(grab.id)) {
                toplevel.set_resizing(false);
            }
        }
        self.dirty = true;
    }
}
