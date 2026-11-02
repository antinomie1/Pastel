//! wlr-screencopy from the client's side, as grim uses it.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::AsFd;

use wayland_client::protocol::{wl_buffer, wl_output};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1 as frame, zwlr_screencopy_manager_v1 as manager,
};

use super::App;

/// A finished capture.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixels: Vec<u8>,
}

impl Captured {
    /// (r, g, b) at a point, whichever of the two orders the format was.
    pub fn at(&self, x: u32, y: u32) -> [u8; 3] {
        let i = (y * self.stride + x * 4) as usize;
        let [b, g, r] = self.pixels[i..i + 3].try_into().expect("three bytes");
        [r, g, b]
    }
}

#[derive(Default)]
pub struct Capturing {
    buffer: Option<(File, wl_buffer::WlBuffer, u32, u32, u32)>,
}

impl App {
    pub(super) fn capture(&mut self) {
        let qh = self.qh.clone();
        let screencopy: manager::ZwlrScreencopyManagerV1 =
            self.globals.bind(&qh, 1..=3, ()).expect("screencopy");
        let output: wl_output::WlOutput = self.globals.bind(&qh, 1..=4, ()).expect("wl_output");
        self.capturing = Capturing::default();
        screencopy.capture_output(0, &output, &qh, ());
    }
}

impl Dispatch<manager::ZwlrScreencopyManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &manager::ZwlrScreencopyManagerV1,
        _: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<frame::ZwlrScreencopyFrameV1, ()> for App {
    fn event(
        app: &mut Self,
        copy: &frame::ZwlrScreencopyFrameV1,
        event: frame::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            frame::Event::Buffer {
                format: WEnum::Value(format),
                width,
                height,
                stride,
            } => {
                let size = (stride * height) as i32;
                let file = Self::shm_file(&vec![0; size as usize]);
                let pool = app.shm.create_pool(file.as_fd(), size, qh, ());
                let (w, h, s) = (width as i32, height as i32, stride as i32);
                let buffer = pool.create_buffer(0, w, h, s, format, qh, ());
                pool.destroy();
                app.capturing.buffer = Some((file, buffer, width, height, stride));
            }
            frame::Event::BufferDone => {
                if let Some((_, buffer, ..)) = &app.capturing.buffer {
                    copy.copy(buffer);
                }
            }
            frame::Event::Ready { .. } => {
                let Some((file, _, width, height, stride)) = &mut app.capturing.buffer else {
                    return;
                };
                let mut pixels = Vec::new();
                file.seek(SeekFrom::Start(0)).expect("seek shm");
                file.read_to_end(&mut pixels).expect("read shm");
                let captured = Captured {
                    width: *width,
                    height: *height,
                    stride: *stride,
                    pixels,
                };
                app.report.lock().expect("report").captured = Some(captured);
            }
            frame::Event::Failed => {
                app.report.lock().expect("report").capture_failed = true;
            }
            _ => {}
        }
    }
}
