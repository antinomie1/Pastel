//! Logical-pixel geometry. `f64` because animated positions are fractional; the
//! shell rounds at the last moment, when a rect becomes a scene item.

/// A rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self { x: self.x + dx, y: self.y + dy, ..*self }
    }

    /// Grown by `by` on every side; negative shrinks. Never inverts.
    pub fn inflated(&self, by: f64) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            w: (self.w + 2.0 * by).max(0.0),
            h: (self.h + 2.0 * by).max(0.0),
        }
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then_some(Rect::new(x, y, r - x, b - y))
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// One of the four screen directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
