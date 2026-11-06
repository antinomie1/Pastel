//! Drag and drop between clients: a browser tab dragged to another window or
//! out of its own, a file dragged from a file manager into a terminal.
//! luminaria runs the protocol; what is Pastel's is the icon the dragging
//! client draws, which follows the pointer above everything else.

use luminaria::shell::SceneItem;
use luminaria::util::Rect;

use crate::state::State;

impl State<'_> {
    /// A drag is running: the pointer belongs to it, not to focus.
    pub(crate) fn dragging(&self) -> bool {
        self.services.globals.data_device().dragging()
    }

    /// The pointer moved or a button changed: an icon on screen follows, and
    /// one whose drag just ended goes.
    pub(crate) fn drag_moved(&mut self) {
        let icon = self.services.globals.data_device().drag_icon();
        if icon.is_some() || self.drag_icon.is_some() {
            self.dirty = true;
        }
    }

    /// The drag icon under the pointer, offset as the client asked, topmost.
    pub(crate) fn drag_icon_item(&mut self, scene: &mut Vec<SceneItem>) {
        let icon = self.services.globals.data_device().drag_icon();
        if let Some(new) = icon.filter(|i| Some(*i) != self.drag_icon) {
            self.tell_scale(new);
        }
        self.drag_icon = icon;
        let (Some(icon), Some((x, y))) = (icon, self.services.cursor.position()) else {
            return;
        };
        let (w, h) = (icon.surface_width(), icon.surface_height());
        if w <= 0 || h <= 0 {
            return;
        }
        let (dx, dy) = icon.offset();
        let (x, y) = (x.floor() as i32 + dx, y.floor() as i32 + dy);
        let rect = Rect::new(x, y, w, h);
        scene.push(SceneItem::surface(icon, rect, x, y).with_accepts_input(false));
    }
}
