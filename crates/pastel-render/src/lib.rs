//! Everything between "where should windows be" and the scene list: animation
//! state, window effects and overlays. The pure crates decide; this one turns the
//! decisions into luminaria [`SceneItem`]s.

mod compose;
mod effect;
mod hotkey;
mod overlay;

pub use compose::{Composer, Host};
pub use effect::{Effect, Pipeline, WindowFrame, color};
pub use hotkey::HotkeyOverlay;
pub use luminaria::shell::SceneItem;
pub use overlay::{Overlay, OverlayContext};

use luminaria::util::Rect as PixelRect;
use pastel_core::Rect;

/// Snap a logical rect to whole pixels. Edges are rounded rather than the size, so
/// two rects that touch keep touching mid-animation.
pub fn to_pixels(rect: Rect) -> PixelRect {
    let (x0, y0) = (rect.x.round(), rect.y.round());
    let (x1, y1) = (rect.right().round(), rect.bottom().round());
    PixelRect::new(x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32)
}
