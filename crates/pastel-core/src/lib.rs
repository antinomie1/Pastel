//! The leaves of Pastel: logical geometry and animation. Depends on nothing.

pub mod anim;
pub mod geom;

pub use anim::{Anim, AnimRect, Curve, Ease, Easing, Sample, Slowed, Snap, Spring};
pub use geom::{Direction, Rect};
