//! Assembling a compositor out of luminaria's parts. Everything here borrows from
//! something declared before it, which is why it's one function: the order of these
//! locals is the order they must be torn down in reverse.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use luminaria::backend::{AutoBackend, BackendKind, BackendOptions};
use luminaria::core::Display;
use luminaria::gpu::{Cursor, SceneRenderer};
use luminaria::protocol::{CoreGlobals, DecorationMode, LayerShell};
use luminaria::shell::{InputRouter, OutputLayout, PopupTree, ScreenManager, ScreenOptions};
use pastel_config::Config;
use pastel_render::color;

use crate::state::{Services, State};

pub type Error = Box<dyn std::error::Error>;

/// Build the compositor and hand it to `driver`, which owns the main loop:
/// [`main_loop`] normally, or a test stepping through a scenario.
pub fn run<R>(
    backend_options: BackendOptions,
    config: Config,
    config_path: Option<PathBuf>,
    driver: impl for<'a> FnOnce(&mut State<'a>, &'a Display) -> Result<R, Error>,
) -> Result<R, Error> {
    let display = Display::new()?;
    run_on(&display, backend_options, config, config_path, driver)
}

fn run_on<'d, R>(
    display: &'d Display,
    mut options: BackendOptions,
    config: Config,
    config_path: Option<PathBuf>,
    driver: impl for<'a> FnOnce(&mut State<'a>, &'a Display) -> Result<R, Error>,
) -> Result<R, Error> {
    options.title = "Pastel".into();
    let globals = CoreGlobals::new(display)?.forward_window_requests();
    globals
        .decoration()
        .set_default_mode(DecorationMode::ServerSide);
    let (shell, seat) = (globals.shell(), globals.seat());
    let layer_shell = LayerShell::new(display)?;
    let layout = OutputLayout::new()?;
    let popups = PopupTree::new(shell)?;
    let backend = AutoBackend::new(display, &options)?;
    let session = backend.session();

    let renderer = SceneRenderer::new(&backend.vulkan_options())?;
    if !renderer.gpu_enabled() {
        eprintln!(
            "pastel: no GPU ({}): no rounding, shadows or blur",
            renderer.gpu_error()
        );
    }
    renderer.set_presentation(Some(globals.presentation()));
    let _dmabuf = renderer.linux_dmabuf(display);
    let cursor = Cursor::new_with_shapes(&renderer, &layout, seat, globals.cursor_shape())?;

    let screen_options = ScreenOptions {
        background: color(config.background),
        name: "pastel".into(),
        ..ScreenOptions::default()
    };
    let screens = ScreenManager::new(display, &backend, &renderer, &layout, &screen_options)?;
    screens.follow_popups(&popups);
    let router = InputRouter::new(seat, &cursor, &screens, shell, &popups)?;
    router.set_click_to_focus(false); // focus lives in Desktop, not in the router
    if let Some(source) = backend.input() {
        seat.follow(source, false);
    }

    let socket = display.add_socket_auto()?;
    backend.start()?;
    println!("pastel: WAYLAND_DISPLAY={socket}");

    let nested = backend.kind() == BackendKind::Nested;
    let services = Services {
        globals: &globals,
        layer_shell: &layer_shell,
        layout: &layout,
        popups: &popups,
        renderer: &renderer,
        screens: &screens,
        router: &router,
        cursor: &cursor,
        session: session.as_ref(),
    };
    let mut state = State::new(services, config, config_path, socket, nested)?;
    state.autostart();

    driver(&mut state, display)
}

/// One turn of the loop: wait for something to happen, take it in, show it.
pub fn step(
    state: &mut State<'_>,
    display: &Display,
    limit: Option<Duration>,
) -> Result<(), Error> {
    let timeout = match (state.timeout(), limit) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    for event in display.dispatch(timeout)? {
        state.handle(event);
    }
    state.frame();
    Ok(())
}

/// Run until quit, or past `deadline` if there is one.
pub fn main_loop(
    state: &mut State<'_>,
    display: &Display,
    deadline: Option<Instant>,
) -> Result<(), Error> {
    while !state.quit && deadline.is_none_or(|d| Instant::now() < d) {
        step(
            state,
            display,
            deadline.map(|d| d.saturating_duration_since(Instant::now())),
        )?;
    }
    Ok(())
}
