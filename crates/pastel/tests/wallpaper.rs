//! A wallpaper from a client: real pixels with blur on, the way `swaybg` runs on a
//! real desktop. The blur has to sample the client's picture, not the `background`
//! colour behind it, however the two arrive and whichever goes first.
//!
//! Set PASTEL_SNAPSHOT_DIR to keep each frame as `<name>.rgba` (1280x800 RGBA8).

mod common;

use std::time::{Duration, Instant};

use common::{Client, Pattern};
use luminaria::backend::{BackendKind, BackendOptions, HeadlessBackend, Output};
use luminaria::core::Display;
use luminaria::shell::SceneKind;
use luminaria::util::{Color, Rect};
use pastel::app;
use pastel::state::State;
use pastel_config::Config;

const WIDTH: i32 = 1280;
const HEIGHT: i32 = 800;
const NO_OVERLAY: &str = "[overlay]\nat-startup = false";
const REAL_BLUR: &str = "[overlay]\nat-startup = false\n[window]\nblur-xray = false";
const WHITE: u32 = 0xffffffff;
const RED: u32 = 0xffe02020;
const GREEN: u32 = 0xff20e020;
const STRIPES: Pattern = Pattern::Stripes(WHITE, RED, 10);
/// Clear enough that what's behind it decides the colour.
const GLASS: u32 = 0x20101010;
/// Dark and bluish, so a blur of it can't pass for any wallpaper here.
const BG: Color = Color::rgb(0.05, 0.05, 0.3);

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

/// One output kept across frames, so what's cached between them is really tested.
struct Camera<'d> {
    _backend: HeadlessBackend<'d>,
    output: Output,
}

struct Shot(Vec<u8>);

impl Shot {
    fn at(&self, x: i32, y: i32) -> [u8; 3] {
        let i = (y * WIDTH + x) as usize * 4;
        self.0[i..i + 3].try_into().expect("three bytes")
    }

    /// The largest swing of green along a row, which is where the stripes differ.
    fn contrast(&self, xs: std::ops::Range<i32>) -> u8 {
        let row: Vec<u8> = xs.map(|x| self.at(x, HEIGHT / 2)[1]).collect();
        row.iter().max().expect("row") - row.iter().min().expect("row")
    }
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

    fn shoot(&mut self, state: &State<'_>, name: &str) -> Shot {
        let renderer = state.services.renderer;
        renderer
            .present(&mut self.output, &state.scene, BG)
            .expect("present");
        let pixels = renderer.read_back(&self.output).expect("read back");
        if let Ok(dir) = std::env::var("PASTEL_SNAPSHOT_DIR") {
            std::fs::write(format!("{dir}/{name}.rgba"), &pixels).expect("write snapshot");
        }
        Shot(pixels)
    }
}

fn settle(state: &mut State<'_>, display: &Display, mut done: impl FnMut(&State<'_>) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(state) {
        assert!(Instant::now() < deadline, "timed out");
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

/// `surfaces` client surfaces in the scene, and everything finished moving.
fn at_rest(state: &mut State<'_>, display: &Display, surfaces: usize) {
    app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    settle(state, display, |s| {
        s.timeout().is_none() && s.scene_has_surfaces(surfaces)
    });
    // One more turn for the last commit to reach the scene.
    for _ in 0..6 {
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

/// The window, which is the topmost surface: a wallpaper is at the bottom.
fn window(state: &State<'_>) -> Rect {
    let top = state
        .scene
        .iter()
        .rfind(|item| item.kind == SceneKind::Surface);
    top.expect("a window").rect
}

fn behind_the_window(state: &State<'_>) -> i32 {
    let window = window(state);
    window.x + window.width / 2
}

/// A window takes half the screen, so one of the sides is free.
fn beside_the_window(state: &State<'_>) -> i32 {
    if window(state).x > 200 {
        100
    } else {
        WIDTH - 100
    }
}

fn is_reddish(pixel: [u8; 3]) -> bool {
    // Red and white stripes average to pink; the background has no red at all.
    pixel[0] > 180 && pixel[0] > pixel[2]
}

#[test]
fn a_wallpaper_fills_the_output_and_reserves_nothing() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.wallpaper(Pattern::Solid(RED));
        at_rest(state, display, 1);
        assert_eq!(client.wallpaper_sizes(), [(WIDTH, HEIGHT)]);
        let monitor = &state.desktop.monitors()[0];
        assert_eq!(monitor.usable, monitor.rect, "a wallpaper takes no room");
        let mut camera = Camera::new(state, display);
        let shot = camera.shoot(state, "wallpaper");
        assert!(is_reddish(shot.at(10, 10)), "{:?}", shot.at(10, 10));
    });
}

fn blur_samples_the_wallpaper(config: &str, name: &str) {
    scenario(config, |state, display, client| {
        client.wallpaper(STRIPES);
        at_rest(state, display, 1);
        client.open("glass", GLASS);
        at_rest(state, display, 2);
        let (x, beside) = (behind_the_window(state), beside_the_window(state));
        let mut camera = Camera::new(state, display);
        let shot = camera.shoot(state, name);
        let sharp = shot.contrast(beside - 90..beside + 90);
        assert!(sharp > 100, "stripes outside the window: {sharp}");
        let soft = shot.contrast(x - 90..x + 90);
        assert!(
            u32::from(soft) * 3 < u32::from(sharp),
            "blur should smear them: {soft} vs {sharp}"
        );
        assert!(
            is_reddish(shot.at(x, HEIGHT / 2)),
            "{:?}",
            shot.at(x, HEIGHT / 2)
        );
    });
}

#[test]
fn xray_blur_samples_the_wallpaper() {
    blur_samples_the_wallpaper(NO_OVERLAY, "xray");
}

#[test]
fn real_blur_samples_the_wallpaper() {
    blur_samples_the_wallpaper(REAL_BLUR, "real");
}

#[test]
fn blur_follows_a_wallpaper_that_comes_and_goes() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("glass", GLASS);
        at_rest(state, display, 1);
        let x = behind_the_window(state);
        let mut camera = Camera::new(state, display);
        let bare = camera.shoot(state, "bare").at(x, HEIGHT / 2);
        assert!(
            bare[0] < 80 && bare[2] > bare[0],
            "the background: {bare:?}"
        );

        // swaybg from `autostart` easily loses the race with the first window.
        client.wallpaper(Pattern::Solid(RED));
        at_rest(state, display, 2);
        let red = camera.shoot(state, "late").at(x, HEIGHT / 2);
        assert!(is_reddish(red), "a late wallpaper: {red:?}");

        // Changing it is a kill and a restart.
        client.wallpaper(Pattern::Solid(GREEN));
        settle(state, display, |s| s.scene_has_surfaces(2));
        at_rest(state, display, 2);
        let green = camera.shoot(state, "changed").at(x, HEIGHT / 2);
        assert!(
            green[1] > 150 && green[1] > green[0],
            "a new one: {green:?}"
        );

        client.no_wallpaper();
        settle(state, display, |s| !s.scene_has_surfaces(2));
        at_rest(state, display, 1);
        let gone = camera.shoot(state, "gone").at(x, HEIGHT / 2);
        assert!(gone[0] < 80 && gone[2] > gone[0], "back to it: {gone:?}");
    });
}

/// The hotkey overlay floats over windows, so x-ray (wallpaper only) would blur
/// the wrong thing: whatever windows use, its blur is real.
#[test]
fn the_hotkey_overlay_blurs_the_windows_behind_it() {
    scenario("[window]\nblur-xray = true", |state, display, client| {
        client.open("behind", RED);
        at_rest(state, display, 1);
        let panel = state.scene.iter().find(|i| i.kind == SceneKind::Image);
        let Some(panel) = panel else {
            eprintln!("skipped: no font for the overlay");
            return;
        };
        assert!(panel.blur_enabled && !panel.blur_xray, "real blur");
        let window = state.scene.iter().rfind(|i| i.kind == SceneKind::Surface);
        assert!(window.expect("the window").blur_xray, "windows keep x-ray");
    });
}
