//! ext-session-lock from the client's side, as swaylock uses it.

use std::fs::File;
use std::os::fd::AsFd;

use wayland_client::protocol::{wl_buffer, wl_output, wl_shm, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};

use super::App;

pub struct Locker {
    lock: ext_session_lock_v1::ExtSessionLockV1,
    surfaces: Vec<LockedSurface>,
}

struct LockedSurface {
    surface: wl_surface::WlSurface,
    role: ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
    drawn: Option<(File, wl_buffer::WlBuffer)>,
}

impl App {
    pub(super) fn lock(&mut self) {
        let qh = self.qh.clone();
        let manager: ext_session_lock_manager_v1::ExtSessionLockManagerV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("session lock");
        let lock = manager.lock(&qh, ());
        let outputs: Vec<_> = self
            .globals
            .contents()
            .clone_list()
            .into_iter()
            .filter(|global| global.interface == "wl_output")
            .collect();
        let mut surfaces = Vec::new();
        for (index, global) in outputs.into_iter().enumerate() {
            let output: wl_output::WlOutput = self.globals.registry().bind(global.name, 4, &qh, ());
            let surface = self.compositor.create_surface(&qh, ());
            let role = lock.get_lock_surface(&surface, &output, &qh, index);
            surfaces.push(LockedSurface {
                surface,
                role,
                drawn: None,
            });
        }
        self.session_lock = Some(Locker { lock, surfaces });
    }

    pub(super) fn unlock(&mut self) {
        if let Some(locker) = self.session_lock.take() {
            for locked in locker.surfaces {
                locked.role.destroy();
                locked.surface.destroy();
            }
            locker.lock.unlock_and_destroy();
            self.report.lock().expect("report").locked = false;
        }
    }

    fn draw_lock(&mut self, index: usize, w: i32, h: i32) {
        let qh = self.qh.clone();
        let bytes = vec![0x80u8; (w * h * 4) as usize];
        let file = Self::shm_file(&bytes);
        let pool = self.shm.create_pool(file.as_fd(), w * h * 4, &qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Xrgb8888, &qh, ());
        pool.destroy();
        let Some(locker) = &mut self.session_lock else {
            return;
        };
        let locked = &mut locker.surfaces[index];
        locked.surface.attach(Some(&buffer), 0, 0);
        locked.surface.damage_buffer(0, 0, w, h);
        locked.surface.commit();
        locked.drawn = Some((file, buffer));
    }
}

impl Dispatch<ext_session_lock_manager_v1::ExtSessionLockManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &ext_session_lock_manager_v1::ExtSessionLockManagerV1,
        _: ext_session_lock_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, ext_session_lock_v1::Event::Locked) {
            app.report.lock().expect("report").locked = true;
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, usize> for App {
    fn event(
        app: &mut Self,
        role: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            role.ack_configure(serial);
            let size = (width, height);
            app.report.lock().expect("report").lock_sizes.push(size);
            app.draw_lock(*index, width as i32, height as i32);
        }
    }
}
