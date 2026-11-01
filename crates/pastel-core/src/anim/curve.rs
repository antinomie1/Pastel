//! Curves: how a value gets from 0 to 1. Normalised, so one curve serves any
//! distance, and closed-form, so an animation is a pure function of time and the
//! frame rate cannot change what it looks like.

/// A point on a curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Progress: 0 at the start, 1 at rest. May overshoot.
    pub value: f64,
    /// Progress per second.
    pub velocity: f64,
    /// At rest for good. `value` is 1 from here on.
    pub done: bool,
}

/// The extension point for animation feel.
pub trait Curve {
    /// Where the curve is `t` seconds in, having started with velocity `v0`
    /// (progress per second). Curves that have no use for `v0` may ignore it.
    fn sample(&self, t: f64, v0: f64) -> Sample;
}

/// Done before it starts. What `animations = false` is made of.
#[derive(Debug, Clone, Copy, Default)]
pub struct Snap;

impl Curve for Snap {
    fn sample(&self, _t: f64, _v0: f64) -> Sample {
        Sample {
            value: 1.0,
            velocity: 0.0,
            done: true,
        }
    }
}

/// A damped spring. Interruptible: a retarget carries velocity across, which is
/// what makes rapid key presses feel continuous rather than jerky.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Higher is faster.
    pub stiffness: f64,
    /// 1 is critically damped; below 1 overshoots.
    pub damping: f64,
}

/// Rest thresholds. Relative to the distance travelled: 1/2000 of a 2000px move
/// lands within a pixel.
const REST_DISTANCE: f64 = 5e-4;
const REST_VELOCITY: f64 = 1e-2;

impl Curve for Spring {
    fn sample(&self, t: f64, v0: f64) -> Sample {
        // In terms of the displacement from rest: d(0) = -1, d'(0) = v0.
        let w0 = self.stiffness.max(1e-6).sqrt();
        let z = self.damping.max(0.0);
        let (d, v) = if (z - 1.0).abs() < 1e-6 {
            let b = v0 - w0;
            let e = (-w0 * t).exp();
            ((b * t - 1.0) * e, (b - w0 * (b * t - 1.0)) * e)
        } else if z < 1.0 {
            let wd = w0 * (1.0 - z * z).sqrt();
            let b = (v0 - z * w0) / wd;
            let e = (-z * w0 * t).exp();
            let (s, c) = (wd * t).sin_cos();
            let d = e * (-c + b * s);
            (d, -z * w0 * d + e * (wd * s + b * wd * c))
        } else {
            let root = (z * z - 1.0).sqrt();
            let (r1, r2) = (-w0 * (z - root), -w0 * (z + root));
            let c2 = (v0 + r1) / (r2 - r1);
            let c1 = -1.0 - c2;
            let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
            (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        };
        let done = d.abs() < REST_DISTANCE && v.abs() < REST_VELOCITY;
        if done {
            Sample {
                value: 1.0,
                velocity: 0.0,
                done,
            }
        } else {
            Sample {
                value: 1.0 + d,
                velocity: v,
                done,
            }
        }
    }
}

/// The shape of an [`Ease`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    OutQuad,
    OutCubic,
    OutQuint,
    OutExpo,
    InOutCubic,
}

impl Easing {
    pub fn apply(self, x: f64) -> f64 {
        match self {
            Self::Linear => x,
            Self::OutQuad => 1.0 - (1.0 - x).powi(2),
            Self::OutCubic => 1.0 - (1.0 - x).powi(3),
            Self::OutQuint => 1.0 - (1.0 - x).powi(5),
            Self::OutExpo if x >= 1.0 => 1.0,
            Self::OutExpo => 1.0 - 2f64.powf(-10.0 * x),
            Self::InOutCubic if x < 0.5 => 4.0 * x * x * x,
            Self::InOutCubic => 1.0 - (-2.0 * x + 2.0).powi(3) / 2.0,
        }
    }
}

/// A fixed-duration ease. Ignores `v0`: it always takes `seconds`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ease {
    pub seconds: f64,
    pub easing: Easing,
}

impl Curve for Ease {
    fn sample(&self, t: f64, _v0: f64) -> Sample {
        if self.seconds <= 0.0 || t >= self.seconds {
            return Sample {
                value: 1.0,
                velocity: 0.0,
                done: true,
            };
        }
        let x = t / self.seconds;
        let step = 1e-4;
        let velocity =
            (self.easing.apply((x + step).min(1.0)) - self.easing.apply(x)) / step / self.seconds;
        Sample {
            value: self.easing.apply(x),
            velocity,
            done: false,
        }
    }
}

/// Any curve, slower by `factor`: the global animation-speed setting.
pub struct Slowed<C> {
    pub inner: C,
    pub factor: f64,
}

impl<C: Curve> Curve for Slowed<C> {
    fn sample(&self, t: f64, v0: f64) -> Sample {
        let f = self.factor.max(1e-3);
        let s = self.inner.sample(t / f, v0 * f);
        Sample {
            velocity: s.velocity / f,
            ..s
        }
    }
}
