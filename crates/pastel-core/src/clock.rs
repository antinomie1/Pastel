//! When the next frame will reach the screen. Animations sampled at the moment
//! they're drawn run a frame behind and judder with how long drawing took; sampled
//! at the moment they'll be seen, they move evenly.

/// Follows `Present` events. All times are seconds on one clock.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameClock {
    /// When the last frame reached the screen, and how often they do.
    last: Option<(f64, f64)>,
}

impl FrameClock {
    /// A frame reached the screen at `at`. `refresh` is 0 where nobody knows.
    pub fn presented(&mut self, at: f64, refresh: f64) {
        self.last = (refresh > 0.0).then_some((at, refresh));
    }

    /// The first vblank after `now`, or `now` itself with nothing to go on.
    pub fn next_frame(&self, now: f64) -> f64 {
        let Some((at, refresh)) = self.last else {
            return now;
        };
        let frames = ((now - at) / refresh).floor() + 1.0;
        at + frames.max(1.0) * refresh
    }
}
