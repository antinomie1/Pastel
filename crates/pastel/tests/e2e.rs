//! The whole compositor on the headless backend, with a real client and key events
//! going in the same way they do from a keyboard.

mod common;

use std::time::{Duration, Instant};

use common::Client;
use luminaria::Event;
use luminaria::backend::{BackendKind, BackendOptions, InputEvent, Modifiers, OutputEvent};
use luminaria::core::Display;
use luminaria::util::{KeymapState, Keysym};
use pastel::app;
use pastel::state::State;
use pastel_config::Config;
use pastel_config::keys::{LOGO, SHIFT};
use pastel_layout::WindowId;

const KEY_A: u32 = 30;
const KEY_H: u32 = 35;
const KEY_Q: u32 = 16;
const KEY_F: u32 = 33;
const KEY_1: u32 = 2;
const KEY_U: u32 = 22;
const RED: u32 = 0xffd05050;
const BLUE: u32 = 0xff5070d0;

/// Run a scenario on a fresh compositor.
fn scenario(config: &str, body: impl for<'a> FnOnce(&mut State<'a>, &'a Display, &Client)) {
    let config = Config::parse(config).expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        let seat = state.services.globals.seat();
        seat.set_capabilities(true, true, false);
        let keymap = KeymapState::from_layout("us").expect("keymap").text();
        assert!(seat.set_keymap(&keymap));
        let client = Client::connect(state.socket());
        settle(state, display, "output arrives", |s| {
            !s.desktop.monitors().is_empty()
        });
        body(state, display, &client);
        Ok(())
    })
    .expect("compositor runs");
}

/// Run the loop until `done`, or fail saying what never happened.
fn settle(
    state: &mut State<'_>,
    display: &Display,
    what: &str,
    mut done: impl FnMut(&State<'_>) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(state) {
        assert!(Instant::now() < deadline, "timed out waiting: {what}");
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
    }
}

fn press(state: &mut State<'_>, mods: u32, key: u32, keysym: Keysym) {
    let modifiers = Modifiers {
        depressed: mods,
        ..Modifiers::default()
    };
    state.input(InputEvent::Key {
        key,
        pressed: true,
        keysym,
        modifiers,
    });
    state.input(InputEvent::Key {
        key,
        pressed: false,
        keysym,
        modifiers,
    });
}

fn window_count(state: &State<'_>) -> usize {
    state
        .desktop
        .monitors()
        .iter()
        .flat_map(|m| &m.workspaces)
        .map(|w| w.layout.len())
        .sum()
}

fn wait_for_windows(state: &mut State<'_>, display: &Display, count: usize) {
    settle(state, display, "windows", |s| {
        window_count(s) == count
            && s.desktop.monitors()[0].workspaces.iter().all(|w| {
                let mut placed = Vec::new();
                w.layout.place(&mut placed);
                placed
                    .iter()
                    .all(|p| s.windows.get(p.id).is_some_and(|w| w.mapped))
            })
    });
}

#[test]
fn a_window_gets_half_the_screen_and_the_keyboard() {
    scenario("", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        settle(state, display, "activated", |_| client.seen(0).activated);
        // 1280x800 with 12px gaps: (1280 - 12) / 2 - 12.
        assert_eq!(client.seen(0).size, (622, 776));
    });
}

#[test]
fn a_second_window_takes_focus_without_resizing_the_first() {
    scenario("", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        client.open("two", BLUE);
        wait_for_windows(state, display, 2);
        settle(state, display, "focus moved", |_| {
            client.seen(1).activated && !client.seen(0).activated
        });
        assert_eq!(client.seen(0).size, client.seen(1).size);
    });
}

#[test]
fn bindings_move_focus_and_never_reach_clients() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        client.open("two", BLUE);
        wait_for_windows(state, display, 2);
        settle(state, display, "focus", |_| client.seen(1).activated);
        press(state, LOGO, KEY_H, Keysym::h);
        settle(state, display, "focus left", |_| client.seen(0).activated);
        assert!(!client.seen(1).activated);
        assert!(
            client.keys().is_empty(),
            "the binding leaked: {:?}",
            client.keys()
        );
    });
}

#[test]
fn unbound_keys_go_to_the_focused_client() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        settle(state, display, "focus", |_| client.seen(0).activated);
        press(state, 0, KEY_A, Keysym::a);
        settle(state, display, "key", |_| client.keys() == [KEY_A]);
    });
}

#[test]
fn the_overlay_takes_the_first_key_and_lets_go() {
    scenario("", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        settle(state, display, "focus", |_| client.seen(0).activated);
        press(state, 0, KEY_A, Keysym::a);
        press(state, 0, KEY_A, Keysym::a);
        settle(state, display, "second key", |_| client.keys() == [KEY_A]);
    });
}

#[test]
fn shifted_bindings_match_the_unshifted_key() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        let monitor = |s: &State<'_>| s.desktop.monitors()[0].active;
        press(state, LOGO, KEY_U, Keysym::u);
        settle(state, display, "workspace 2", |s| monitor(s) == 1);
        // Shift+1 really produces `!`; the binding says `Mod+Shift+1`.
        client.open("two", BLUE);
        wait_for_windows(state, display, 2);
        press(state, LOGO | SHIFT, KEY_1, Keysym::exclam);
        settle(state, display, "moved to workspace 1", |s| {
            monitor(s) == 0 && s.desktop.monitors()[0].active_workspace().layout.len() == 2
        });
    });
}

#[test]
fn closing_asks_the_client_and_its_window_leaves_when_it_goes() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        press(state, LOGO, KEY_Q, Keysym::q);
        settle(state, display, "close request", |_| {
            client.seen(0).close_requests == 1
        });
        assert_eq!(window_count(state), 1, "the client decides when it's gone");
        client.close(0);
        settle(state, display, "window gone", |s| window_count(s) == 0);
        assert_eq!(state.desktop.focused_window(), None);
    });
}

#[test]
fn fullscreen_is_requested_of_the_client_at_the_screen_size() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        press(state, LOGO | SHIFT, KEY_F, Keysym::f);
        settle(state, display, "fullscreen", |_| client.seen(0).fullscreen);
        assert_eq!(client.seen(0).size, (1280, 800));
        press(state, LOGO | SHIFT, KEY_F, Keysym::f);
        settle(state, display, "windowed", |_| !client.seen(0).fullscreen);
        assert_eq!(client.seen(0).size, (622, 776));
    });
}

#[test]
fn nothing_moving_means_no_timer_at_all() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        settle(state, display, "animations finish", |s| {
            s.timeout().is_none()
        });
        client.close(0);
        settle(state, display, "ghost appears", |s| s.timeout().is_some());
        settle(state, display, "ghost fades", |s| s.timeout().is_none());
        assert!(window_id_unknown(state));
    });
}

/// Rounded and translucent windows go through an offscreen group. Those must stop
/// producing frames when nothing changes, or the screen never idles.
#[test]
fn a_still_screen_presents_nothing_even_with_rounded_translucent_windows() {
    let config = "[overlay]\nat-startup = false\n[window]\nopacity = 0.8";
    scenario(config, |state, display, client| {
        client.open("one", RED);
        client.open("two", BLUE);
        wait_for_windows(state, display, 2);
        settle(state, display, "animations finish", |s| {
            s.timeout().is_none()
        });
        // What's in flight when the last animation ends.
        for _ in 0..30 {
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }
        let mut frames = 0;
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            for event in display
                .dispatch(Some(Duration::from_millis(5)))
                .expect("dispatch")
            {
                frames += usize::from(matches!(event, Event::Output(_, OutputEvent::Present(_))));
                state.handle(event);
            }
            state.frame();
        }
        assert_eq!(frames, 0, "an idle screen must not present");
    });
}

fn window_id_unknown(state: &State<'_>) -> bool {
    !state.windows.contains(WindowId(0)) && window_count(state) == 0
}

#[test]
fn animations_can_be_switched_off() {
    let config = "[overlay]\nat-startup = false\n[animation]\nenabled = false";
    scenario(config, |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        assert_eq!(state.timeout(), None);
    });
}
