//! Animation: a value that heads towards a target along a [`Curve`].
//!
//! Time is always a parameter, `now`, in seconds from any fixed origin; nothing
//! here reads a clock, so a test can step through an animation exactly.

mod curve;

use std::rc::Rc;

pub use curve::{Curve, Ease, Easing, Sample, Slowed, Snap, Spring};

use crate::geom::Rect;

/// A number on its way somewhere. Retargeting mid-flight starts from where it is,
/// at the speed it is going.
#[derive(Clone)]
pub struct Anim {
    from: f64,
    to: f64,
    start: f64,
    /// Velocity at `start`, in progress per second.
    v0: f64,
    curve: Rc<dyn Curve>,
}

impl Anim {
    /// At rest at `value`.
    pub fn new(value: f64, curve: Rc<dyn Curve>) -> Self {
        Self { from: value, to: value, start: 0.0, v0: 0.0, curve }
    }

    fn sample(&self, now: f64) -> Sample {
        if self.from == self.to {
            return Sample { value: 1.0, velocity: 0.0, done: true };
        }
        self.curve.sample((now - self.start).max(0.0), self.v0)
    }

    pub fn value(&self, now: f64) -> f64 {
        self.from + (self.to - self.from) * self.sample(now).value
    }

    /// In units per second.
    pub fn velocity(&self, now: f64) -> f64 {
        (self.to - self.from) * self.sample(now).velocity
    }

    pub fn target(&self) -> f64 {
        self.to
    }

    pub fn is_done(&self, now: f64) -> bool {
        self.sample(now).done
    }

    /// Head for `to` from wherever this is now. Asking for the current target is
    /// free, so callers can set it every frame.
    pub fn set(&mut self, to: f64, now: f64) {
        if to == self.to {
            return;
        }
        let (value, velocity) = (self.value(now), self.velocity(now));
        self.from = value;
        self.to = to;
        self.start = now;
        // Rescale to the new distance: velocity is in progress per second.
        self.v0 = if value == to { 0.0 } else { velocity / (to - value) };
    }

    /// Replay from `from` to the current target.
    pub fn restart_from(&mut self, from: f64, now: f64) {
        self.from = from;
        self.start = now;
        self.v0 = 0.0;
    }

    /// Be at `value` with no animation.
    pub fn snap(&mut self, value: f64) {
        self.from = value;
        self.to = value;
        self.v0 = 0.0;
    }

    /// Move the whole animation by `delta`, for when what it is relative to moved.
    pub fn shift(&mut self, delta: f64) {
        self.from += delta;
        self.to += delta;
    }

    pub fn set_curve(&mut self, curve: Rc<dyn Curve>) {
        self.curve = curve;
    }

    /// Settle if finished. True while still moving.
    pub fn tick(&mut self, now: f64) -> bool {
        let moving = !self.is_done(now);
        if !moving {
            self.from = self.to;
        }
        moving
    }
}

/// A rectangle, each edge of which is an [`Anim`] on the same curve.
#[derive(Clone)]
pub struct AnimRect {
    x: Anim,
    y: Anim,
    w: Anim,
    h: Anim,
}

impl AnimRect {
    pub fn new(rect: Rect, curve: Rc<dyn Curve>) -> Self {
        Self {
            x: Anim::new(rect.x, curve.clone()),
            y: Anim::new(rect.y, curve.clone()),
            w: Anim::new(rect.w, curve.clone()),
            h: Anim::new(rect.h, curve),
        }
    }

    pub fn value(&self, now: f64) -> Rect {
        Rect::new(self.x.value(now), self.y.value(now), self.w.value(now), self.h.value(now))
    }

    pub fn target(&self) -> Rect {
        Rect::new(self.x.target(), self.y.target(), self.w.target(), self.h.target())
    }

    pub fn set(&mut self, rect: Rect, now: f64) {
        self.x.set(rect.x, now);
        self.y.set(rect.y, now);
        self.w.set(rect.w, now);
        self.h.set(rect.h, now);
    }

    pub fn snap(&mut self, rect: Rect) {
        self.x.snap(rect.x);
        self.y.snap(rect.y);
        self.w.snap(rect.w);
        self.h.snap(rect.h);
    }

    pub fn shift(&mut self, dx: f64, dy: f64) {
        self.x.shift(dx);
        self.y.shift(dy);
    }

    pub fn set_curve(&mut self, curve: Rc<dyn Curve>) {
        self.x.set_curve(curve.clone());
        self.y.set_curve(curve.clone());
        self.w.set_curve(curve.clone());
        self.h.set_curve(curve);
    }

    pub fn tick(&mut self, now: f64) -> bool {
        // Not `||`: every axis has to settle.
        self.x.tick(now) | self.y.tick(now) | self.w.tick(now) | self.h.tick(now)
    }
}
