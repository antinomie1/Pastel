//! What a three-finger swipe means: the view follows the fingers sideways, and a
//! push up or down at the end changes workspace. Only arithmetic: distances come
//! in, layout commands go out.

use crate::{LayoutOp, WorkspaceTarget};

/// Fingers that have moved less than this haven't said which way they're going.
const DECIDE_AFTER: f64 = 16.0;
/// How far up or down counts as asking for another workspace.
const WORKSPACE_AFTER: f64 = 60.0;
/// View pixels per pixel of touchpad.
const SCROLL_SPEED: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Sideways,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Swipe {
    dx: f64,
    dy: f64,
    axis: Option<Axis>,
}

impl Swipe {
    /// The fingers moved. A sideways swipe pans as it goes.
    pub fn update(&mut self, dx: f64, dy: f64) -> Option<LayoutOp> {
        self.dx += dx;
        self.dy += dy;
        if self.axis.is_none() && self.dx.hypot(self.dy) >= DECIDE_AFTER {
            let sideways = self.dx.abs() >= self.dy.abs();
            self.axis = Some(if sideways {
                Axis::Sideways
            } else {
                Axis::Vertical
            });
            // What was held back until now shouldn't be lost to the threshold.
            return sideways.then_some(LayoutOp::Scroll(-self.dx * SCROLL_SPEED));
        }
        (self.axis == Some(Axis::Sideways)).then_some(LayoutOp::Scroll(-dx * SCROLL_SPEED))
    }

    /// The fingers lifted. Content follows them, so pushing up brings up what's
    /// below. `cancelled` means they were doing something else after all.
    pub fn finish(&self, cancelled: bool) -> Option<WorkspaceTarget> {
        if cancelled || self.axis != Some(Axis::Vertical) || self.dy.abs() < WORKSPACE_AFTER {
            return None;
        }
        Some(if self.dy < 0.0 {
            WorkspaceTarget::Down
        } else {
            WorkspaceTarget::Up
        })
    }
}
