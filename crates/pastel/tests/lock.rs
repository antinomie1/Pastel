//! The session lock fails closed: while it holds, nothing but lock surfaces is in
//! the scene and nothing but them gets a key, whatever becomes of the lock client.

mod common;

use std::time::{Duration, Instant};

use common::Client;
use luminaria::backend::{BackendKind, BackendOptions, InputEvent, Modifiers};
use luminaria::core::Display;
use luminaria::shell::SceneKind;
use luminaria::util::{KeymapState, Keysym};
use pastel::app;
use pastel::state::State;
use pastel_config::Config;
use pastel_config::keys::LOGO;

const RED: u32 = 0xffd05050;
const KEY_A: u32 = 30;
const KEY_Q: u32 = 16;

fn scenario(body: impl for<'a> FnOnce(&mut State<'a>, &'a Display)) {
    let config = Config::parse("[overlay]\nat-startup = false").expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        let seat = state.services.globals.seat();
        seat.set_capabilities(true, true, false);
        let keymap = KeymapState::from_layout("us").expect("keymap").text();
        assert!(seat.set_keymap(&keymap));
        settle(state, display, |s| !s.desktop.monitors().is_empty());
        body(state, display);
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

fn turn(state: &mut State<'_>, display: &Display) {
    for _ in 0..20 {
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

fn press(state: &mut State<'_>, mods: u32, key: u32, keysym: Keysym) {
    let modifiers = Modifiers {
        depressed: mods,
        ..Modifiers::default()
    };
    for pressed in [true, false] {
        state.input(InputEvent::Key {
            key,
            pressed,
            keysym,
            modifiers,
        });
    }
}

fn surfaces(state: &State<'_>) -> usize {
    let surface = |item: &&luminaria::shell::SceneItem| item.kind == SceneKind::Surface;
    state.scene.iter().filter(surface).count()
}

fn window_up<'c>(
    state: &mut State<'_>,
    display: &Display,
    socket: &str,
    held: &'c mut Option<Client>,
) -> &'c Client {
    let client = held.insert(Client::connect(socket));
    client.open("private", RED);
    settle(state, display, |s| {
        s.windows_mapped() == 1 && s.scene_has_surfaces(1)
    });
    client
}

#[test]
fn locking_covers_everything_and_takes_the_keyboard() {
    scenario(|state, display| {
        let socket = state.socket().to_owned();
        let mut held = None;
        let window = window_up(state, display, &socket, &mut held);
        settle(state, display, |_| window.seen(0).activated);

        let locker = Client::connect(&socket);
        locker.lock();
        settle(state, display, |s| s.is_locked());
        settle(state, display, |_| locker.locked());
        assert_eq!(locker.lock_sizes()[0], (1280, 800));
        settle(state, display, |s| surfaces(s) == 1);
        assert!(
            !window.seen(0).activated,
            "a locked window keeps no keyboard"
        );

        press(state, 0, KEY_A, Keysym::new(0x61));
        settle(state, display, |_| locker.keys() == [KEY_A]);
        assert!(
            window.keys().is_empty(),
            "a password must reach only the lock"
        );

        // Close-window, which must not reach anything behind the lock.
        press(state, LOGO, KEY_Q, Keysym::new(0x71));
        turn(state, display);
        assert_eq!(window.seen(0).close_requests, 0);
        assert_eq!(locker.keys(), [KEY_A, KEY_Q], "bindings are off too");

        locker.unlock();
        settle(state, display, |s| !s.is_locked());
        settle(state, display, |_| window.seen(0).activated);
        settle(state, display, |s| surfaces(s) == 1); // the window, not the lock
    });
}

#[test]
fn a_lock_client_that_dies_leaves_the_screen_covered() {
    scenario(|state, display| {
        let socket = state.socket().to_owned();
        let mut held = None;
        let window = window_up(state, display, &socket, &mut held);

        let locker = Client::connect(&socket);
        locker.lock();
        settle(state, display, |_| locker.locked());
        settle(state, display, |s| surfaces(s) == 1);
        drop(locker);
        settle(state, display, |s| surfaces(s) == 0);
        turn(state, display);
        assert!(state.is_locked(), "a crash is not an unlock");
        assert_eq!(surfaces(state), 0, "the window stays hidden");
        assert!(!window.seen(0).activated);

        // A new locker can take over, and only it can let the session go.
        let second = Client::connect(&socket);
        second.lock();
        settle(state, display, |_| second.locked());
        settle(state, display, |s| surfaces(s) == 1);
        second.unlock();
        settle(state, display, |s| !s.is_locked());
        settle(state, display, |_| window.seen(0).activated);
    });
}

#[test]
fn a_window_opening_under_the_lock_stays_out_of_sight() {
    scenario(|state, display| {
        let socket = state.socket().to_owned();
        let locker = Client::connect(&socket);
        locker.lock();
        settle(state, display, |_| locker.locked());
        settle(state, display, |s| surfaces(s) == 1);
        let mut held = None;
        let window = held.insert(Client::connect(&socket));
        window.open("new", RED);
        settle(state, display, |s| s.windows_mapped() == 1);
        turn(state, display);
        assert_eq!(surfaces(state), 1);
        assert!(!window.seen(0).activated);
    });
}
