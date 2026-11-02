//! xdg-activation from the client's side: a token from one window, for another.

use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1};

use super::App;

impl App {
    pub(super) fn activate(&mut self, from: usize, target: usize) {
        let qh = self.qh.clone();
        let manager: xdg_activation_v1::XdgActivationV1 =
            self.globals.bind(&qh, 1..=1, ()).expect("xdg_activation");
        let token = manager.get_activation_token(&qh, (manager.clone(), target));
        if let Some(window) = self.windows.get(&from) {
            token.set_surface(&window.surface);
        }
        token.commit();
    }
}

impl Dispatch<xdg_activation_v1::XdgActivationV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &xdg_activation_v1::XdgActivationV1,
        _: xdg_activation_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

type Pending = (xdg_activation_v1::XdgActivationV1, usize);

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, Pending> for App {
    fn event(
        app: &mut Self,
        _: &xdg_activation_token_v1::XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        (manager, target): &Pending,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event
            && let Some(window) = app.windows.get(target)
        {
            manager.activate(token, &window.surface);
        }
    }
}
