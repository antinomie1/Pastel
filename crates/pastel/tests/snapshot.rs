//! Real pixels: the scene Pastel built, rendered by the GPU into a second headless
//! output and read back. Skips itself on a machine with no usable GPU.
//!
//! Set PASTEL_SNAPSHOT_DIR to keep each frame as `<name>.rgba` (1280x800 RGBA8).

mod common;

use std::time::{Duration, Instant};

use common::Client;
use luminaria::backend::{BackendKind, BackendOptions, HeadlessBackend, Output};
use luminaria::core::Display;
use luminaria::gpu::SceneOutcome;
use luminaria::shell::{SceneItem, SceneKind};
use luminaria::util::{Color, Rect as PixelRect};
use pastel::app;
use pastel::state::State;
use pastel_config::Config;
use pastel_layout::{Direction, LayoutOp};

/// Corners are square by default; these tests are about the rounded path.
const ROUNDED: &str = "[overlay]\nat-startup = false\n[window]\ncorner-radius = 12.0";
const WIDTH: i32 = 1280;
const HEIGHT: i32 = 800;
const RED: u32 = 0xffd05050;
const BLUE: u32 = 0xff5070d0;
const GLASS: u32 = 0x60204060;
const BG: Color = Color::rgb(0.118, 0.125, 0.157);

struct Shot {
    pixels: Vec<u8>,
    outcome: SceneOutcome,
}

impl Shot {
    fn at(&self, x: i32, y: i32) -> [u8; 4] {
        let i = (y * WIDTH + x) as usize * 4;
        self.pixels[i..i + 4].try_into().expect("four bytes")
    }

    fn near(&self, x: i32, y: i32, want: [u8; 3]) -> bool {
        let got = self.at(x, y);
        (0..3).all(|c| got[c].abs_diff(want[c]) <= 3)
    }
}

/// `state`'s current scene, plus `under`, as it comes out of the GPU.
fn shoot(state: &State<'_>, display: &Display, name: &str, under: &[SceneItem]) -> Shot {
    let mut backend = HeadlessBackend::new(display, 16).expect("backend");
    let mut output = backend.add_output(WIDTH, HEIGHT).expect("output");
    let renderer = state.services.renderer;
    renderer.reset(&mut output).expect("reset");
    let mut scene = under.to_vec();
    scene.extend_from_slice(&state.scene);
    let outcome = renderer.present(&mut output, &scene, BG).expect("present");
    let pixels = renderer.read_back(&output).expect("read back");
    if let Ok(dir) = std::env::var("PASTEL_SNAPSHOT_DIR") {
        std::fs::write(format!("{dir}/{name}.rgba"), &pixels).expect("write snapshot");
    }
    Shot { pixels, outcome }
}

/// One output kept across frames, so what's cached between them is really tested.
struct Camera<'d> {
    _backend: HeadlessBackend<'d>,
    output: Output,
}

impl<'d> Camera<'d> {
    fn new(state: &State<'_>, display: &'d Display) -> Self {
        let mut backend = HeadlessBackend::new(display, 16).expect("backend");
        let mut output = backend.add_output(WIDTH, HEIGHT).expect("output");
        state.services.renderer.reset(&mut output).expect("reset");
        Self {
            _backend: backend,
            output,
        }
    }

    fn present(&mut self, state: &State<'_>) -> SceneOutcome {
        state
            .services
            .renderer
            .present(&mut self.output, &state.scene, BG)
            .expect("present")
    }

    fn pixels(&mut self, state: &State<'_>) -> Shot {
        Shot {
            pixels: state
                .services
                .renderer
                .read_back(&self.output)
                .expect("read back"),
            outcome: SceneOutcome::Composited,
        }
    }
}

fn scenario(config: &str, body: impl for<'a> FnOnce(&mut State<'a>, &'a Display, &Client)) {
    let config = Config::parse(config).expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        if !state.services.renderer.gpu_enabled() {
            eprintln!("skipped: no GPU");
            return Ok(());
        }
        let client = Client::connect(state.socket());
        body(state, display, &client);
        Ok(())
    })
    .expect("compositor runs");
}

fn settle(state: &mut State<'_>, display: &Display, mut done: impl FnMut(&State<'_>) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(state) {
        assert!(Instant::now() < deadline, "timed out");
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

/// `count` windows mapped and everything finished moving.
fn at_rest(state: &mut State<'_>, display: &Display, count: usize) {
    // Whatever was just asked for has to start before it can have finished.
    app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    settle(state, display, |s| {
        s.windows_mapped() == count && s.timeout().is_none() && s.scene_has_surfaces(count)
    });
    // One more turn for the last commit to reach the scene.
    for _ in 0..4 {
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

#[test]
fn windows_render_with_rounded_borders_gaps_and_focus() {
    scenario(ROUNDED, |state, display, client| {
        client.open("one", RED);
        client.open("two", RED);
        at_rest(state, display, 2);
        let shot = shoot(state, display, "columns", &[]);
        let red = [0xd0, 0x50, 0x50];
        assert!(
            shot.near(300, 400, red),
            "first window: {:?}",
            shot.at(300, 400)
        );
        assert!(
            shot.near(1000, 400, red),
            "second window: {:?}",
            shot.at(1000, 400)
        );
        // Gaps show the background, darkened by the windows' shadows.
        for x in [640, 1275] {
            let gap = shot.at(x, 400);
            assert!(
                gap[0] < 40 && gap[1] < 40 && gap[2] < 50,
                "gap at {x}: {gap:?}"
            );
        }
        // Left window's border is the inactive colour; the focused one's is active.
        assert!(
            shot.near(11, 400, [0x4a, 0x4f, 0x5c]),
            "inactive: {:?}",
            shot.at(11, 400)
        );
        assert!(
            shot.near(645, 400, [0xa6, 0xc8, 0xff]),
            "active: {:?}",
            shot.at(645, 400)
        );
        // Corners are cut, on the window and on the border around it.
        assert!(
            !shot.near(12, 12, red),
            "square window corner: {:?}",
            shot.at(12, 12)
        );
        let corner = shot.at(10, 10);
        assert!(corner[0] < 45, "square border corner: {corner:?}");
    });
}

#[test]
fn corners_are_square_by_default() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        at_rest(state, display, 1);
        let shot = shoot(state, display, "square", &[]);
        assert!(
            shot.near(12, 12, [0xd0, 0x50, 0x50]),
            "the window fills its corner: {:?}",
            shot.at(12, 12)
        );
        assert!(
            shot.near(10, 10, [0xa6, 0xc8, 0xff]),
            "the border fills its corner: {:?}",
            shot.at(10, 10)
        );
    });
}

#[test]
fn an_idle_rounded_window_is_not_redrawn_but_new_content_is() {
    scenario(ROUNDED, |state, display, client| {
        client.open("one", RED);
        at_rest(state, display, 1);
        let mut camera = Camera::new(state, display);
        camera.present(state);
        assert_eq!(
            camera.present(state),
            SceneOutcome::Unchanged,
            "nothing moved"
        );
        let shot = camera.pixels(state);
        assert!(
            shot.near(300, 400, [0xd0, 0x50, 0x50]),
            "{:?}",
            shot.at(300, 400)
        );

        // The same wl_buffer with new pixels: only the commit says anything changed.
        client.repaint(0, BLUE);
        // A client gets to it when it gets to it; only never would be a bug.
        let deadline = Instant::now() + Duration::from_secs(5);
        while camera.present(state) == SceneOutcome::Unchanged {
            assert!(
                Instant::now() < deadline,
                "new pixels never reached the screen"
            );
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }
        let shot = camera.pixels(state);
        assert!(
            shot.near(300, 400, [0x50, 0x70, 0xd0]),
            "stale: {:?}",
            shot.at(300, 400)
        );
        assert_eq!(camera.present(state), SceneOutcome::Unchanged);
    });
}

#[test]
fn a_window_that_only_moves_shows_the_same_pixels_in_the_new_place() {
    scenario(ROUNDED, |state, display, client| {
        client.open("one", RED);
        at_rest(state, display, 1);
        let mut camera = Camera::new(state, display);
        camera.present(state);
        let mut moved = state.scene.clone();
        for item in &mut moved {
            item.rect.x += 300;
            item.surface_origin.0 += 300;
        }
        let renderer = state.services.renderer;
        let outcome = renderer
            .present(&mut camera.output, &moved, BG)
            .expect("present");
        assert_ne!(outcome, SceneOutcome::Unchanged);
        let shot = camera.pixels(state);
        assert!(
            shot.near(800, 400, [0xd0, 0x50, 0x50]),
            "{:?}",
            shot.at(800, 400)
        );
        assert!(
            !shot.near(100, 400, [0xd0, 0x50, 0x50]),
            "{:?}",
            shot.at(100, 400)
        );
    });
}

#[test]
fn a_scrolled_layout_moves_what_is_on_screen() {
    let config = "[overlay]\nat-startup = false\n[layout]\ndefault-width = 0.75";
    scenario(config, |state, display, client| {
        client.open("one", RED);
        client.open("two", RED);
        at_rest(state, display, 2);
        // 939px columns: x=957 is inside the second one, until the view scrolls
        // back and makes it the gap between the two.
        let red = [0xd0, 0x50, 0x50];
        let before = shoot(state, display, "scrolled-right", &[]);
        assert!(before.near(957, 400, red), "{:?}", before.at(957, 400));
        state.desktop.apply(LayoutOp::Focus(Direction::Left));
        state.dirty = true;
        at_rest(state, display, 2);
        let after = shoot(state, display, "scrolled-left", &[]);
        assert!(!after.near(957, 400, red), "{:?}", after.at(957, 400));
    });
}

#[test]
fn the_hotkey_overlay_shows_text_on_a_panel() {
    scenario("", |state, display, client| {
        client.open("one", RED);
        at_rest(state, display, 1);
        let shot = shoot(state, display, "overlay", &[]);
        let (cx, cy) = (WIDTH / 2, HEIGHT / 2);
        // Text means bright pixels in the panel's middle.
        let bright = (0..500)
            .flat_map(|x| (0..120).map(move |y| (x, y)))
            .filter(|&(x, y)| shot.at(cx - 250 + x, cy - 60 + y)[0] > 150)
            .count();
        assert!(bright > 200, "no text on the panel: {bright} bright pixels");
    });
}

/// Left to right along a row: the first pixel of the panel's border.
fn panel_border(shot: &Shot, y: i32) -> Option<i32> {
    (200..WIDTH / 2).find(|&x| shot.near(x, y, [0xa6, 0xc8, 0xff]))
}

#[test]
fn the_hotkey_overlay_is_framed_like_a_window() {
    scenario("", |state, display, _client| {
        settle(state, display, |s| {
            s.timeout().is_none() && !s.scene.is_empty()
        });
        let shot = shoot(state, display, "overlay-square", &[]);
        let edge = panel_border(&shot, HEIGHT / 2).expect("a border");
        let top = (0..HEIGHT / 2).find(|&y| shot.near(WIDTH / 2, y, [0xa6, 0xc8, 0xff]));
        let top = top.expect("a border along the top");
        assert!(
            shot.near(edge, top, [0xa6, 0xc8, 0xff]),
            "square by default"
        );
    });
    scenario(
        "[window]\ncorner-radius = 12.0",
        |state, display, _client| {
            settle(state, display, |s| {
                s.timeout().is_none() && !s.scene.is_empty()
            });
            let shot = shoot(state, display, "overlay-rounded", &[]);
            let edge = panel_border(&shot, HEIGHT / 2).expect("a border");
            let top = (0..HEIGHT / 2).find(|&y| shot.near(WIDTH / 2, y, [0xa6, 0xc8, 0xff]));
            let top = top.expect("a border along the top");
            assert!(
                !shot.near(edge, top, [0xa6, 0xc8, 0xff]),
                "follows corner-radius"
            );
        },
    );
}

#[test]
fn blur_softens_what_is_behind_a_translucent_window() {
    // Hard stripes the blur has to smear, standing in for a wallpaper.
    let stripes: Vec<SceneItem> = (0..WIDTH / 40)
        .map(|i| {
            let rect = PixelRect::new(i * 40, 0, 20, HEIGHT);
            SceneItem::rect(rect, Color::rgb(1.0, 1.0, 1.0)).with_accepts_input(false)
        })
        .collect();
    let contrast = |shot: &Shot| {
        let row: Vec<u8> = (100..500).map(|x| shot.at(x, 400)[1]).collect();
        row.iter().max().expect("row") - row.iter().min().expect("row")
    };
    let mut sharp = 0;
    let mut sharp_detail = String::new();
    scenario(
        "[overlay]\nat-startup = false\n[window]\nblur = false",
        |state, display, client| {
            client.open("glass", GLASS);
            at_rest(state, display, 1);
            let shot = shoot(state, display, "unblurred", &stripes);
            sharp = contrast(&shot);
            // Seen once in some 500 runs, cause unknown: a frame with no stripes.
            let sample = (shot.at(110, 400), shot.at(130, 400), shot.at(1000, 400));
            sharp_detail = format!("{:?}, pixels {sample:?}", shot.outcome);
        },
    );
    let mut soft = 0;
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("glass", GLASS);
        at_rest(state, display, 1);
        soft = contrast(&shoot(state, display, "blurred", &stripes));
    });
    assert!(
        sharp > 60,
        "the stripes should show through unblurred: {sharp} ({sharp_detail})"
    );
    assert!(
        soft * 3 < sharp,
        "blur should flatten the stripes: {soft} vs {sharp}"
    );
}

/// What `grim` does, through the output the screen is drawn on.
#[test]
fn a_client_can_capture_what_is_on_the_output() {
    scenario(
        "[overlay]\nat-startup = false\n[window]\nblur = false\nborder-width = 0.0",
        |state, display, client| {
            client.open("shown", RED);
            at_rest(state, display, 1);
            client.capture();
            settle(state, display, |_| client.captured().is_some());
            let captured = client.captured().expect("a capture");
            assert_eq!(
                (captured.width, captured.height),
                (WIDTH as u32, HEIGHT as u32)
            );
            let rect = state.scene.iter().rfind(|i| i.kind == SceneKind::Surface);
            let rect = rect.expect("the window").rect;
            let inside = captured.at(
                (rect.x + rect.width / 2) as u32,
                (rect.y + rect.height / 2) as u32,
            );
            let [r, g, b] = [(RED >> 16) as u8, (RED >> 8) as u8, RED as u8];
            let near = |a: u8, b: u8| a.abs_diff(b) <= 3;
            assert!(
                near(inside[0], r) && near(inside[1], g) && near(inside[2], b),
                "{inside:?}"
            );
            let outside = if rect.x > 100 { 10 } else { WIDTH as u32 - 10 };
            assert_ne!(captured.at(outside, HEIGHT as u32 / 2), inside);
        },
    );
}

/// 1.5 turns 1280x800 into a desktop of 854x534 on the same pixels. A window lands
/// where its logical place times the scale says, and the desktop reaches the edge.
#[test]
fn a_fractional_scale_lays_out_in_logical_pixels_and_draws_in_device_ones() {
    scenario(
        "[overlay]\nat-startup = false\n[window]\nblur = false\nborder-width = 0.0\n\
         [[output]]\nname = \"pastel-1\"\nscale = 1.5\n",
        |state, display, client| {
            client.open("shown", RED);
            at_rest(state, display, 1);
            client.capture();
            settle(state, display, |_| client.captured().is_some());
            let captured = client.captured().expect("a capture");
            assert_eq!(
                (captured.width, captured.height),
                (WIDTH as u32, HEIGHT as u32)
            );
            let rect = state.scene.iter().rfind(|i| i.kind == SceneKind::Surface);
            let rect = rect.expect("the window").rect;
            assert!(
                rect.x + rect.width <= 854 && rect.y + rect.height <= 534,
                "{rect:?}"
            );
            let device = |logical: i32| (f64::from(logical) * 1.5).round() as u32;
            let [r, g, b] = [(RED >> 16) as u8, (RED >> 8) as u8, RED as u8];
            let is_red =
                |p: &[u8]| p[0].abs_diff(r) <= 3 && p[1].abs_diff(g) <= 3 && p[2].abs_diff(b) <= 3;
            let (left, right) = (device(rect.x), device(rect.x + rect.width));
            let (top, bottom) = (device(rect.y), device(rect.y + rect.height));
            let middle = (top + bottom) / 2;
            assert!(is_red(&captured.at(left, middle)));
            assert!(is_red(&captured.at(right - 1, middle)));
            assert!(is_red(&captured.at((left + right) / 2, bottom - 1)));
            assert!(!is_red(&captured.at(right, middle)));
            assert!(!is_red(&captured.at((left + right) / 2, bottom)));
        },
    );
}

/// A window that's gone still has its colour on the way out, with nothing of the
/// client left to ask.
#[test]
fn a_closed_window_fades_out_with_its_own_pixels() {
    scenario(
        "[overlay]\nat-startup = false\n[window]\nblur = false\nborder-width = 0.0\n\
         [animation]\nslowdown = 20.0",
        |state, display, client| {
            client.open("leaving", RED);
            at_rest(state, display, 1);
            let mut camera = Camera::new(state, display);
            camera.present(state);
            let rect = state.scene.iter().rfind(|i| i.kind == SceneKind::Surface);
            let rect = rect.expect("the window").rect;
            let (x, y) = (rect.x + rect.width / 2, rect.y + rect.height / 2);

            client.close(0);
            settle(state, display, |s| s.windows_mapped() == 0);
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
            assert!(
                state.scene.iter().any(|i| i.kind == SceneKind::Retained),
                "its pixels are in the scene"
            );
            camera.present(state);
            let shot = camera.pixels(state);
            let [r, g, b] = shot.at(x, y)[..3] else {
                panic!("a pixel");
            };
            assert!(
                r > g.saturating_add(60) && r > b.saturating_add(60),
                "still red: {r} {g} {b}"
            );
        },
    );
}
