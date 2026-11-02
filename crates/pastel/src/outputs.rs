//! What each monitor should be: from `[[output]]`, and at runtime from wlr-randr or
//! kanshi. One path for both, so a config and a client can't disagree.

use luminaria::backend::{Output, OutputId};
use luminaria::desktop::{OutputConfigRequest, OutputHeadRequest};
use luminaria::util::Transform;
use pastel_config::OutputConfig;

use crate::state::{State, logical};

/// One output as somebody wants it. `None` leaves that as it is.
#[derive(Debug, Clone, Default, PartialEq)]
struct Wanted {
    enabled: Option<bool>,
    mode: Option<(i32, i32, i32)>,
    scale: Option<f64>,
    transform: Option<Transform>,
    position: Option<(i32, i32)>,
}

impl From<&OutputConfig> for Wanted {
    fn from(config: &OutputConfig) -> Self {
        Self {
            enabled: Some(config.enabled),
            mode: config.mode,
            scale: config.scale,
            transform: config.transform.map(Transform::from_u32),
            position: config.position,
        }
    }
}

impl From<&OutputHeadRequest> for Wanted {
    fn from(head: &OutputHeadRequest) -> Self {
        Self {
            enabled: Some(head.enabled),
            mode: head.mode,
            scale: head.scale,
            transform: head.transform,
            position: head.position,
        }
    }
}

impl State<'_> {
    pub(crate) fn output_added(&mut self, output: Output) {
        let name = self.services.screens.output_name(&output);
        self.services.output_manager.add_head(&output, &name);
        let id = output.id();
        self.outputs.push(output);
        let config = self.config.outputs.iter().find(|o| o.name == name);
        let want = config.map(Wanted::from).unwrap_or_default();
        self.join_desktop(id);
        if let Err(e) = self.apply_output(id, &want) {
            eprintln!("pastel: output {name}: {e}");
        }
        self.arrange_monitors();
    }

    fn join_desktop(&mut self, id: OutputId) {
        if let Some(output) = self.outputs.iter().find(|o| o.id() == id) {
            let rect = logical(self.services.layout.output_box(output));
            self.desktop.add_monitor(id.0, rect);
        }
    }

    fn leave_desktop(&mut self, id: OutputId) {
        self.layers.close_output(id.0 as u32);
        self.forget_lock_monitor(id);
        self.desktop.remove_monitor(id.0);
        self.composer.remove_monitor(id.0);
    }

    /// `[[output]]`, again, after a reload.
    pub(crate) fn apply_output_config(&mut self) {
        let mut changes = Vec::new();
        for output in &self.outputs {
            let name = self.services.screens.output_name(output);
            let config = self.config.outputs.iter().find(|o| o.name == name);
            changes.extend(config.map(|c| (output.id(), name, Wanted::from(c))));
        }
        for (id, name, want) in changes {
            if let Err(e) = self.apply_output(id, &want) {
                eprintln!("pastel: output {name}: {e}");
            }
        }
        self.arrange_monitors();
    }

    fn is_enabled(&self, id: OutputId) -> bool {
        self.desktop.monitor(id.0).is_some()
    }

    fn apply_output(&mut self, id: OutputId, want: &Wanted) -> Result<(), String> {
        let enable = want.enabled.unwrap_or(self.is_enabled(id));
        if !enable && self.is_enabled(id) {
            if self.desktop.monitors().len() == 1 {
                return Err("the last output can't be turned off".into());
            }
            self.leave_desktop(id);
        }
        let layout = self.services.layout;
        let Some(output) = self.outputs.iter_mut().find(|o| o.id() == id) else {
            return Err("it's gone".into());
        };
        if !enable {
            layout.remove(output);
            return Ok(());
        }
        if let Some((width, height, refresh)) = want.mode {
            output
                .set_mode(width, height, refresh)
                .map_err(|e| e.to_string())?;
        }
        let scale = want.scale.filter(|scale| *scale != output.scale());
        let transform = want.transform.filter(|t| *t != output.transform());
        if let Some(scale) = scale {
            output.set_scale(scale);
        }
        if let Some(transform) = transform {
            output.set_transform(transform);
        }
        if scale.is_some() || transform.is_some() {
            output.announce_change();
        }
        match want.position {
            Some((x, y)) => layout.add(output, x, y),
            None if !self.desktop.monitor(id.0).is_some() => layout.add_auto(output),
            None => {}
        }
        if !self.is_enabled(id) {
            self.join_desktop(id);
        }
        Ok(())
    }

    /// wlr-randr or kanshi asked. All of it works, or none of it happens.
    pub(crate) fn output_request(&mut self, request: OutputConfigRequest) {
        let manager = self.services.output_manager;
        let heads = manager.heads_of(request.ticket);
        let wants = self.resolve_heads(&heads);
        let answer = match wants {
            Some(wants) if !request.test => self.apply_all(&wants),
            Some(_) => true,
            None => false,
        };
        manager.answer(request.ticket, answer);
        manager.update();
    }

    fn resolve_heads(&self, heads: &[OutputHeadRequest]) -> Option<Vec<(OutputId, Wanted)>> {
        let screens = self.services.screens;
        let named = |name: &str| self.outputs.iter().find(|o| screens.output_name(o) == name);
        let mut wants = Vec::new();
        for head in heads {
            let output = named(&head.name)?;
            let want = Wanted::from(head);
            let can_show = |&(w, h, _): &(i32, i32, i32)| {
                (w, h) == (output.width(), output.height())
                    || output.modes().iter().any(|m| (m.width, m.height) == (w, h))
            };
            if !want.mode.as_ref().is_none_or(can_show) {
                return None;
            }
            wants.push((output.id(), want));
        }
        wants
            .iter()
            .any(|(_, want)| want.enabled == Some(true))
            .then_some(wants)
    }

    fn apply_all(&mut self, wants: &[(OutputId, Wanted)]) -> bool {
        let before: Vec<_> = wants
            .iter()
            .map(|(id, _)| (*id, self.state_of(*id)))
            .collect();
        // Turning on comes first, or the last enabled output could refuse to go off.
        let mut order: Vec<_> = wants.iter().collect();
        order.sort_by_key(|(_, want)| want.enabled == Some(false));
        let failed = order
            .iter()
            .any(|(id, want)| match self.apply_output(*id, want) {
                Ok(()) => false,
                Err(e) => {
                    eprintln!("pastel: output configuration: {e}");
                    true
                }
            });
        if failed {
            for (id, state) in &before {
                let _ = self.apply_output(*id, state);
            }
        }
        self.arrange_monitors();
        !failed
    }

    fn state_of(&self, id: OutputId) -> Wanted {
        let output = self.outputs.iter().find(|o| o.id() == id);
        let Some(output) = output else {
            return Wanted::default();
        };
        let mode = output.current_mode();
        let place = self.services.layout.output_box(output);
        Wanted {
            enabled: Some(self.is_enabled(id)),
            mode: Some((mode.width, mode.height, mode.refresh_mhz)),
            scale: Some(output.scale()),
            transform: Some(output.transform()),
            position: Some((place.x, place.y)),
        }
    }
}
