//! Where windows go. A [`Layout`] is one workspace's arrangement; [`Desktop`] is
//! the monitors and workspaces those arrangements live in.
//!
//! Nothing here knows about time: a layout answers "where should windows be right
//! now", and `pastel-render` animates towards the answer. A new layout therefore
//! gets every animation without writing any.

mod desktop;
mod gesture;
mod grab;
mod scrolling;

pub use desktop::{Desktop, Floating, Monitor, Workspace, WorkspaceTarget};
pub use gesture::Swipe;
pub use grab::{Edges, Grab, GrabKind};
pub use pastel_core::{Direction, Rect};
pub use scrolling::Scrolling;

/// A window, as far as layouts care. Opaque; the shell uses the surface id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

/// A window and where it should be, in layout coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub id: WindowId,
    pub rect: Rect,
    /// Covers the whole monitor, above everything including top layer surfaces.
    pub fullscreen: bool,
    /// Out of the layout, above it.
    pub floating: bool,
    /// False for a floating window that hasn't said what size it wants: it's asked
    /// with a size of 0, and `rect` is only a guess.
    pub sized: bool,
    /// Under the pointer's control: follow it exactly, with no animation.
    pub interactive: bool,
}

/// What a window would like, before it is placed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowHint {
    /// Width as a fraction of the working area; `None` takes the layout's default.
    pub width: Option<f64>,
    /// Out of the layout from the start, as a dialog is.
    pub floating: bool,
}

/// A command to a layout. One enum for every layout, so bindings don't depend on
/// which is in use: a layout returns false for what it has no meaning for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutOp {
    Focus(Direction),
    /// Move the focused window, or in `Scrolling` its whole column on the
    /// horizontal axis.
    Move(Direction),
    FocusFirst,
    FocusLast,
    MoveFirst,
    MoveLast,
    /// Join the neighbouring column in this direction, or if sharing a column,
    /// leave it for a new one on that side.
    ConsumeOrExpel(Direction),
    /// Step through the preset widths.
    CycleWidth,
    /// Add to the width, as a fraction of the working area.
    AdjustWidth(f64),
    ToggleMaximize,
    Center,
    /// Pan the view without changing focus, in logical pixels.
    Scroll(f64),
}

/// One workspace's arrangement of windows. The extension point for tiling
/// behaviour: implement this, add it to [`create`], and `layout = "name"` works.
pub trait Layout {
    fn name(&self) -> &'static str;

    /// The rectangle to arrange inside, already excluding panels.
    fn set_area(&mut self, area: Rect);

    /// Add a window and focus it.
    fn add(&mut self, id: WindowId, hint: WindowHint);

    /// False if it was not here.
    fn remove(&mut self, id: WindowId) -> bool;

    fn contains(&self, id: WindowId) -> bool;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn focused(&self) -> Option<WindowId>;

    /// False if it is not here.
    fn focus(&mut self, id: WindowId) -> bool;

    /// False if nothing changed: an edge was hit, or the op means nothing here.
    fn apply(&mut self, op: LayoutOp) -> bool;

    /// Append where every window should be. Includes windows outside the area:
    /// a scrolled-away column still has a place. Keeps the caller's allocation.
    fn place(&self, out: &mut Vec<Placement>);
}

/// When the view scrolls to keep the focused column in sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CenterFocus {
    /// Scroll the minimum that brings it fully on screen.
    #[default]
    Never,
    /// Always centre it.
    Always,
    /// Centre it if it was off screen, otherwise leave the view alone.
    OnOverflow,
}

/// Everything layouts are configured with.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
    /// Which layout, by [`Layout::name`].
    pub kind: String,
    /// Between windows and around the edge, in logical pixels.
    pub gap: f64,
    /// Width of a new column, as a fraction of the working area.
    pub default_width: f64,
    /// What [`LayoutOp::CycleWidth`] steps through.
    pub preset_widths: Vec<f64>,
    pub center_focus: CenterFocus,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            kind: "scrolling".into(),
            gap: 12.0,
            default_width: 0.5,
            preset_widths: vec![1.0 / 3.0, 0.5, 2.0 / 3.0],
            center_focus: CenterFocus::Never,
        }
    }
}

/// Layout names [`create`] knows.
pub const LAYOUTS: &[&str] = &["scrolling"];

/// A new empty layout, or `None` if `options.kind` is not one of [`LAYOUTS`].
pub fn create(options: &LayoutOptions) -> Option<Box<dyn Layout>> {
    match options.kind.as_str() {
        "scrolling" => Some(Box::new(Scrolling::new(options.clone()))),
        _ => None,
    }
}
