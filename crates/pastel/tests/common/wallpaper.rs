//! A wallpaper client that asks the compositor for exactly what swaybg does, so a
//! test sees what a user with `autostart = ["swaybg -i ..."]` gets: one background
//! layer surface for each `wl_output`, named, filling it, ignoring exclusive zones,
//! taking no input, and opaque.

use std::fs::File;
use std::os::fd::AsFd;

use wayland_client::protocol::{wl_buffer, wl_output, wl_shm, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use super::App;

/// A picture to fill an output with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Solid(u32),
    /// Vertical stripes, each `.2` pixels wide.
    Stripes(u32, u32, i32),
}

impl Pattern {
    fn at(self, x: i32) -> u32 {
        match self {
            Self::Solid(rgb) => rgb,
            Self::Stripes(a, b, width) => {
                if (x / width) % 2 == 0 {
                    a
                } else {
                    b
                }
            }
        }
    }
}

pub struct Wallpaper {
    pattern: Pattern,
    surface: wl_surface::WlSurface,
    layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    drawn: Option<(File, wl_buffer::WlBuffer)>,
}

impl App {
    pub(super) fn start_wallpaper(&mut self, pattern: Pattern) {
        self.stop_wallpaper();
        use zwlr_layer_surface_v1::Anchor;
        let qh = self.qh.clone();
        let shell: zwlr_layer_shell_v1::ZwlrLayerShellV1 =
            self.globals.bind(&qh, 1..=4, ()).expect("layer shell");
        let outputs: Vec<_> = self
            .globals
            .contents()
            .clone_list()
            .into_iter()
            .filter(|global| global.interface == "wl_output")
            .collect();
        for global in outputs {
            let output: wl_output::WlOutput = self.globals.registry().bind(global.name, 4, &qh, ());
            let surface = self.compositor.create_surface(&qh, ());
            let nothing = self.compositor.create_region(&qh, ());
            surface.set_input_region(Some(&nothing));
            let index = self.wallpapers.len();
            let layer = shell.get_layer_surface(
                &surface,
                Some(&output),
                zwlr_layer_shell_v1::Layer::Background,
                "wallpaper".into(),
                &qh,
                index,
            );
            layer.set_size(0, 0);
            layer.set_anchor(Anchor::Top | Anchor::Right | Anchor::Bottom | Anchor::Left);
            layer.set_exclusive_zone(-1);
            surface.commit();
            self.wallpapers.push(Wallpaper {
                pattern,
                surface,
                layer,
                drawn: None,
            });
        }
    }

    pub(super) fn stop_wallpaper(&mut self) {
        for wallpaper in self.wallpapers.drain(..) {
            wallpaper.layer.destroy();
            wallpaper.surface.destroy();
        }
    }

    fn draw_wallpaper(&mut self, index: usize, w: i32, h: i32) {
        let qh = self.qh.clone();
        let wallpaper = &self.wallpapers[index];
        let row: Vec<u8> = (0..w)
            .flat_map(|x| wallpaper.pattern.at(x).to_le_bytes())
            .collect();
        let bytes = row.repeat(h as usize);
        let file = Self::shm_file(&bytes);
        let pool = self.shm.create_pool(file.as_fd(), w * h * 4, &qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Xrgb8888, &qh, ());
        pool.destroy();
        let opaque = self.compositor.create_region(&qh, ());
        opaque.add(0, 0, w, h);
        let wallpaper = &mut self.wallpapers[index];
        wallpaper.surface.set_opaque_region(Some(&opaque));
        wallpaper.surface.attach(Some(&buffer), 0, 0);
        wallpaper.surface.damage_buffer(0, 0, w, h);
        wallpaper.surface.commit();
        wallpaper.drawn = Some((file, buffer));
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, usize> for App {
    fn event(
        app: &mut Self,
        layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer.ack_configure(serial);
                let size = (width as i32, height as i32);
                app.report.lock().expect("report").wallpapers.push(size);
                // Configures for a surface already stopped are still on their way.
                if app
                    .wallpapers
                    .get(*index)
                    .is_some_and(|w| w.layer == *layer)
                {
                    app.draw_wallpaper(*index, size.0, size.1);
                }
            }
            zwlr_layer_surface_v1::Event::Closed => layer.destroy(),
            _ => {}
        }
    }
}
