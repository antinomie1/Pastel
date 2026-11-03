//! ext-workspace-v1 with a real pager: one group per monitor on its wl_output,
//! even when the pager binds the output last; Pastel's dynamic workspaces as
//! they come and go; and a switch the pager asks for.

mod common;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::Client;
use luminaria::backend::{BackendKind, BackendOptions};
use luminaria::core::Display;
use pastel::app;
use pastel::state::State;
use pastel_config::Config;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, event_created_child};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1 as group, ext_workspace_handle_v1 as handle,
    ext_workspace_manager_v1 as manager,
};

#[derive(Clone, Debug, Default, PartialEq)]
struct Seen {
    groups: usize,
    output_enters: usize,
    /// (name, active) of each live workspace, in announcement order.
    workspaces: Vec<(String, bool)>,
}

#[derive(Default)]
struct Pager {
    seen: Seen,
    handles: Vec<(handle::ExtWorkspaceHandleV1, String, bool, bool)>,
}

impl Pager {
    fn snapshot(&mut self) -> Seen {
        self.seen.workspaces = self
            .handles
            .iter()
            .filter(|(_, _, _, removed)| !removed)
            .map(|(_, name, active, _)| (name.clone(), *active))
            .collect();
        self.seen.clone()
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Pager {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<wl_output::WlOutput, ()> for Pager {
    fn event(
        _: &mut Self,
        _: &wl_output::WlOutput,
        _: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<manager::ExtWorkspaceManagerV1, ()> for Pager {
    fn event(
        _: &mut Self,
        _: &manager::ExtWorkspaceManagerV1,
        _: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(Pager, manager::ExtWorkspaceManagerV1, [
        manager::EVT_WORKSPACE_GROUP_OPCODE => (group::ExtWorkspaceGroupHandleV1, ()),
        manager::EVT_WORKSPACE_OPCODE => (handle::ExtWorkspaceHandleV1, ()),
    ]);
}
impl Dispatch<group::ExtWorkspaceGroupHandleV1, ()> for Pager {
    fn event(
        pager: &mut Self,
        _: &group::ExtWorkspaceGroupHandleV1,
        event: group::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            group::Event::Capabilities { .. } => pager.seen.groups += 1,
            group::Event::OutputEnter { .. } => pager.seen.output_enters += 1,
            _ => {}
        }
    }
}
impl Dispatch<handle::ExtWorkspaceHandleV1, ()> for Pager {
    fn event(
        pager: &mut Self,
        proxy: &handle::ExtWorkspaceHandleV1,
        event: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let index = match pager.handles.iter().position(|(p, ..)| p == proxy) {
            Some(index) => index,
            None => {
                pager
                    .handles
                    .push((proxy.clone(), String::new(), false, false));
                pager.handles.len() - 1
            }
        };
        let entry = &mut pager.handles[index];
        match event {
            handle::Event::Name { name } => entry.1 = name,
            handle::Event::State {
                state: WEnum::Value(state),
            } => {
                entry.2 = state.contains(handle::State::Active);
            }
            handle::Event::Removed => entry.3 = true,
            _ => {}
        }
    }
}

enum Ask {
    Look,
    Activate(String),
}

fn pager(socket: String, asks: mpsc::Receiver<Ask>, answers: mpsc::Sender<Seen>) {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap();
    let stream = std::os::unix::net::UnixStream::connect(format!("{runtime}/{socket}")).unwrap();
    let conn = Connection::from_socket(stream).unwrap();
    let (globals, mut queue) = registry_queue_init::<Pager>(&conn).unwrap();
    let qh = queue.handle();
    let manager: manager::ExtWorkspaceManagerV1 = globals.bind(&qh, 1..=1, ()).unwrap();
    let mut pager = Pager::default();
    queue.roundtrip(&mut pager).unwrap();
    // The output only after the manager, as a registry usually orders them.
    let _output: wl_output::WlOutput = globals.bind(&qh, 1..=4, ()).unwrap();
    for ask in asks {
        if let Ask::Activate(name) = ask {
            let found = pager
                .handles
                .iter()
                .find(|(_, n, _, removed)| *n == name && !removed);
            found.unwrap().0.activate();
            manager.commit();
        }
        queue.roundtrip(&mut pager).unwrap();
        answers.send(pager.snapshot()).unwrap();
    }
}

#[test]
fn a_pager_follows_and_switches_pastels_workspaces() {
    let config = Config::parse("[overlay]\nat-startup = false").expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        let settle = |state: &mut State<'_>,
                      display: &Display,
                      what: &str,
                      done: &mut dyn FnMut(&State<'_>) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !done(state) {
                assert!(Instant::now() < deadline, "timed out waiting: {what}");
                app::step(state, display, Some(Duration::from_millis(5))).expect("step");
            }
        };
        settle(state, display, "an output", &mut |s| {
            !s.desktop.monitors().is_empty()
        });
        for _ in 0..5 {
            app::step(state, display, Some(Duration::from_millis(5))).expect("step");
        }

        let (ask, asks) = mpsc::channel();
        let (answer, answers) = mpsc::channel();
        let socket = state.socket().to_owned();
        std::thread::spawn(move || pager(socket, asks, answer));
        let look = |state: &mut State<'_>, request: Ask, want: &dyn Fn(&Seen) -> bool| -> Seen {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut request = Some(request);
            loop {
                ask.send(request.take().unwrap_or(Ask::Look)).unwrap();
                let seen = loop {
                    app::step(state, display, Some(Duration::from_millis(5))).expect("step");
                    if let Ok(seen) = answers.try_recv() {
                        break seen;
                    }
                };
                if want(&seen) {
                    return seen;
                }
                assert!(Instant::now() < deadline, "the pager saw {seen:?}");
            }
        };

        let first = look(state, Ask::Look, &|s| s.output_enters == 1);
        assert_eq!(first.groups, 1);
        assert_eq!(first.workspaces, [("1".to_owned(), true)]);

        // A window fills the workspace, and Pastel adds an empty one below.
        let client = Client::connect(state.socket());
        client.open("one", 0xffd05050);
        let two = look(state, Ask::Look, &|s| s.workspaces.len() == 2);
        assert_eq!(
            two.workspaces,
            [("1".to_owned(), true), ("2".to_owned(), false)]
        );

        let switched = look(state, Ask::Activate("2".into()), &|s| {
            s.workspaces.iter().any(|w| w == &("2".to_owned(), true))
        });
        assert!(
            !switched
                .workspaces
                .iter()
                .any(|w| w == &("1".to_owned(), true))
        );
        assert_eq!(state.desktop.monitors()[0].active, 1);
        Ok(())
    })
    .expect("compositor runs");
}
