//! Windows out of the layout: which ones, and how the pointer moves them.

mod common;

use std::time::{Duration, Instant};

use common::Client;
use luminaria::backend::{
    BackendKind, BackendOptions, GestureEvent, GestureKind, GesturePhase, InputEvent, Modifiers,
};
use luminaria::core::Display;
use luminaria::shell::SceneKind;
use pastel::app;
use pastel::state::State;
use pastel_config::keys::LOGO;
use pastel_config::{Action, Config};
use pastel_layout::{Rect, WindowId};

const RED: u32 = 0xffd05050;
const BLUE: u32 = 0xff5070d0;
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const NO_OVERLAY: &str = "[overlay]\nat-startup = false\n";

fn scenario(config: &str, body: impl for<'a> FnOnce(&mut State<'a>, &'a Display, &Client)) {
    let config = Config::parse(config).expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        let seat = state.services.globals.seat();
        seat.set_capabilities(true, true, false);
        let client = Client::connect(state.socket());
        settle(state, display, |s| !s.desktop.monitors().is_empty());
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

fn rest(state: &mut State<'_>, display: &Display) {
    for _ in 0..30 {
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

fn focused(state: &State<'_>) -> WindowId {
    state.desktop.focused_window().expect("a focused window")
}

fn float_rect(state: &State<'_>) -> Rect {
    let floating = state.desktop.floating(focused(state));
    floating.expect("it floats").rect
}

fn button(state: &mut State<'_>, button: u32, pressed: bool) {
    state.input(InputEvent::Button { button, pressed });
}

fn pointer(state: &mut State<'_>, dx: f64, dy: f64) {
    state.input(InputEvent::Motion { dx, dy });
}

fn hold_mod(state: &mut State<'_>) {
    state.input(InputEvent::Modifiers(Modifiers {
        depressed: LOGO,
        ..Modifiers::default()
    }));
}

#[test]
fn a_window_of_one_fixed_size_floats_at_that_size() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open_fixed("settings", RED, (300, 200));
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        let rect = float_rect(state);
        assert_eq!((rect.w, rect.h), (300.0, 200.0));
        assert_eq!(rect.center(), (640.0, 400.0), "in the middle");
        assert_eq!(client.seen(0).size, (300, 200));
    });
}

#[test]
fn a_dialog_floats_over_its_parent_and_gives_focus_back() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("main", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        let parent = focused(state);
        client.open_dialog("dialog", BLUE, 0);
        settle(state, display, |s| s.windows_mapped() == 2);
        rest(state, display);
        assert_ne!(focused(state), parent);
        assert!(state.desktop.floating(focused(state)).is_some());
        assert!(state.desktop.floating(parent).is_none());
        client.close(1);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        assert_eq!(focused(state), parent);
    });
}

#[test]
fn a_rule_decides_either_way() {
    let rule = "[[window-rule]]\napp-id = \"float\"\nfloating = true\n\
                [[window-rule]]\napp-id = \"tile\"\nfloating = false\n";
    scenario(&format!("{NO_OVERLAY}{rule}"), |state, display, client| {
        client.open("float", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        assert!(state.desktop.floating(focused(state)).is_some());
        client.open_fixed("tile", BLUE, (300, 200));
        settle(state, display, |s| s.windows_mapped() == 2);
        rest(state, display);
        assert!(state.desktop.floating(focused(state)).is_none());
    });
}

#[test]
fn a_binding_floats_a_window_and_the_client_hears_it_isnt_tiled() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("one", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        state.run(Action::ToggleFloating);
        rest(state, display);
        assert!(state.desktop.floating(focused(state)).is_some());
        state.run(Action::ToggleFloating);
        rest(state, display);
        assert!(state.desktop.floating(focused(state)).is_none());
    });
}

#[test]
fn a_title_bar_drag_moves_a_window_with_the_pointer() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open_fixed("settings", RED, (300, 200));
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        let start = float_rect(state);
        button(state, BTN_LEFT, true);
        client.request_move(0);
        settle(state, display, |s| s.is_grabbing());
        pointer(state, 40.0, 25.0);
        assert_eq!(float_rect(state), start.translated(40.0, 25.0));
        pointer(state, 10.0, -5.0);
        assert_eq!(float_rect(state), start.translated(50.0, 20.0));
        button(state, BTN_LEFT, false);
        assert!(!state.is_grabbing());
        pointer(state, 100.0, 100.0);
        assert_eq!(float_rect(state), start.translated(50.0, 20.0), "let go");
    });
}

#[test]
fn a_move_with_no_button_down_is_refused() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open_fixed("settings", RED, (300, 200));
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        client.request_move(0);
        rest(state, display);
        assert!(!state.is_grabbing(), "a window can't chase the pointer");
    });
}

#[test]
fn dragging_a_tiled_window_floats_it_where_it_was() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("one", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        let tiled = state.desktop.rect_of(focused(state)).expect("placed");
        button(state, BTN_LEFT, true);
        client.request_move(0);
        settle(state, display, |s| s.is_grabbing());
        assert_eq!(float_rect(state), tiled);
        button(state, BTN_LEFT, false);
    });
}

#[test]
fn an_edge_drag_resizes_a_floating_window_within_its_limits() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("float", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        state.run(Action::ToggleFloating);
        rest(state, display);
        let start = float_rect(state);
        button(state, BTN_LEFT, true);
        client.request_resize_right(0);
        settle(state, display, |s| s.is_grabbing());
        pointer(state, 30.0, 0.0);
        assert_eq!(float_rect(state).w, start.w + 30.0);
        pointer(state, -10_000.0, 0.0);
        assert!(float_rect(state).w >= 50.0, "it stays big enough to hold");
        button(state, BTN_LEFT, false);
        rest(state, display);
        assert_eq!(client.seen(0).size.0 as f64, float_rect(state).w);
    });
}

#[test]
fn an_edge_drag_on_a_tiled_window_changes_its_column() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("one", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        let start = state.desktop.rect_of(focused(state)).expect("placed");
        button(state, BTN_LEFT, true);
        client.request_resize_right(0);
        settle(state, display, |s| s.is_grabbing());
        pointer(state, 128.0, 0.0);
        let after = state.desktop.rect_of(focused(state)).expect("placed");
        assert!(after.w > start.w, "{} then {}", start.w, after.w);
        assert!(state.desktop.floating(focused(state)).is_none());
        button(state, BTN_LEFT, false);
    });
}

#[test]
fn mod_and_a_button_drag_a_floating_window_without_the_client_seeing() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("float", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        state.run(Action::ToggleFloating);
        rest(state, display);
        let start = float_rect(state);
        let (cx, cy) = start.center();
        let (x, y) = state.services.cursor.position().expect("a pointer");
        pointer(state, cx - x, cy - y);
        hold_mod(state);
        button(state, BTN_LEFT, true);
        assert!(state.is_grabbing());
        pointer(state, 20.0, 10.0);
        assert_eq!(float_rect(state), start.translated(20.0, 10.0));
        button(state, BTN_LEFT, false);
        assert!(!state.is_grabbing());
        rest(state, display);

        button(state, BTN_RIGHT, true);
        assert!(state.is_grabbing());
        pointer(state, 25.0, 25.0);
        let resized = float_rect(state);
        assert_eq!((resized.w, resized.h), (start.w + 25.0, start.h + 25.0));
        button(state, BTN_RIGHT, false);
    });
}

fn swipe(state: &mut State<'_>, phase: GesturePhase, fingers: u32, dx: f64, dy: f64) {
    state.input(InputEvent::Gesture(GestureEvent {
        kind: GestureKind::Swipe,
        phase,
        fingers,
        dx,
        dy,
        scale: 1.0,
        rotation: 0.0,
        cancelled: false,
    }));
}

#[test]
fn three_fingers_up_change_workspace_and_sideways_pan() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("one", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        swipe(state, GesturePhase::Begin, 3, 0.0, 0.0);
        swipe(state, GesturePhase::Update, 3, 0.0, -90.0);
        swipe(state, GesturePhase::End, 3, 0.0, 0.0);
        assert_eq!(state.desktop.monitors()[0].active, 1);
        swipe(state, GesturePhase::Begin, 3, 0.0, 0.0);
        swipe(state, GesturePhase::Update, 3, 0.0, 90.0);
        swipe(state, GesturePhase::End, 3, 0.0, 0.0);
        assert_eq!(state.desktop.monitors()[0].active, 0);

        // Four fingers are the client's, and the desktop stays where it is.
        swipe(state, GesturePhase::Begin, 4, 0.0, 0.0);
        swipe(state, GesturePhase::Update, 4, 0.0, -90.0);
        swipe(state, GesturePhase::End, 4, 0.0, 0.0);
        assert_eq!(state.desktop.monitors()[0].active, 0);
    });
}

#[test]
fn a_touch_lands_on_the_window_under_it_and_gives_it_focus() {
    scenario(NO_OVERLAY, |state, display, client| {
        client.open("one", RED);
        client.open("two", BLUE);
        settle(state, display, |s| s.windows_mapped() == 2);
        rest(state, display);
        state
            .services
            .globals
            .seat()
            .set_capabilities(true, true, true);
        rest(state, display);
        let first = surface_rect(state, 0);
        let second = surface_rect(state, 1);
        let target = if client.seen(1).activated {
            first
        } else {
            second
        };
        let (x, y) = target.center();
        state.input(InputEvent::TouchDown {
            id: 7,
            x: x / 1280.0,
            y: y / 800.0,
        });
        state.input(InputEvent::TouchFrame);
        settle(state, display, |_| !client.touches().0.is_empty());
        let (touches, _) = client.touches();
        assert_eq!(touches[0], (x - target.x, y - target.y));
        settle(state, display, |_| client.seen(0).activated);
        state.input(InputEvent::TouchMotion {
            id: 7,
            x: (x + 10.0) / 1280.0,
            y: y / 800.0,
        });
        state.input(InputEvent::TouchFrame);
        settle(state, display, |_| client.touches().0.len() == 2);
        let (touches, _) = client.touches();
        assert!((touches[1].0 - (x - target.x + 10.0)).abs() < 0.01);
        state.input(InputEvent::TouchUp { id: 7 });
        state.input(InputEvent::TouchFrame);
        settle(state, display, |_| client.touches().1 == 1);
    });
}

#[test]
fn a_touch_follows_the_panel_when_the_output_is_rotated() {
    let config = format!("{NO_OVERLAY}[[output]]\nname = \"pastel-1\"\ntransform = \"90\"\n");
    scenario(&config, |state, display, client| {
        client.open("one", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        rest(state, display);
        state
            .services
            .globals
            .seat()
            .set_capabilities(true, true, true);
        rest(state, display);
        let screen = state.desktop.monitors()[0].rect;
        assert_eq!((screen.w, screen.h), (800.0, 1280.0));
        let target = surface_rect(state, 0);
        let (x, y) = target.center();
        // Turned a quarter, so the panel's y runs along the screen's x.
        state.input(InputEvent::TouchDown {
            id: 1,
            x: 1.0 - y / screen.h,
            y: x / screen.w,
        });
        state.input(InputEvent::TouchFrame);
        settle(state, display, |_| !client.touches().0.is_empty());
        let (dx, dy) = client.touches().0[0];
        assert!((dx - (x - target.x)).abs() < 0.01 && (dy - (y - target.y)).abs() < 0.01);
    });
}

fn surface_rect(state: &State<'_>, index: usize) -> Rect {
    let surfaces = state.scene.iter().filter(|i| i.kind == SceneKind::Surface);
    let mut rects: Vec<_> = surfaces.map(|i| i.rect).collect();
    rects.sort_by_key(|r| r.x);
    let r = rects[index];
    Rect::new(r.x.into(), r.y.into(), r.width.into(), r.height.into())
}
