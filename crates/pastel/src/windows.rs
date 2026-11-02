//! What Pastel remembers about each client window, and the [`Host`] view of it.

use std::collections::HashMap;

use luminaria::backend::Output;
use luminaria::protocol::{Layer, SurfaceId, XdgShell};
use luminaria::shell::{LayerManager, PopupTree, SceneItem};
use pastel_config::ResolvedRule;
use pastel_layout::WindowId;
use pastel_render::Host;

pub fn id_of(surface: SurfaceId) -> WindowId {
    WindowId(surface.0)
}

pub fn surface_of(id: WindowId) -> SurfaceId {
    SurfaceId(id.0)
}

pub struct Window {
    pub rule: ResolvedRule,
    pub mapped: bool,
    /// What was last asked of the client, to send only changes.
    pub configured: Option<(i32, i32, bool, bool)>,
    /// What it was last told about its monitor's scale; 0 until then.
    pub scale: i32,
}

#[derive(Default)]
pub struct Windows {
    map: HashMap<WindowId, Window>,
}

impl Windows {
    pub fn insert(&mut self, id: WindowId, rule: ResolvedRule) {
        self.map.insert(
            id,
            Window {
                rule,
                mapped: false,
                configured: None,
                scale: 0,
            },
        );
    }

    pub fn remove(&mut self, id: WindowId) -> Option<Window> {
        self.map.remove(&id)
    }

    pub fn get(&self, id: WindowId) -> Option<&Window> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.map.get_mut(&id)
    }

    pub fn mapped(&self) -> usize {
        self.map.values().filter(|w| w.mapped).count()
    }

    pub fn contains(&self, id: WindowId) -> bool {
        self.map.contains_key(&id)
    }
}

/// Everything the composer needs from the client side, borrowed for one frame.
pub struct SceneHost<'a, 's> {
    pub windows: &'a Windows,
    pub shell: &'a XdgShell<'s>,
    pub layers: &'a LayerManager,
    pub popups: &'a PopupTree,
    pub outputs: &'a [Output],
}

impl Host for SceneHost<'_, '_> {
    fn is_mapped(&self, id: WindowId) -> bool {
        self.windows.get(id).is_some_and(|w| w.mapped)
    }

    fn window(&self, id: WindowId, x: i32, y: i32) -> Option<SceneItem> {
        self.shell
            .toplevel(surface_of(id))
            .map(|t| SceneItem::toplevel(&t, x, y))
    }

    fn crop(&self, id: WindowId) -> (f32, f32, f32, f32) {
        let Some(toplevel) = self.shell.toplevel(surface_of(id)) else {
            return (0.0, 0.0, 1.0, 1.0);
        };
        let window = toplevel.geometry();
        let surface = toplevel.surface();
        let (width, height) = (surface.surface_width(), surface.surface_height());
        if window.width <= 0 || window.height <= 0 || width <= 0 || height <= 0 {
            return (0.0, 0.0, 1.0, 1.0);
        }
        let (w, h) = (width as f32, height as f32);
        (
            window.x as f32 / w,
            window.y as f32 / h,
            window.width as f32 / w,
            window.height as f32 / h,
        )
    }

    fn output_key(&self, monitor: u64) -> u64 {
        let output = self.outputs.iter().find(|o| o.id().0 == monitor);
        output.map_or(0, Output::key)
    }

    fn rule(&self, id: WindowId) -> ResolvedRule {
        self.windows.get(id).map(|w| w.rule).unwrap_or_default()
    }

    fn layers(&self, monitor: u64, layer: Layer, items: &mut Vec<SceneItem>) {
        self.layers.append_to(items, monitor as u32, layer);
    }

    fn popups(&self, items: &mut Vec<SceneItem>) {
        self.popups.append_to(items);
    }
}
