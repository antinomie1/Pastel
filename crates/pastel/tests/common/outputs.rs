//! wlr-output-management from the client's side, as wlr-randr uses it.

use wayland_client::{Connection, Dispatch, QueueHandle, event_created_child};
use wayland_protocols_wlr::output_management::v1::client::{
    zwlr_output_configuration_head_v1 as config_head,
    zwlr_output_configuration_v1 as configuration, zwlr_output_head_v1 as head,
    zwlr_output_manager_v1 as manager, zwlr_output_mode_v1 as mode,
};

use super::App;

#[derive(Default)]
pub struct Randr {
    manager: Option<manager::ZwlrOutputManagerV1>,
    head: Option<head::ZwlrOutputHeadV1>,
    serial: u32,
    /// A scale to ask for once the state has arrived, and whether just to test it.
    queued: Option<(f64, bool)>,
}

impl App {
    pub(super) fn randr(&mut self, scale: f64, test: bool) {
        self.randr.queued = Some((scale, test));
        if self.randr.manager.is_none() {
            let qh = self.qh.clone();
            self.randr.manager = Some(self.globals.bind(&qh, 1..=4, ()).expect("output manager"));
        } else {
            self.randr_send();
        }
    }

    fn randr_send(&mut self) {
        let qh = self.qh.clone();
        let (Some(manager), Some(head)) = (&self.randr.manager, &self.randr.head) else {
            return;
        };
        let Some((scale, test)) = self.randr.queued.take() else {
            return;
        };
        let config = manager.create_configuration(self.randr.serial, &qh, ());
        let wanted = config.enable_head(head, &qh, ());
        wanted.set_scale(scale);
        if test {
            config.test();
        } else {
            config.apply();
        }
    }
}

impl Dispatch<manager::ZwlrOutputManagerV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &manager::ZwlrOutputManagerV1,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            manager::Event::Head { head } => app.randr.head = Some(head),
            manager::Event::Done { serial } => {
                app.randr.serial = serial;
                app.randr_send();
            }
            _ => {}
        }
    }

    event_created_child!(App, manager::ZwlrOutputManagerV1, [
        manager::EVT_HEAD_OPCODE => (head::ZwlrOutputHeadV1, ()),
    ]);
}

impl Dispatch<head::ZwlrOutputHeadV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &head::ZwlrOutputHeadV1,
        event: head::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let head::Event::Name { name } = event {
            app.report.lock().expect("report").heads.push(name);
        }
    }

    event_created_child!(App, head::ZwlrOutputHeadV1, [
        head::EVT_MODE_OPCODE => (mode::ZwlrOutputModeV1, ()),
    ]);
}

impl Dispatch<mode::ZwlrOutputModeV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &mode::ZwlrOutputModeV1,
        _: mode::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<config_head::ZwlrOutputConfigurationHeadV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &config_head::ZwlrOutputConfigurationHeadV1,
        _: config_head::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<configuration::ZwlrOutputConfigurationV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &configuration::ZwlrOutputConfigurationV1,
        event: configuration::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let succeeded = matches!(event, configuration::Event::Succeeded);
        app.report.lock().expect("report").randr.push(succeeded);
    }
}
