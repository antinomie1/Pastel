//! A window following the pointer. Only arithmetic: where the pointer is comes in,
//! where the window should be goes out.

use pastel_core::Rect;

use crate::WindowId;

/// Which sides of a window a resize is holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    /// The corner of `rect` nearest a point, for a resize with no edge to go by.
    pub fn nearest(rect: Rect, x: f64, y: f64) -> Self {
        let (cx, cy) = rect.center();
        Self {
            left: x < cx,
            right: x >= cx,
            top: y < cy,
            bottom: y >= cy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrabKind {
    Move,
    Resize(Edges),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grab {
    pub id: WindowId,
    pub kind: GrabKind,
    /// The pointer when it started.
    pub start: (f64, f64),
    /// The window when it started.
    pub rect: Rect,
}

impl Grab {
    /// Where the window goes with the pointer at (`x`, `y`). `min` and `max` are the
    /// client's limits, with a `max` of 0 for none. The held side moves and the
    /// opposite one stays, even when a limit stops the size.
    pub fn rect_at(&self, x: f64, y: f64, min: (f64, f64), max: (f64, f64)) -> Rect {
        let (dx, dy) = (x - self.start.0, y - self.start.1);
        let Self { rect, .. } = *self;
        let GrabKind::Resize(edges) = self.kind else {
            return rect.translated(dx, dy);
        };
        let span = |base: f64, delta: f64, near: bool, far: bool, min: f64, max: f64| {
            let wanted = match (near, far) {
                (true, _) => base - delta,
                (_, true) => base + delta,
                _ => base,
            };
            let upper = if max > 0.0 { max } else { f64::MAX };
            wanted.clamp(min, upper.max(min))
        };
        let w = span(rect.w, dx, edges.left, edges.right, min.0, max.0);
        let h = span(rect.h, dy, edges.top, edges.bottom, min.1, max.1);
        let x = if edges.left { rect.right() - w } else { rect.x };
        let y = if edges.top { rect.bottom() - h } else { rect.y };
        Rect::new(x, y, w, h)
    }
}
