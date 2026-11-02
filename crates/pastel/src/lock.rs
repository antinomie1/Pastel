//! The session lock. It fails closed: from the moment a client asks, the scene is
//! an opaque cover per monitor and that monitor's lock surface, and nothing else.
//! Nothing here depends on the lock client staying alive: one that dies leaves the
//! cover up until another one unlocks.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use luminaria::backend::OutputId;
use luminaria::desktop::{LockId, SessionLockEvent};
use luminaria::protocol::SurfaceId;
use luminaria::shell::SceneItem;
use luminaria::util::{Color, Rect as PixelRect};

use crate::state::State;

/// `locked` promises the screen is covered, and a flip already on its way could
/// be the old picture: the second present of an output is certainly the cover.
const PRESENTS: u8 = 2;
/// For an output that will never present, like one on a VT we've left.
const COVER_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct LockState {
    /// A client that asked, until the screen is covered.
    pending: Option<LockId>,
    owed: HashMap<OutputId, u8>,
    deadline: Option<Instant>,
    /// Each lock surface and the monitor it's for.
    pub(crate) surfaces: Vec<(SurfaceId, u64)>,
    focus: Option<SurfaceId>,
}

impl State<'_> {
    /// Whether anything but the lock may be shown or get input.
    pub fn is_locked(&self) -> bool {
        self.lock.pending.is_some() || self.services.locks.is_locked()
    }

    pub(crate) fn lock_event(&mut self, event: SessionLockEvent) {
        match event {
            SessionLockEvent::NewLock(lock) => {
                if self.services.locks.is_locked() {
                    // Its predecessor died: the cover never came down.
                    self.services.locks.lock(lock).send_locked();
                } else {
                    self.lock.pending = Some(lock);
                }
            }
            SessionLockEvent::NewSurface { surface, .. } => self.place_lock_surface(surface),
            SessionLockEvent::SurfaceMap(_) => {}
            SessionLockEvent::SurfaceDestroy(surface) => {
                self.lock.surfaces.retain(|(s, _)| *s != surface);
            }
            SessionLockEvent::LockDestroy { lock, .. } => {
                if self.lock.pending == Some(lock) {
                    self.lock.pending = None;
                    self.lock.deadline = None;
                }
            }
        }
        self.dirty = true;
    }

    fn place_lock_surface(&mut self, surface: SurfaceId) {
        let Some(lock_surface) = self.services.locks.lock_surface(surface) else {
            return;
        };
        let identity = lock_surface.output_identity();
        let screens = self.services.screens;
        if let Some(output) = screens.output_with(identity, &self.outputs) {
            self.lock.surfaces.push((surface, output.id().0));
            self.size_lock_surfaces();
            self.tell_output_scales();
        }
    }

    /// To their monitors, which can change size under a lock.
    pub(crate) fn size_lock_surfaces(&self) {
        for (surface, monitor) in &self.lock.surfaces {
            let lock_surface = self.services.locks.lock_surface(*surface);
            let monitor = self.desktop.monitor(*monitor);
            if let (Some(lock_surface), Some(monitor)) = (lock_surface, monitor) {
                let size = (monitor.rect.w as u32, monitor.rect.h as u32);
                if lock_surface.size() != size {
                    lock_surface.configure(size.0, size.1);
                }
            }
        }
    }

    pub(crate) fn forget_lock_monitor(&mut self, id: OutputId) {
        self.lock.surfaces.retain(|(_, monitor)| *monitor != id.0);
        self.lock.owed.remove(&id);
        self.finish_locking();
    }

    fn mapped(&self, surface: SurfaceId) -> bool {
        let lock_surface = self.services.locks.lock_surface(surface);
        lock_surface.is_some_and(|s| s.is_mapped())
    }

    /// What has the keyboard: a surface that was clicked, or the focused monitor's.
    pub(crate) fn lock_keyboard_target(&self) -> Option<SurfaceId> {
        let mapped = |surface: &SurfaceId| self.mapped(*surface);
        let focused = self.desktop.focused_monitor().map(|monitor| monitor.id);
        let on_focused = self
            .lock
            .surfaces
            .iter()
            .filter(|(_, m)| Some(*m) == focused);
        self.lock
            .focus
            .filter(mapped)
            .or(on_focused.map(|(s, _)| *s).find(mapped))
            .or(self.lock.surfaces.iter().map(|(s, _)| *s).find(mapped))
    }

    pub(crate) fn lock_click(&mut self, surface: Option<SurfaceId>) {
        if surface.is_some_and(|s| self.lock.surfaces.iter().any(|(known, _)| *known == s)) {
            self.lock.focus = surface;
            self.dirty = true;
        }
    }

    /// The whole scene while locked. The covers are what `locked` is a promise
    /// about: unlike an empty list, they are a change on every output.
    pub(crate) fn lock_scene(&mut self) {
        let mut scene = std::mem::take(&mut self.scene);
        scene.clear();
        for monitor in self.desktop.monitors() {
            let r = monitor.rect;
            let rect = PixelRect::new(r.x as i32, r.y as i32, r.w as i32, r.h as i32);
            scene.push(SceneItem::rect(rect, Color::rgb(0.0, 0.0, 0.0)).with_accepts_input(false));
            for (surface, on) in &self.lock.surfaces {
                if *on == monitor.id && self.mapped(*surface) {
                    scene.push(SceneItem::surface(*surface, rect, rect.x, rect.y));
                }
            }
        }
        self.scene = scene;
        if self.lock.pending.is_some() && self.lock.deadline.is_none() {
            self.lock.deadline = Some(Instant::now() + COVER_TIMEOUT);
            let outputs = self.outputs.iter().map(|o| (o.id(), PRESENTS));
            self.lock.owed = outputs.collect();
            self.finish_locking();
        }
    }

    pub(crate) fn lock_presented(&mut self, output: OutputId) {
        if let Some(owed) = self.lock.owed.get_mut(&output) {
            *owed -= 1;
            if *owed == 0 {
                self.lock.owed.remove(&output);
            }
            self.finish_locking();
        }
    }

    /// How long the loop may sleep before giving up on a present.
    pub(crate) fn lock_timeout(&self) -> Option<Duration> {
        let deadline = self.lock.deadline?;
        Some(deadline.saturating_duration_since(Instant::now()))
    }

    pub(crate) fn check_lock_deadline(&mut self) {
        if self.lock.deadline.is_some_and(|d| Instant::now() >= d) {
            self.lock.owed.clear();
            self.finish_locking();
        }
    }

    fn finish_locking(&mut self) {
        if self.lock.deadline.is_none() || !self.lock.owed.is_empty() {
            return;
        }
        if let Some(lock) = self.lock.pending.take() {
            self.services.locks.lock(lock).send_locked();
        }
        self.lock.deadline = None;
    }
}
