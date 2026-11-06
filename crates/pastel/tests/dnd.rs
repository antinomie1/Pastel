//! Drag and drop as a browser tab does it: the icon the client draws follows
//! the pointer, offset as it asked, until the drop takes it away; and focus
//! does not follow a drag over other windows, which would scroll them away.

mod common;

use std::time::{Duration, Instant};

use common::dnd::{ICON, ICON_OFFSET};
use common::{Client, Command};
use luminaria::backend::{BackendKind, BackendOptions, InputEvent};
use luminaria::core::Display;
use luminaria::shell::{SceneItem, SceneKind};
use luminaria::util::Rect;
use pastel::app;
use pastel::state::State;
use pastel_config::Config;

const BTN_LEFT: u32 = 0x110;

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

/// Where the scene draws the drag icon.
fn icon_rect(state: &State<'_>) -> Option<Rect> {
    let icon = state.services.globals.data_device().drag_icon()?;
    let drawn = |item: &&SceneItem| item.kind == SceneKind::Surface && item.surface == icon;
    state.scene.iter().find(drawn).map(|item| item.rect)
}

fn to(state: &mut State<'_>, (x, y): (f64, f64)) {
    state.input(InputEvent::MotionAbsolute { x, y, output: None });
}

#[test]
fn a_dragged_tab_follows_the_pointer_until_the_drop() {
    let config = "[overlay]\nat-startup = false\n[input]\nfocus-follows-mouse = true\n";
    let options = BackendOptions {
        kind: BackendKind::Headless,
        ..BackendOptions::default()
    };
    let config = Config::parse(config).expect("config");
    app::run(options, config, None, |state, display| {
        state.services.globals.seat().set_capabilities(true, true, false);
        let client = Client::connect(state.socket());
        settle(state, display, "an output", |s| !s.desktop.monitors().is_empty());
        client.open("other", 0xff5070d0);
        settle(state, display, "a window", |_| client.seen(0).activated);
        let other = state.desktop.focused_window().expect("focus");
        client.open("browser", 0xffd07050);
        settle(state, display, "the browser", |_| client.seen(1).activated);
        let browser = state.desktop.focused_window().expect("focus");
        let start = state.desktop.rect_of(browser).expect("browser").center();
        let over = state.desktop.rect_of(other).expect("other").center();

        to(state, start);
        client.send(Command::ArmDrag(1));
        settle(state, display, "the client", |_| client.drag_armed());
        state.input(InputEvent::Button {
            button: BTN_LEFT,
            pressed: true,
        });
        let at = |(x, y): (f64, f64)| {
            let (x, y) = (x.floor() as i32 + ICON_OFFSET.0, y.floor() as i32 + ICON_OFFSET.1);
            Rect::new(x, y, ICON.0, ICON.1)
        };
        settle(state, display, "the icon", |s| icon_rect(s) == Some(at(start)));
        let icon = state.services.globals.data_device().drag_icon().expect("icon");

        to(state, over);
        settle(state, display, "the icon to follow", |s| {
            icon_rect(s) == Some(at(over))
        });
        assert_eq!(
            state.desktop.focused_window(),
            Some(browser),
            "focus followed the drag"
        );

        state.input(InputEvent::Button {
            button: BTN_LEFT,
            pressed: false,
        });
        settle(state, display, "the drop", |s| {
            let data = s.services.globals.data_device();
            !data.dragging() && !s.scene.iter().any(|i| i.surface == icon)
        });
        Ok(())
    })
    .expect("compositor runs");
}
