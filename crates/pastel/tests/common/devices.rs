//! What a game, a drawing program, a night-light tool and wtype use: pointer
//! locks with relative motion, tablets, gamma ramps and a virtual keyboard
//! with its own keymap.

use std::os::fd::AsFd;

use wayland_client::protocol::{wl_output, wl_pointer};
use wayland_client::{Connection, Dispatch, QueueHandle, event_created_child};
use wayland_protocols::wp::pointer_constraints::zv1::client::{
    zwp_confined_pointer_v1, zwp_locked_pointer_v1, zwp_pointer_constraints_v1,
};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1, zwp_relative_pointer_v1,
};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2, zwp_tablet_pad_group_v2, zwp_tablet_pad_ring_v2,
    zwp_tablet_pad_strip_v2, zwp_tablet_pad_v2, zwp_tablet_seat_v2, zwp_tablet_tool_v2,
    zwp_tablet_v2,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};

use super::App;

/// What the devices said.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Devices {
    pub pointer_motions: usize,
    /// Where the last wl_pointer.motion put the pointer, surface-local.
    pub last_motion: Option<(f64, f64)>,
    pub locked: bool,
    pub confined: bool,
    pub relative: Vec<(f64, f64)>,
    pub tablets: usize,
    pub tool_in: usize,
    pub tool_motions: Vec<(f64, f64)>,
    pub tool_down: usize,
    /// `Some(size)` once a gamma control said how big, `None` before.
    pub gamma_size: Option<u32>,
    pub gamma_failed: bool,
    pub keymaps: usize,
}

impl App {
    fn devices(&self, change: impl FnOnce(&mut Devices)) {
        change(&mut self.report.lock().expect("report").devices);
    }

    pub(super) fn lock_pointer(&mut self, index: usize) {
        let qh = self.qh.clone();
        let Some(window) = self.windows.get(&index) else {
            return;
        };
        let pointer = self.seat.get_pointer(&qh, ());
        let constraints: zwp_pointer_constraints_v1::ZwpPointerConstraintsV1 = self
            .globals
            .bind(&qh, 1..=1, ())
            .expect("pointer constraints");
        let relative: zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("relative pointer");
        relative.get_relative_pointer(&pointer, &qh, ());
        constraints.lock_pointer(
            &window.surface,
            &pointer,
            None,
            zwp_pointer_constraints_v1::Lifetime::Persistent,
            &qh,
            (),
        );
        window.surface.commit();
    }

    pub(super) fn confine_pointer(&mut self, index: usize) {
        let qh = self.qh.clone();
        let Some(window) = self.windows.get(&index) else {
            return;
        };
        let pointer = self.seat.get_pointer(&qh, ());
        let constraints: zwp_pointer_constraints_v1::ZwpPointerConstraintsV1 = self
            .globals
            .bind(&qh, 1..=1, ())
            .expect("pointer constraints");
        constraints.confine_pointer(
            &window.surface,
            &pointer,
            None,
            zwp_pointer_constraints_v1::Lifetime::Persistent,
            &qh,
            (),
        );
        window.surface.commit();
    }

    pub(super) fn watch_tablets(&mut self) {
        let qh = self.qh.clone();
        let manager: zwp_tablet_manager_v2::ZwpTabletManagerV2 =
            self.globals.bind(&qh, 1..=1, ()).expect("tablets");
        manager.get_tablet_seat(&self.seat, &qh, ());
    }

    pub(super) fn ask_gamma(&mut self) {
        let qh = self.qh.clone();
        let output: wl_output::WlOutput = self.globals.bind(&qh, 1..=4, ()).expect("output");
        let manager: zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("gamma");
        manager.get_gamma_control(&output, &qh, ());
    }

    /// Types `key` through a virtual keyboard whose keymap is `keymap`.
    pub(super) fn virtual_type(&mut self, keymap: &str, key: u32) {
        let qh = self.qh.clone();
        let manager: zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("virtual keyboard");
        let keyboard = manager.create_virtual_keyboard(&self.seat, &qh, ());
        let mut text = keymap.as_bytes().to_vec();
        text.push(0);
        let file = Self::shm_file(&text);
        keyboard.keymap(1, file.as_fd(), text.len() as u32);
        keyboard.key(0, key, 1);
        keyboard.key(1, key, 0);
    }

    pub(super) fn keymap_arrived(&self) {
        self.devices(|d| d.keymaps += 1);
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(
        app: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Motion {
            surface_x,
            surface_y,
            ..
        } = event
        {
            app.devices(|d| {
                d.pointer_motions += 1;
                d.last_motion = Some((surface_x, surface_y));
            });
        }
    }
}

impl Dispatch<zwp_locked_pointer_v1::ZwpLockedPointerV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &zwp_locked_pointer_v1::ZwpLockedPointerV1,
        event: zwp_locked_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_locked_pointer_v1::Event::Locked => app.devices(|d| d.locked = true),
            zwp_locked_pointer_v1::Event::Unlocked => app.devices(|d| d.locked = false),
            _ => {}
        }
    }
}

impl Dispatch<zwp_confined_pointer_v1::ZwpConfinedPointerV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &zwp_confined_pointer_v1::ZwpConfinedPointerV1,
        event: zwp_confined_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_confined_pointer_v1::Event::Confined => app.devices(|d| d.confined = true),
            zwp_confined_pointer_v1::Event::Unconfined => app.devices(|d| d.confined = false),
            _ => {}
        }
    }
}

impl Dispatch<zwp_relative_pointer_v1::ZwpRelativePointerV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &zwp_relative_pointer_v1::ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } = event {
            app.devices(|d| d.relative.push((dx, dy)));
        }
    }
}

impl Dispatch<zwp_tablet_seat_v2::ZwpTabletSeatV2, ()> for App {
    fn event(
        app: &mut Self,
        _: &zwp_tablet_seat_v2::ZwpTabletSeatV2,
        event: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_seat_v2::Event::TabletAdded { .. } = event {
            app.devices(|d| d.tablets += 1);
        }
    }
    event_created_child!(App, zwp_tablet_seat_v2::ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (zwp_tablet_v2::ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (zwp_tablet_tool_v2::ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (zwp_tablet_pad_v2::ZwpTabletPadV2, ()),
    ]);
}

impl Dispatch<zwp_tablet_tool_v2::ZwpTabletToolV2, ()> for App {
    fn event(
        app: &mut Self,
        _: &zwp_tablet_tool_v2::ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_tablet_tool_v2::Event::ProximityIn { .. } => app.devices(|d| d.tool_in += 1),
            zwp_tablet_tool_v2::Event::Motion { x, y } => {
                app.devices(|d| d.tool_motions.push((x, y)))
            }
            zwp_tablet_tool_v2::Event::Down { .. } => app.devices(|d| d.tool_down += 1),
            _ => {}
        }
    }
}

impl Dispatch<zwp_tablet_pad_v2::ZwpTabletPadV2, ()> for App {
    fn event(
        _: &mut Self,
        _: &zwp_tablet_pad_v2::ZwpTabletPadV2,
        _: zwp_tablet_pad_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(App, zwp_tablet_pad_v2::ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, ()> for App {
    fn event(
        _: &mut Self,
        _: &zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2,
        _: zwp_tablet_pad_group_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(App, zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2, ()),
    ]);
}

impl Dispatch<zwlr_gamma_control_v1::ZwlrGammaControlV1, ()> for App {
    fn event(
        app: &mut Self,
        _: &zwlr_gamma_control_v1::ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                app.devices(|d| d.gamma_size = Some(size))
            }
            zwlr_gamma_control_v1::Event::Failed => app.devices(|d| d.gamma_failed = true),
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(App: ignore zwp_pointer_constraints_v1::ZwpPointerConstraintsV1);
wayland_client::delegate_noop!(App: ignore zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1);
wayland_client::delegate_noop!(App: ignore zwp_tablet_manager_v2::ZwpTabletManagerV2);
wayland_client::delegate_noop!(App: ignore zwp_tablet_v2::ZwpTabletV2);
wayland_client::delegate_noop!(App: ignore zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2);
wayland_client::delegate_noop!(App: ignore zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2);
wayland_client::delegate_noop!(App: ignore zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1);
wayland_client::delegate_noop!(App: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
wayland_client::delegate_noop!(App: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);
