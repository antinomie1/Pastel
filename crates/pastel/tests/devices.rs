//! The input and display protocols that wait on the compositor to act: a locked
//! pointer that stays still while relative motion still flows, a tablet pen over
//! a window, a virtual keyboard with its own keymap, and gamma on an output
//! without a table (headless), which must say so.

mod common;

use std::time::{Duration, Instant};

use common::{Client, Command};
use luminaria::backend::{BackendKind, BackendOptions, InputEvent, Modifiers};
use luminaria::core::Display;
use luminaria::protocol::{TabletAxes, TabletDevice, TabletInput, TabletToolCaps, TabletToolType};
use luminaria::util::{KeymapState, Keysym};
use pastel::app;
use pastel::state::State;
use pastel_config::Config;

const KEY_A: u32 = 30;
const KEY_S: u32 = 31;

fn scenario(body: impl for<'a> FnOnce(&mut State<'a>, &'a Display, &Client)) {
    let config = Config::parse("[overlay]\nat-startup = false").expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        let seat = state.services.globals.seat();
        seat.set_capabilities(true, true, false);
        assert!(seat.set_keymap(&KeymapState::from_layout("us").expect("keymap").text()));
        let client = Client::connect(state.socket());
        settle(state, display, "an output", |s, _| {
            !s.desktop.monitors().is_empty()
        });
        client.open("app", 0xff5070d0);
        settle(state, display, "the window", |_, _| {
            client.seen(0).activated && client.seen(0).size.0 > 0
        });
        body(state, display, &client);
        Ok(())
    })
    .expect("compositor runs");
}

fn settle(
    state: &mut State<'_>,
    display: &Display,
    what: &str,
    mut done: impl FnMut(&State<'_>, ()) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(state, ()) {
        assert!(Instant::now() < deadline, "timed out waiting: {what}");
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

/// Puts the pointer in the middle of the window.
fn point_at_window(state: &mut State<'_>, display: &Display) {
    let rect = state
        .desktop
        .rect_of(state.desktop.focused_window().unwrap())
        .unwrap();
    let (x, y) = rect.center();
    state.input(InputEvent::MotionAbsolute { x, y, output: None });
    for _ in 0..3 {
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

#[test]
fn a_locked_pointer_stays_still_and_motion_stays_relative() {
    scenario(|state, display, client| {
        client.send(Command::LockPointer(0));
        point_at_window(state, display);
        settle(state, display, "the lock", |_, _| client.devices().locked);
        let motions = client.devices().pointer_motions;
        state.input(InputEvent::Motion { dx: 5.0, dy: -3.0 });
        settle(state, display, "relative motion", |_, _| {
            client.devices().relative == [(5.0, -3.0)]
        });
        for _ in 0..5 {
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }
        assert_eq!(
            client.devices().pointer_motions,
            motions,
            "a locked pointer moved"
        );
    });
}

#[test]
fn a_confined_pointer_stays_in_its_window() {
    scenario(|state, display, client| {
        client.send(Command::ConfinePointer(0));
        point_at_window(state, display);
        settle(state, display, "the confinement", |_, _| {
            client.devices().confined
        });
        let (w, h) = client.seen(0).size;
        for _ in 0..3 {
            state.input(InputEvent::Motion {
                dx: 5000.0,
                dy: 5000.0,
            });
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }
        settle(state, display, "a motion", |_, _| {
            client
                .devices()
                .last_motion
                .is_some_and(|(x, y)| x > f64::from(w) / 2.0 && y > f64::from(h) / 2.0)
        });
        let (x, y) = client.devices().last_motion.unwrap();
        assert!(
            x <= f64::from(w) && y <= f64::from(h),
            "the pointer left: {x},{y} of {w}x{h}"
        );
        // Leaving the window would have ended the confinement.
        assert!(client.devices().confined, "the pointer got out");
    });
}

#[test]
fn a_pen_over_a_window_reaches_it() {
    scenario(|state, display, client| {
        client.send(Command::WatchTablets);
        state.input(InputEvent::Tablet(TabletInput::DeviceAdded(TabletDevice {
            device: 1,
            name: "Pen tablet".into(),
            vid: 0x56a,
            pid: 0x100,
            bustype: 3,
            path: "/dev/input/event9".into(),
            pad: None,
        })));
        settle(state, display, "the tablet", |_, _| {
            client.devices().tablets == 1
        });
        // Where the window is, as a fraction of the monitor the tablet maps to.
        let monitor = state.desktop.monitors()[0].rect;
        let (x, y) = state
            .desktop
            .rect_of(state.desktop.focused_window().unwrap())
            .unwrap()
            .center();
        let at = ((x - monitor.x) / monitor.w, (y - monitor.y) / monitor.h);
        state.input(InputEvent::Tablet(TabletInput::Proximity {
            device: 1,
            tool: 7,
            inside: true,
            tool_type: TabletToolType::Pen,
            serial: 0,
            wacom_id: 0,
            caps: TabletToolCaps::default(),
            axes: TabletAxes::at(at.0, at.1),
            time_ms: 10,
        }));
        settle(state, display, "the pen", |_, _| {
            client.devices().tool_in == 1 && !client.devices().tool_motions.is_empty()
        });
        state.input(InputEvent::Tablet(TabletInput::Tip {
            device: 1,
            tool: 7,
            down: true,
            axes: TabletAxes::at(at.0, at.1),
            time_ms: 20,
        }));
        settle(state, display, "the tip", |_, _| {
            client.devices().tool_down == 1
        });
    });
}

#[test]
fn a_virtual_keyboard_types_with_its_own_keymap() {
    scenario(|state, display, client| {
        let keymaps = client.devices().keymaps;
        let german = KeymapState::from_layout("de").expect("de").text();
        client.send(Command::VirtualType(german, KEY_A));
        settle(state, display, "the typed key", |_, _| {
            client.keys() == [KEY_A]
        });
        assert!(
            client.devices().keymaps > keymaps,
            "the window was never told the virtual keymap"
        );
        let switched = client.devices().keymaps;
        // A real key: the seat's own keymap comes back first.
        for pressed in [true, false] {
            state.input(InputEvent::Key {
                key: KEY_S,
                pressed,
                keysym: Keysym::s,
                modifiers: Modifiers::default(),
            });
        }
        settle(state, display, "the real key", |_, _| {
            client.keys() == [KEY_A, KEY_S]
        });
        assert!(
            client.devices().keymaps > switched,
            "the seat kept the virtual keymap"
        );
    });
}

#[test]
fn gamma_on_an_output_without_a_table_fails() {
    scenario(|state, display, client| {
        client.send(Command::AskGamma);
        settle(state, display, "an answer", |_, _| {
            client.devices().gamma_failed || client.devices().gamma_size.is_some()
        });
        assert!(
            client.devices().gamma_failed,
            "headless has no gamma table: {:?}",
            client.devices()
        );
    });
}
