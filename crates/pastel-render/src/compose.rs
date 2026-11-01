//! The frame: desktop state in, scene list out. Layouts say where windows belong;
//! this is where they actually are on their way there.

use std::collections::HashMap;
use std::rc::Rc;

use luminaria::protocol::Layer;
use luminaria::shell::{SceneItem, SceneKind};
use luminaria::util::{Color, Rect as PixelRect};
use pastel_config::{Config, ResolvedRule, Rgba};
use pastel_core::{Anim, AnimRect, Curve, Rect};
use pastel_layout::{Desktop, Monitor, Placement, WindowId, Workspace};

use crate::effect::{Pipeline, WindowFrame, color};
use crate::to_pixels;

/// How far a new window slides up as it fades in, in logical pixels.
const OPEN_SLIDE: f64 = 28.0;
/// How far a closing window's ghost shrinks on each side.
const CLOSE_SHRINK: f64 = 18.0;
/// How much of the border colour tints the pane a closed window leaves behind.
const GHOST_ALPHA: f32 = 0.4;

/// What the composer needs from whoever knows about clients.
pub trait Host {
    fn is_mapped(&self, id: WindowId) -> bool;

    /// The window's item with its geometry at (`x`, `y`), unclipped.
    fn window(&self, id: WindowId, x: i32, y: i32) -> Option<SceneItem>;

    fn rule(&self, id: WindowId) -> ResolvedRule;

    /// Push one monitor's layer surfaces of one layer.
    fn layers(&self, monitor: u64, layer: Layer, items: &mut Vec<SceneItem>);

    /// Push every popup. Called once, after all the windows.
    fn popups(&self, items: &mut Vec<SceneItem>);
}

struct Visual {
    rect: AnimRect,
    open: Anim,
    focus: Anim,
    /// Drawn last frame, so it's somewhere a close animation makes sense.
    shown: bool,
}

/// A frosted pane standing in for a closed window: its pixels are gone with it.
struct Ghost {
    id: WindowId,
    rect: Rect,
    fade: Anim,
    rule: ResolvedRule,
}

struct Curves {
    movement: Rc<dyn Curve>,
    open: Rc<dyn Curve>,
    close: Rc<dyn Curve>,
    workspace: Rc<dyn Curve>,
    focus: Rc<dyn Curve>,
}

pub struct Composer {
    curves: Curves,
    pipeline: Pipeline,
    ghost_tint: Color,
    visuals: HashMap<WindowId, Visual>,
    views: HashMap<u64, Anim>,
    ghosts: Vec<Ghost>,
    /// One workspace's placements; kept to save allocating every frame.
    placements: Vec<Placement>,
}

impl Composer {
    pub fn new(config: &Config) -> Self {
        let a = &config.animation;
        Self {
            curves: Curves {
                movement: a.window_movement.clone(),
                open: a.window_open.clone(),
                close: a.window_close.clone(),
                workspace: a.workspace_switch.clone(),
                focus: a.focus_ring.clone(),
            },
            pipeline: Pipeline::from_config(&config.window),
            ghost_tint: color(Rgba {
                a: GHOST_ALPHA,
                ..config.window.inactive_color
            }),
            visuals: HashMap::new(),
            views: HashMap::new(),
            ghosts: Vec::new(),
            placements: Vec::new(),
        }
    }

    /// Pick up a reloaded config, including mid-flight.
    pub fn reconfigure(&mut self, config: &Config) {
        let fresh = Self::new(config);
        self.curves = fresh.curves;
        self.pipeline = fresh.pipeline;
        self.ghost_tint = fresh.ghost_tint;
        for visual in self.visuals.values_mut() {
            visual.rect.set_curve(self.curves.movement.clone());
            visual.open.set_curve(self.curves.open.clone());
            visual.focus.set_curve(self.curves.focus.clone());
        }
        for view in self.views.values_mut() {
            view.set_curve(self.curves.workspace.clone());
        }
    }

    /// A window is gone. If it was on screen, it leaves a ghost to fade out.
    pub fn close(&mut self, id: WindowId, rule: ResolvedRule, now: f64) {
        let Some(visual) = self.visuals.remove(&id) else {
            return;
        };
        if !visual.shown {
            return;
        }
        let mut fade = Anim::new(0.0, self.curves.close.clone());
        fade.set(1.0, now);
        self.ghosts.push(Ghost {
            id,
            rect: visual.rect.value(now),
            fade,
            rule,
        });
    }

    /// Forget a monitor that left.
    pub fn remove_monitor(&mut self, id: u64) {
        self.views.remove(&id);
    }

    /// Build the scene into `items`. True while anything is still moving, which
    /// is what keeps frames coming.
    pub fn compose(
        &mut self,
        desktop: &mut Desktop,
        host: &dyn Host,
        now: f64,
        items: &mut Vec<SceneItem>,
    ) -> bool {
        items.clear();
        // Before drawing: a finished ghost must not get one more frame, which
        // would be the last one and stay on screen.
        self.ghosts.retain(|ghost| !ghost.fade.is_done(now));
        let mut animating = false;
        for monitor in desktop.monitors_mut() {
            let curve = &self.curves.workspace;
            let view = self
                .views
                .entry(monitor.id)
                .or_insert_with(|| Anim::new(monitor.active as f64, curve.clone()));
            view.shift(monitor.take_view_shift());
            view.set(monitor.active as f64, now);
            animating |= view.tick(now);
        }
        let focused = desktop.focused_window();
        let monitors = desktop.monitors();
        for monitor in monitors {
            let clip = clip_of(monitors, monitor);
            let view = self.views[&monitor.id].value(now);
            host.layers(monitor.id, Layer::Background, items);
            host.layers(monitor.id, Layer::Bottom, items);
            for (index, workspace) in monitor.workspaces.iter().enumerate() {
                let offset = index as f64 - view;
                animating |= self.workspace(
                    (monitor, workspace),
                    offset,
                    focused,
                    clip,
                    host,
                    now,
                    items,
                );
            }
            animating |= self.draw_ghosts(monitor, clip, now, items);
            // A fullscreen window at rest covers the bar; while sliding it can't.
            let covered = monitor.active_workspace().fullscreen.is_some() && view.fract() == 0.0;
            if !covered {
                host.layers(monitor.id, Layer::Top, items);
            }
            host.layers(monitor.id, Layer::Overlay, items);
        }
        host.popups(items);
        animating
    }

    /// Update one workspace's animations, and draw it if it's on screen.
    #[allow(clippy::too_many_arguments)]
    fn workspace(
        &mut self,
        (monitor, workspace): (&Monitor, &Workspace),
        offset: f64,
        focused: Option<WindowId>,
        clip: Rect,
        host: &dyn Host,
        now: f64,
        items: &mut Vec<SceneItem>,
    ) -> bool {
        let mut placements = std::mem::take(&mut self.placements);
        placements.clear();
        Desktop::place(monitor, workspace, &mut placements);
        // Keep the focused window, and any fullscreen one, above its neighbours.
        placements.sort_by_key(|p| (p.fullscreen, Some(p.id) == focused));
        let dy = offset * monitor.rect.h;
        let visible = offset.abs() < 1.0;
        let mut animating = false;
        for placement in &placements {
            if !host.is_mapped(placement.id) {
                continue;
            }
            let is_focused = Some(placement.id) == focused;
            let visual = self.visual(placement, is_focused, now);
            visual.rect.set(placement.rect, now);
            visual.focus.set(f64::from(u8::from(is_focused)), now);
            animating |= visual.rect.tick(now) | visual.open.tick(now) | visual.focus.tick(now);
            visual.shown = visible;
            if !visible {
                continue;
            }
            let open = visual.open.value(now).clamp(0.0, 1.0);
            let rect = visual
                .rect
                .value(now)
                .translated(0.0, dy + (1.0 - open) * OPEN_SLIDE);
            let frame = WindowFrame {
                id: placement.id,
                rect: from_pixels(to_pixels(rect)),
                alpha: open as f32,
                focus: visual.focus.value(now).clamp(0.0, 1.0) as f32,
                rule: host.rule(placement.id),
                fullscreen: placement.fullscreen,
            };
            self.draw_window(&frame, monitor, clip, host, items);
        }
        self.placements = placements;
        animating
    }

    fn visual(&mut self, placement: &Placement, focused: bool, now: f64) -> &mut Visual {
        let curves = &self.curves;
        self.visuals.entry(placement.id).or_insert_with(|| {
            let mut open = Anim::new(0.0, curves.open.clone());
            open.set(1.0, now);
            let focus = f64::from(u8::from(focused));
            Visual {
                rect: AnimRect::new(placement.rect, curves.movement.clone()),
                open,
                focus: Anim::new(focus, curves.focus.clone()),
                shown: false,
            }
        })
    }

    fn draw_window(
        &self,
        frame: &WindowFrame,
        monitor: &Monitor,
        clip: Rect,
        host: &dyn Host,
        items: &mut Vec<SceneItem>,
    ) {
        if frame.rect.intersection(&monitor.rect).is_none() {
            return;
        }
        let bounds = to_pixels(frame.rect);
        let Some(mut item) = host.window(frame.id, bounds.x, bounds.y) else {
            return;
        };
        let visible = to_pixels(clip_to(frame.rect, clip));
        // Clip to the animated box: mid-resize the client's buffer is still the
        // old size, and must not stick out past its border.
        let Some(shown) = item.rect.intersection(&visible) else {
            return;
        };
        item.rect = shown;
        item.alpha = frame.alpha;
        let start = items.len();
        self.pipeline.below(frame, items);
        clip_items(&mut items[start..], clip);
        self.pipeline.style(frame, &mut item);
        items.push(item);
    }

    fn draw_ghosts(
        &self,
        monitor: &Monitor,
        clip: Rect,
        now: f64,
        items: &mut Vec<SceneItem>,
    ) -> bool {
        let mut animating = false;
        for ghost in &self.ghosts {
            let progress = ghost.fade.value(now).clamp(0.0, 1.0);
            animating |= !ghost.fade.is_done(now);
            let rect = ghost.rect.inflated(-progress * CLOSE_SHRINK);
            if rect.intersection(&monitor.rect).is_none() {
                continue;
            }
            let frame = WindowFrame {
                id: ghost.id,
                rect: from_pixels(to_pixels(rect)),
                alpha: (1.0 - progress) as f32,
                focus: 0.0,
                rule: ghost.rule,
                fullscreen: false,
            };
            let start = items.len();
            self.pipeline.below(&frame, items);
            let mut body = SceneItem::rect(to_pixels(rect), self.ghost_tint)
                .with_alpha(frame.alpha)
                .with_accepts_input(false);
            self.pipeline.style(&frame, &mut body);
            items.push(body);
            clip_items(&mut items[start..], clip);
        }
        animating
    }
}

fn from_pixels(rect: PixelRect) -> Rect {
    Rect::new(
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    )
}

/// A monitor's scene is one list shared with every output, so what spills over an
/// edge shared with another monitor must be cut there. Edges facing nothing stay
/// open: cutting them would round a window's corners at the edge of the screen.
fn clip_of(monitors: &[Monitor], monitor: &Monitor) -> Rect {
    const FAR: f64 = 1e7;
    let r = monitor.rect;
    let beside = |o: &Rect| o.y < r.bottom() && o.bottom() > r.y;
    let stacked = |o: &Rect| o.x < r.right() && o.right() > r.x;
    let others = || {
        monitors
            .iter()
            .filter(|m| m.id != monitor.id)
            .map(|m| &m.rect)
    };
    let left = others().any(|o| beside(o) && o.right() <= r.x);
    let right = others().any(|o| beside(o) && o.x >= r.right());
    let up = others().any(|o| stacked(o) && o.bottom() <= r.y);
    let down = others().any(|o| stacked(o) && o.y >= r.bottom());
    let x0 = if left { r.x } else { -FAR };
    let y0 = if up { r.y } else { -FAR };
    let x1 = if right { r.right() } else { FAR };
    let y1 = if down { r.bottom() } else { FAR };
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn clip_to(rect: Rect, clip: Rect) -> Rect {
    rect.intersection(&clip).unwrap_or_default()
}

fn clip_items(items: &mut [SceneItem], clip: Rect) {
    let clip = to_pixels(clip);
    for item in items {
        let cut = item.rect.intersection(&clip).unwrap_or_default();
        // An image's pixels are 1:1 with its box: cut, it couldn't be drawn at all.
        let whole = item.kind != SceneKind::Image || cut == item.rect;
        item.rect = if whole { cut } else { PixelRect::default() };
    }
}
