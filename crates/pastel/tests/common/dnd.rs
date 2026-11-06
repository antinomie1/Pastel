//! What a browser does to drag a tab: on a press, `start_drag` with an icon
//! surface the compositor must draw under the pointer.

use std::fs::File;
use std::os::fd::AsFd;

use wayland_client::protocol::{
    wl_buffer, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_pointer,
    wl_shm, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, event_created_child};

use super::App;

/// The icon's size and where it sits from the pointer.
pub const ICON: (i32, i32) = (20, 10);
pub const ICON_OFFSET: (i32, i32) = (-4, -6);

#[derive(Default)]
pub(super) struct Dragging {
    /// The window whose next press starts the drag.
    armed: Option<usize>,
    pointer: Option<wl_pointer::WlPointer>,
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    device: Option<wl_data_device::WlDataDevice>,
    /// Kept alive while the drag runs.
    icon: Option<(wl_surface::WlSurface, File, wl_buffer::WlBuffer)>,
    source: Option<wl_data_source::WlDataSource>,
}

impl App {
    pub(super) fn arm_drag(&mut self, index: usize) {
        let qh = self.qh.clone();
        if self.dragging.pointer.is_none() {
            self.dragging.pointer = Some(self.seat.get_pointer(&qh, ()));
            let manager: wl_data_device_manager::WlDataDeviceManager =
                self.globals.bind(&qh, 3..=3, ()).expect("data device");
            self.dragging.device = Some(manager.get_data_device(&self.seat, &qh, ()));
            self.dragging.manager = Some(manager);
        }
        self.dragging.armed = Some(index);
        self.report.lock().expect("report").drag_armed = true;
    }

    pub(super) fn pressed(&mut self, serial: u32) {
        let qh = self.qh.clone();
        let Some(index) = self.dragging.armed.take() else {
            return;
        };
        let dragging = &self.dragging;
        let (Some(manager), Some(device), Some(window)) =
            (&dragging.manager, &dragging.device, self.windows.get(&index))
        else {
            return;
        };
        let source = manager.create_data_source(&qh, ());
        source.offer("text/x-moz-url".into());
        source.set_actions(wl_data_device_manager::DndAction::Move);

        let (w, h) = ICON;
        let bytes: Vec<u8> = [0xff, 0xff, 0xff, 0xff].repeat((w * h) as usize);
        let file = Self::shm_file(&bytes);
        let pool = self.shm.create_pool(file.as_fd(), w * h * 4, &qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy();
        let icon = self.compositor.create_surface(&qh, ());
        device.start_drag(Some(&source), &window.surface, Some(&icon), serial);
        icon.attach(Some(&buffer), 0, 0);
        icon.offset(ICON_OFFSET.0, ICON_OFFSET.1);
        icon.damage_buffer(0, 0, w, h);
        icon.commit();
        self.dragging.icon = Some((icon, file, buffer));
        self.dragging.source = Some(source);
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_data_device::WlDataDevice,
        _: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(App, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

impl Dispatch<wl_data_source::WlDataSource, ()> for App {
    fn event(
        app: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_source::Event::Cancelled | wl_data_source::Event::DndFinished = event {
            source.destroy();
            if let Some((icon, ..)) = app.dragging.icon.take() {
                icon.destroy();
            }
            app.dragging.source = None;
        }
    }
}

wayland_client::delegate_noop!(App: ignore wl_data_device_manager::WlDataDeviceManager);
wayland_client::delegate_noop!(App: ignore wl_data_offer::WlDataOffer);
