//! Tablets and colour control: what luminaria's protocols deliver and Pastel
//! applies.
//!
//! Tablets map onto the monitor under the pen (the focused one when it is
//! outside every monitor), and the pen goes to the surface under it, like a
//! pointer. Night-light tools (wlr-gamma-control) and colour matrices
//! (hyprland-ctm-control) reach outputs with a gamma table or a CTM: DRM ones.
//! Headless and nested outputs have neither, so their clients hear `failed`.

use luminaria::desktop::{CtmEvent, GammaEvent};
use luminaria::protocol::TabletInput;
use luminaria::protocol::TabletTarget;
use luminaria::util::Rect;

use crate::state::State;

impl State<'_> {
    pub(crate) fn tablet(&mut self, input: &TabletInput) {
        // The monitor the pen is over, else the focused one.
        let monitors = self.desktop.monitors();
        let pen = match input {
            TabletInput::Axis { tool, .. }
            | TabletInput::Tip { tool, .. }
            | TabletInput::Proximity { tool, .. } => self.tablets.tool_position(*tool),
            _ => None,
        };
        let monitor = pen
            .and_then(|(x, y)| monitors.iter().find(|m| m.rect.contains(x, y)))
            .or_else(|| self.desktop.focused_monitor());
        let Some(monitor) = monitor else { return };
        let r = monitor.rect;
        let area = Rect {
            x: r.x as i32,
            y: r.y as i32,
            width: r.w as i32,
            height: r.h as i32,
        };
        let mut feed = std::mem::take(&mut self.tablets);
        feed.feed(self.services.tablets, input, area, |x, y| {
            let hit = self.hit_at(x, y);
            let surface = hit.surface.filter(|_| hit.hit)?;
            Some(TabletTarget {
                surface,
                x: hit.sx,
                y: hit.sy,
            })
        });
        self.tablets = feed;
    }

    /// Tells the gamma protocol how big each output's table is, once the
    /// output has a `wl_output` to be named by.
    pub(crate) fn sync_gamma_sizes(&mut self) {
        for output in &self.outputs {
            let identity = self.services.screens.output_identity(output);
            if identity != 0 && !self.gamma_sized.contains(&identity) {
                self.services
                    .gamma
                    .set_output_size(identity, output.gamma_size());
                self.gamma_sized.push(identity);
            }
        }
        let screens = self.services.screens;
        let outputs = &self.outputs;
        self.gamma_sized
            .retain(|id| outputs.iter().any(|o| screens.output_identity(o) == *id));
    }

    pub(crate) fn gamma(&mut self, event: GammaEvent) {
        let ramp = event
            .has_ramp
            .then(|| self.services.gamma.ramp(event.output_identity))
            .flatten();
        let screens = self.services.screens;
        let Some(output) = self
            .outputs
            .iter_mut()
            .find(|o| screens.output_identity(o) == event.output_identity)
        else {
            return;
        };
        let applied = match &ramp {
            Some(r) => output.set_gamma(Some((&r.red, &r.green, &r.blue))),
            None => output.set_gamma(None),
        };
        if let Err(e) = applied {
            eprintln!("pastel: gamma: {e}");
        }
    }

    pub(crate) fn ctm(&mut self, event: CtmEvent) {
        const IDENTITY: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let matrix = self
            .services
            .ctm
            .matrix(event.output_identity)
            .filter(|m| *m != IDENTITY);
        let screens = self.services.screens;
        let Some(output) = self
            .outputs
            .iter_mut()
            .find(|o| screens.output_identity(o) == event.output_identity)
        else {
            return;
        };
        if let Err(e) = output.set_ctm(matrix) {
            eprintln!("pastel: colour matrix: {e}");
        }
    }
}
