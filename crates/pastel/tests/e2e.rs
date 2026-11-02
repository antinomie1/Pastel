//! The whole compositor on the headless backend, with a real client and key events
//! going in the same way they do from a keyboard.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
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

/// Without fifo-v1 SDL3 prefers X11 whenever there is one, and a client that only
/// draws to Wayland surfaces then can't start at all.
#[test]
fn the_protocols_sdl_needs_to_prefer_wayland_are_offered() {
    scenario("", |state, display, client| {
        client.open("one", RED);
        wait_for_windows(state, display, 1);
        let globals = client.report.lock().expect("report").globals.clone();
        for wanted in [
            "wp_fifo_manager_v1",
            "wp_cursor_shape_manager_v1",
            "wp_commit_timing_manager_v1",
        ] {
            assert!(
                globals.iter().any(|g| g == wanted),
                "{wanted} not in {globals:?}"
            );
        }
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
fn idle_is_reported_and_input_ends_it() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.watch_idle(30);
        settle(state, display, "idled", |_| client.idle() == [true]);
        press(state, 0, KEY_A, Keysym::new(0x61));
        settle(state, display, "resumed", |_| {
            client.idle() == [true, false]
        });
    });
}

#[test]
fn a_taskbar_can_see_focus_and_close_windows() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.watch_windows();
        client.open("one", RED);
        client.open("two", BLUE);
        wait_for_windows(state, display, 2);
        settle(state, display, "it hears of both", |_| {
            client.taskbar().len() == 2
        });
        assert_eq!(client.taskbar(), ["one", "two"]);
        settle(state, display, "the second has focus", |_| {
            client.seen(1).activated
        });
        client.taskbar_activate(0);
        settle(state, display, "the first has focus", |_| {
            client.seen(0).activated
        });
        client.taskbar_close(1);
        settle(state, display, "the second is asked to close", |_| {
            client.seen(1).close_requests == 1
        });
    });
}

/// A launcher, or a link in the window you're using, can raise another window.
#[test]
fn the_focused_window_can_activate_another() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        client.open("two", BLUE);
        wait_for_windows(state, display, 2);
        settle(state, display, "the second gets focus", |_| {
            client.seen(1).activated
        });
        client.activate(1, 0);
        settle(state, display, "the first is raised", |_| {
            client.seen(0).activated && !client.seen(1).activated
        });
    });
}

/// A window in the background can't take the keyboard from what you're typing in.
#[test]
fn an_unfocused_window_cannot_steal_focus() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.open("one", RED);
        client.open("two", BLUE);
        wait_for_windows(state, display, 2);
        settle(state, display, "the second gets focus", |_| {
            client.seen(1).activated
        });
        client.activate(0, 0);
        let until = Instant::now() + Duration::from_millis(150);
        while Instant::now() < until {
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }
        assert!(client.seen(1).activated && !client.seen(0).activated);
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
    let config = "[overlay]\nat-startup = false\n[window]\nopacity = 0.8\ncorner-radius = 12.0";
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

/// A compositor reading its config from a file `body` can rewrite and reload.
fn reloadable(first: &str, body: impl for<'a> FnOnce(&mut State<'a>, &'a Display, &dyn Fn(&str))) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let name = format!(
        "pastel-reload-{}-{}.toml",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let path = std::env::temp_dir().join(name);
    let write = |text: &str| {
        let text = format!("{text}\n[overlay]\nat-startup = false\n");
        std::fs::write(&path, text).expect("write config");
    };
    write(first);
    let config = Config::load(&path).expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, Some(path.clone()), |state, display| {
        settle(state, display, "output arrives", |s| {
            !s.desktop.monitors().is_empty()
        });
        body(state, display, &write);
        Ok(())
    })
    .expect("compositor runs");
    let _ = std::fs::remove_file(&path);
}

/// A reload that changes `background` repaints; one that doesn't stays idle.
#[test]
fn reloading_a_new_background_repaints_and_the_same_one_stays_idle() {
    reloadable("background = \"#102030\"", |state, display, write| {
        let presents = |state: &mut State<'_>| {
            let mut frames = 0;
            let until = Instant::now() + Duration::from_millis(200);
            while Instant::now() < until {
                for event in display
                    .dispatch(Some(Duration::from_millis(5)))
                    .expect("dispatch")
                {
                    let shown = matches!(event, Event::Output(_, OutputEvent::Present(_)));
                    frames += usize::from(shown);
                    state.handle(event);
                }
                state.frame();
            }
            frames
        };
        presents(state);
        state.reload_config();
        assert_eq!(presents(state), 0, "the same config must stay idle");
        write("background = \"#a03020\"");
        state.reload_config();
        // Every buffer in the rotation has the old colour, and owes the new one.
        let frames = presents(state);
        assert!(
            (1..=2).contains(&frames),
            "a full repaint, not {frames} frames"
        );
        assert_eq!(presents(state), 0, "and then idle again");
    });
}

#[test]
fn saving_the_config_reloads_it_with_no_binding_pressed() {
    reloadable("[input]\nlayout = \"de\"", |state, display, write| {
        // Nothing else on the way in, or the loop would go round for other reasons.
        write("[input]\nlayout = \"fr\"");
        settle(state, display, "the reload", |s| {
            symbols(s).contains("_fr_")
        });
    });
}

/// An editor saving to a side file and renaming it over the config.
#[test]
fn replacing_the_config_reloads_it_too() {
    reloadable("[input]\nlayout = \"de\"", |state, display, _| {
        let path = state.config_path().expect("a path").to_owned();
        let side = path.with_extension("swp");
        std::fs::write(&side, "[input]\nlayout = \"fr\"\n").expect("write");
        std::fs::rename(&side, &path).expect("rename");
        settle(state, display, "the reload", |s| {
            symbols(s).contains("_fr_")
        });
    });
}

fn screen(state: &State<'_>) -> (f64, f64) {
    let monitor = &state.desktop.monitors()[0];
    (monitor.rect.w, monitor.rect.h)
}

#[test]
fn an_output_section_applies_at_startup_and_on_reload() {
    let scaled = "[[output]]\nname = \"pastel-1\"\nscale = 2";
    reloadable(scaled, |state, display, write| {
        settle(state, display, "scale", |s| screen(s) == (640.0, 400.0));
        write("[[output]]\nname = \"pastel-1\"\ntransform = \"90\"\nscale = 1");
        settle(state, display, "rotation", |s| screen(s) == (800.0, 1280.0));
        write("[[output]]\nname = \"no-such-output\"\nscale = 3");
        for _ in 0..20 {
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }
        assert_eq!(
            screen(state),
            (800.0, 1280.0),
            "other outputs are left alone"
        );
    });
}

#[test]
fn an_output_client_tests_and_applies_a_scale() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.randr(2.0, true);
        settle(state, display, "the test", |_| {
            client.randr_answers() == [true]
        });
        assert_eq!(client.heads(), ["pastel-1"]);
        assert_eq!(screen(state), (1280.0, 800.0), "a test changes nothing");

        client.randr(2.0, false);
        settle(state, display, "the apply", |_| {
            client.randr_answers().len() == 2
        });
        assert_eq!(client.randr_answers(), [true, true]);
        settle(state, display, "the new size", |s| {
            screen(s) == (640.0, 400.0)
        });
    });
}

#[test]
fn an_output_client_is_told_what_cant_be_done() {
    scenario("[overlay]\nat-startup = false", |state, display, client| {
        client.randr(1.5, false);
        settle(state, display, "the refusal", |_| {
            client.randr_answers() == [false]
        });
        assert_eq!(screen(state), (1280.0, 800.0));
    });
}

fn symbols(state: &State<'_>) -> String {
    let keymap = state.services.globals.seat().keymap();
    let line = keymap.lines().find(|l| l.contains("xkb_symbols"));
    line.expect("a keymap with symbols").to_owned()
}

#[test]
fn the_keyboard_layout_comes_from_the_config_and_reloads() {
    reloadable("[input]\nlayout = \"de\"", |state, _, write| {
        assert!(symbols(state).contains("_de_"), "{}", symbols(state));
        write("[input]\nlayout = \"fr\"\noptions = \"ctrl:nocaps\"");
        state.reload_config();
        assert!(symbols(state).contains("_fr_"), "{}", symbols(state));
        assert!(
            symbols(state).contains("ctrl(nocaps)"),
            "{}",
            symbols(state)
        );
        write("[input]\nlayout = \"no-such-layout\"");
        state.reload_config();
        assert!(symbols(state).contains("_fr_"), "keeps the last good one");
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
