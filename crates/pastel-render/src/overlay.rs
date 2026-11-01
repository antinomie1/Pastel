//! Things drawn above everything, which take the keyboard while they're up.

use luminaria::gpu::SceneRenderer;
use luminaria::shell::SceneItem;
use luminaria::util::Keysym;
use pastel_core::Rect;

/// What an overlay gets to draw with.
pub struct OverlayContext<'a> {
    pub renderer: &'a SceneRenderer,
    /// The focused monitor, in layout coordinates.
    pub monitor: Rect,
    pub now: f64,
}

/// The extension point for full-screen UI: a screenshot picker, an exit
/// confirmation, a window switcher.
pub trait Overlay {
    /// While true, this overlay gets every key press instead of bindings and
    /// clients. An overlay animating out is drawn but no longer modal.
    fn is_modal(&self) -> bool;

    /// A key was pressed. Modifier keys never reach this.
    fn on_key(&mut self, keysym: Keysym, now: f64);

    /// Push this frame's items. True while animating.
    fn draw(&mut self, context: &OverlayContext<'_>, items: &mut Vec<SceneItem>) -> bool;
}
