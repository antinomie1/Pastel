//! Desktop protocols from luminaria that Pastel offers, and the policy that is
//! Pastel's: an input method holding the keyboard grab gets the keys bindings
//! pass on, and what it types back through a virtual keyboard reaches the window.

mod common;

use std::os::fd::{AsFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::Client;
use luminaria::backend::{BackendKind, BackendOptions, InputEvent, Modifiers};
use luminaria::core::Display;
use luminaria::util::{KeymapState, Keysym};
use pastel::app;
use pastel::state::State;
use pastel_config::Config;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_keyboard_grab_v2 as grab, zwp_input_method_manager_v2 as im_manager,
    zwp_input_method_v2 as im,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1 as vk_manager, zwp_virtual_keyboard_v1 as vk,
};

const KEY_A: u32 = 30;
const KEY_B: u32 = 48;
const KEY_H: u32 = 35;

#[derive(Default)]
struct Ime {
    keymap: Option<(OwnedFd, u32)>,
    grabbed: Vec<u32>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Ime {
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
impl Dispatch<wl_seat::WlSeat, ()> for Ime {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<im_manager::ZwpInputMethodManagerV2, ()> for Ime {
    fn event(
        _: &mut Self,
        _: &im_manager::ZwpInputMethodManagerV2,
        _: im_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<im::ZwpInputMethodV2, ()> for Ime {
    fn event(
        _: &mut Self,
        _: &im::ZwpInputMethodV2,
        _: im::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<grab::ZwpInputMethodKeyboardGrabV2, ()> for Ime {
    fn event(
        ime: &mut Self,
        _: &grab::ZwpInputMethodKeyboardGrabV2,
        event: grab::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            grab::Event::Keymap { fd, size, .. } => ime.keymap = Some((fd, size)),
            grab::Event::Key {
                key,
                state: WEnum::Value(wayland_client::protocol::wl_keyboard::KeyState::Pressed),
                ..
            } => ime.grabbed.push(key),
            _ => {}
        }
    }
}
impl Dispatch<vk_manager::ZwpVirtualKeyboardManagerV1, ()> for Ime {
    fn event(
        _: &mut Self,
        _: &vk_manager::ZwpVirtualKeyboardManagerV1,
        _: vk_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<vk::ZwpVirtualKeyboardV1, ()> for Ime {
    fn event(
        _: &mut Self,
        _: &vk::ZwpVirtualKeyboardV1,
        _: vk::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// An input method like fcitx5: grabs the keyboard, and gives back the keys it
/// doesn't use through a virtual keyboard with the keymap the grab came with.
fn input_method(
    socket: String,
    grabbed: Arc<Mutex<Vec<u32>>>,
    give_back: std::sync::mpsc::Receiver<u32>,
) {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap();
    let stream = std::os::unix::net::UnixStream::connect(format!("{runtime}/{socket}")).unwrap();
    let conn = Connection::from_socket(stream).unwrap();
    let (globals, mut queue) = registry_queue_init::<Ime>(&conn).unwrap();
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ()).unwrap();
    let manager: im_manager::ZwpInputMethodManagerV2 = globals.bind(&qh, 1..=1, ()).unwrap();
    let keyboards: vk_manager::ZwpVirtualKeyboardManagerV1 = globals.bind(&qh, 1..=1, ()).unwrap();
    let method = manager.get_input_method(&seat, &qh, ());
    let _grab = method.grab_keyboard(&qh, ());
    let mut ime = Ime::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while ime.keymap.is_none() && Instant::now() < deadline {
        queue.roundtrip(&mut ime).unwrap();
    }
    let (fd, size) = ime.keymap.take().expect("the grab's keymap");
    let keyboard = keyboards.create_virtual_keyboard(&seat, &qh, ());
    keyboard.keymap(1, fd.as_fd(), size);
    queue.roundtrip(&mut ime).unwrap();
    loop {
        match give_back.recv_timeout(Duration::from_millis(5)) {
            Ok(key) => {
                keyboard.key(0, key, 1);
                keyboard.key(1, key, 0);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return,
        }
        queue.roundtrip(&mut ime).unwrap();
        *grabbed.lock().unwrap() = ime.grabbed.clone();
    }
}

fn press(state: &mut State<'_>, key: u32, keysym: Keysym) {
    for pressed in [true, false] {
        state.input(InputEvent::Key {
            key,
            pressed,
            keysym,
            modifiers: Modifiers::default(),
        });
    }
}

#[test]
fn an_input_method_takes_keys_and_types_back() {
    let config = Config::parse("[overlay]\nat-startup = false").expect("config");
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    app::run(options, config, None, |state, display| {
        let settle =
            |state: &mut State<'_>, display: &Display, what: &str, done: &dyn Fn() -> bool| {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !done() {
                    assert!(Instant::now() < deadline, "timed out waiting: {what}");
                    app::step(state, display, Some(Duration::from_millis(5))).expect("step");
                }
            };
        let seat = state.services.globals.seat();
        seat.set_capabilities(true, true, false);
        assert!(seat.set_keymap(&KeymapState::from_layout("us").expect("keymap").text()));

        let client = Client::connect(state.socket());
        settle(state, display, "globals", &|| {
            !client.report.lock().unwrap().globals.is_empty()
        });
        let globals = client.report.lock().unwrap().globals.clone();
        for wanted in [
            "ext_workspace_manager_v1",
            "zwlr_foreign_toplevel_manager_v1",
            "ext_foreign_toplevel_list_v1",
            "zwlr_data_control_manager_v1",
            "zwp_text_input_manager_v3",
            "zwp_input_method_manager_v2",
            "zwp_virtual_keyboard_manager_v1",
            "zwlr_virtual_pointer_manager_v1",
            "wp_color_manager_v1",
            "wp_color_representation_manager_v1",
            "xdg_toplevel_icon_manager_v1",
        ] {
            assert!(
                globals.iter().any(|g| g == wanted),
                "{wanted} is not offered"
            );
        }

        client.open("one", 0xffd05050);
        settle(state, display, "focus", &|| client.seen(0).activated);
        press(state, KEY_A, Keysym::a);
        settle(state, display, "a key", &|| client.keys() == [KEY_A]);

        let grabbed = Arc::new(Mutex::new(Vec::new()));
        let (give_back, keys) = std::sync::mpsc::channel();
        let (socket, seen) = (state.socket().to_owned(), grabbed.clone());
        std::thread::spawn(move || input_method(socket, seen, keys));
        settle(state, display, "the grab", &|| {
            state
                .services
                .input_method
                .current()
                .is_some_and(|im| im.has_keyboard_grab())
        });

        press(state, KEY_H, Keysym::h);
        settle(state, display, "the input method has the key", &|| {
            grabbed.lock().unwrap().contains(&KEY_H)
        });
        assert_eq!(client.keys(), [KEY_A], "the window saw a grabbed key");

        give_back.send(KEY_B).unwrap();
        settle(state, display, "the typed-back key", &|| {
            client.keys() == [KEY_A, KEY_B]
        });
        Ok(())
    })
    .expect("compositor runs");
}
