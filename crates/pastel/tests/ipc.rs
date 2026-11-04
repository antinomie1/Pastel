//! The IPC socket with a real client: a bar subscribes and sees windows and
//! workspaces change, then drives the compositor through it.

mod common;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::Client;
use luminaria::backend::{BackendKind, BackendOptions};
use luminaria::core::Display;
use pastel::app;
use pastel::state::State;
use pastel_config::Config;
use pastel_ipc::{Reply, Request};

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

#[test]
fn a_bar_follows_the_desktop_and_drives_it() {
    let config = Config::parse("[overlay]\nat-startup = false").expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        let path = pastel_ipc::socket_for(state.socket()).expect("runtime dir");
        assert_eq!(
            std::env::var_os(pastel_ipc::SOCKET_ENV),
            Some(path.clone().into())
        );
        settle(state, display, "an output", |s| {
            !s.desktop.monitors().is_empty()
        });

        let (states, received) = mpsc::channel();
        let subscriber = path.clone();
        std::thread::spawn(move || {
            let client = pastel_ipc::Client::connect_to(&subscriber).expect("connect");
            for state in client.subscribe().expect("subscribe") {
                if states.send(state.expect("state")).is_err() {
                    return;
                }
            }
        });
        let mut latest = None;
        let mut pump =
            |state: &mut State<'_>, what: &str, want: &dyn Fn(&pastel_ipc::State) -> bool| {
                settle(state, display, what, |_| {
                    while let Ok(next) = received.try_recv() {
                        latest = Some(next);
                    }
                    latest.as_ref().is_some_and(want)
                });
                latest.clone().unwrap()
            };
        let first = pump(state, "the first state", &|s| !s.workspaces.is_empty());
        assert_eq!(first.outputs.len(), 1);
        assert!(first.workspaces[0].focused);
        assert!(
            !first.keyboard_layout.is_empty(),
            "the keyboard layout is named"
        );

        let client = Client::connect(state.socket());
        client.open("one", 0xffd05050);
        client.open("two", 0xff5070d0);
        let two = pump(state, "both windows", &|s| s.windows.len() == 2);
        assert_eq!(
            two.windows
                .iter()
                .map(|w| w.app_id.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert!(two.windows[1].focused);
        assert_eq!(two.workspaces[0].windows, 2);

        let request = |request: Request| {
            let path = path.clone();
            let (done, reply) = mpsc::channel();
            std::thread::spawn(move || {
                let mut client = pastel_ipc::Client::connect_to(&path).expect("connect");
                done.send(client.request(&request).expect("reply")).unwrap();
            });
            reply
        };
        let reply = request(Request::FocusWindow(two.windows[0].id));
        let focused = pump(state, "focus moves", &|s| s.windows[0].focused);
        assert!(!focused.windows[1].focused);
        assert_eq!(
            reply.recv_timeout(Duration::from_secs(1)).unwrap(),
            Reply::Ok
        );

        let reply = request(Request::Action("nonsense".into()));
        settle(state, display, "an error", |_| {
            reply.try_recv().is_ok_and(|r| matches!(r, Reply::Error(_)))
        });
        Ok(())
    })
    .expect("compositor runs");
}

#[test]
fn layout_names_come_from_the_symbols_in_group_order() {
    let keymap = r#"xkb_keymap {
xkb_types "complete" { level_name[Level1] = "Base"; };
xkb_symbols "pc+us+ru:2" {
    name[Group2]="Russian";
    name[Group1]="English (US)";
    key <AE01> { [ 1, exclam ] };
};
};"#;
    assert_eq!(
        pastel::input::layout_names(keymap),
        ["English (US)", "Russian"]
    );
    assert!(pastel::input::layout_names("").is_empty());
}
