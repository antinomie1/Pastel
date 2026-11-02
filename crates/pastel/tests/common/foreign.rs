//! What a bar and an idle daemon use: the window list and idle notifications.

use wayland_client::{Connection, Dispatch, QueueHandle, event_created_child};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as handle, zwlr_foreign_toplevel_manager_v1 as manager,
};

use super::App;

pub type Handle = handle::ZwlrForeignToplevelHandleV1;

impl App {
    pub(super) fn watch_idle(&mut self, ms: u32) {
        let qh = self.qh.clone();
        let notifier: ext_idle_notifier_v1::ExtIdleNotifierV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("idle notifier");
        notifier.get_idle_notification(ms, &self.seat, &qh, ());
    }

    pub(super) fn watch_windows(&mut self) {
        let qh = self.qh.clone();
        let _: manager::ZwlrForeignToplevelManagerV1 =
            self.globals.bind(&qh, 1..=3, ()).expect("taskbar");
    }

    pub(super) fn taskbar_activate(&mut self, index: usize) {
        self.taskbar[index].activate(&self.seat);
    }

    pub(super) fn taskbar_close(&mut self, index: usize) {
        self.taskbar[index].close();
    }
}

impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ext_idle_notifier_v1::ExtIdleNotifierV1,
        _: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let idled = matches!(event, ext_idle_notification_v1::Event::Idled);
        app.report.lock().expect("report").idle.push(idled);
    }
}

impl Dispatch<manager::ZwlrForeignToplevelManagerV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &manager::ZwlrForeignToplevelManagerV1,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let manager::Event::Toplevel { toplevel } = event {
            app.taskbar.push(toplevel);
        }
    }

    event_created_child!(App, manager::ZwlrForeignToplevelManagerV1, [
        manager::EVT_TOPLEVEL_OPCODE => (Handle, ()),
    ]);
}

impl Dispatch<Handle, ()> for App {
    fn event(
        app: &mut Self,
        _: &Handle,
        event: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let handle::Event::AppId { app_id } = event {
            app.report.lock().expect("report").taskbar.push(app_id);
        }
    }
}
