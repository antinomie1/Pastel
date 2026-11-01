//! What's on screen mid-animation, with a stand-in for the client side.

use std::collections::HashSet;

use luminaria::protocol::{Layer, SurfaceId};
use luminaria::shell::{SceneItem, SceneKind};
use luminaria::util::{Color, Rect as PixelRect};
use pastel_config::{Config, ResolvedRule};
use pastel_layout::{Desktop, LayoutOp, Rect, WindowHint, WindowId, WorkspaceTarget};
use pastel_render::{Composer, Effect, Host, Pipeline, WindowFrame};

const SCREEN: Rect = Rect::new(0.0, 0.0, 1000.0, 600.0);
const RIGHT: Rect = Rect::new(1000.0, 0.0, 1000.0, 600.0);
const A: WindowId = WindowId(1);
const B: WindowId = WindowId(2);
/// Past every default animation.
const LATER: f64 = 10.0;

#[derive(Default)]
struct FakeHost {
    unmapped: HashSet<WindowId>,
    rule: ResolvedRule,
}

impl Host for FakeHost {
    fn is_mapped(&self, id: WindowId) -> bool {
        !self.unmapped.contains(&id)
    }

    fn window(&self, id: WindowId, x: i32, y: i32) -> Option<SceneItem> {
        let (w, h) = (2000, 2000); // bigger than any box: shows the clip
        let rect = PixelRect::new(x, y, w, h);
        Some(SceneItem::surface(SurfaceId(id.0), rect, x, y).with_tag(id.0))
    }

    fn rule(&self, _id: WindowId) -> ResolvedRule {
        self.rule
    }

    fn layers(&self, monitor: u64, layer: Layer, items: &mut Vec<SceneItem>) {
        let tag = monitor * 100 + layer as u64;
        items.push(
            SceneItem::rect(PixelRect::new(0, 0, 1, 1), Color::rgb(0.0, 0.0, 0.0)).with_tag(tag),
        );
    }

    fn popups(&self, _items: &mut Vec<SceneItem>) {}
}

struct World {
    desktop: Desktop,
    composer: Composer,
    host: FakeHost,
    items: Vec<SceneItem>,
}

impl World {
    fn new(config: &str) -> Self {
        let config = Config::parse(config).expect("config");
        let mut desktop = Desktop::new(config.layout.clone()).expect("layout");
        desktop.add_monitor(1, SCREEN);
        Self {
            desktop,
            composer: Composer::new(&config),
            host: FakeHost::default(),
            items: Vec::new(),
        }
    }

    fn compose(&mut self, now: f64) -> bool {
        self.composer
            .compose(&mut self.desktop, &self.host, now, &mut self.items)
    }

    fn add(&mut self, id: WindowId) {
        self.desktop.add_window(id, WindowHint::default());
    }

    fn window(&self, id: WindowId) -> Option<&SceneItem> {
        let is_window = |i: &&SceneItem| i.kind == SceneKind::Surface && i.tag == id.0;
        self.items.iter().find(is_window)
    }

    fn tagged(&self, kind: SceneKind) -> usize {
        self.items
            .iter()
            .filter(|i| i.kind == kind && i.tag > 0 && i.tag < 100)
            .count()
    }
}

#[test]
fn a_new_window_fades_and_slides_in() {
    let mut world = World::new("");
    world.add(A);
    assert!(world.compose(0.0));
    let first = *world.window(A).expect("drawn");
    assert_eq!(first.alpha, 0.0);
    assert!(world.compose(0.08));
    let middle = *world.window(A).expect("drawn");
    assert!(middle.alpha > 0.0 && middle.alpha < 1.0);
    assert!(middle.rect.y < first.rect.y);
    assert!(!world.compose(LATER));
    let rest = world.window(A).expect("drawn");
    assert_eq!(rest.alpha, 1.0);
    assert_eq!(rest.rect.y, 12);
}

#[test]
fn an_unmapped_window_draws_nothing_and_does_not_animate() {
    let mut world = World::new("");
    world.host.unmapped.insert(A);
    world.add(A);
    assert!(!world.compose(0.0));
    assert!(world.window(A).is_none());
    world.host.unmapped.clear();
    assert!(
        world.compose(1.0),
        "its open animation starts when it maps, not before"
    );
    assert_eq!(world.window(A).expect("drawn").alpha, 0.0);
}

#[test]
fn windows_spring_towards_a_new_layout() {
    let mut world = World::new("[layout]\ndefault-width = 0.75");
    world.add(A);
    world.compose(0.0);
    world.compose(LATER);
    let start = world.window(A).expect("drawn").rect.x;
    world.add(B); // scrolls A left
    world.compose(LATER);
    world.compose(LATER + 0.02);
    let moving = world.window(A).expect("drawn").rect.x;
    assert!(moving < start, "moving: {moving}");
    world.compose(LATER * 2.0);
    let end = world.window(A).expect("drawn").rect.x;
    assert!(end < moving, "still heading there: {end}");
}

#[test]
fn the_animation_is_the_same_whatever_the_frame_rate() {
    let run = |frames: u32| {
        let mut world = World::new("[layout]\ndefault-width = 0.75");
        world.add(A);
        world.compose(0.0);
        world.compose(LATER);
        world.add(B);
        for i in 0..=frames {
            world.compose(LATER + 0.05 * f64::from(i) / f64::from(frames));
        }
        world.window(A).expect("drawn").rect.x
    };
    assert_eq!(run(3), run(144));
}

#[test]
fn every_item_of_a_window_fades_with_it() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    world.compose(0.08);
    let alpha = world.window(A).expect("drawn").alpha;
    let border = world
        .items
        .iter()
        .find(|i| i.kind == SceneKind::Border)
        .expect("border");
    assert_eq!(border.alpha, alpha);
    assert!(
        border.corner_radius > 12.0,
        "rounded, around the window's own radius"
    );
}

#[test]
fn focus_fades_the_border_between_colours() {
    let mut world = World::new("");
    world.add(A);
    world.add(B);
    world.compose(0.0);
    world.compose(LATER);
    let tint = |w: &World, id: WindowId| {
        let is_border = |i: &&SceneItem| i.kind == SceneKind::Border && i.tag == id.0;
        w.items.iter().find(is_border).expect("border").color
    };
    let (inactive, active) = (tint(&world, A), tint(&world, B));
    assert_ne!(inactive, active);
    world
        .desktop
        .apply(LayoutOp::Focus(pastel_layout::Direction::Left));
    world.compose(LATER);
    world.compose(LATER + 0.04);
    let fading = tint(&world, A);
    assert_ne!(fading, inactive);
    assert_ne!(fading, active);
    world.compose(LATER * 2.0);
    assert_eq!(tint(&world, A), active);
    assert_eq!(tint(&world, B), inactive);
}

#[test]
fn a_closed_window_leaves_a_ghost_that_fades_out() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    world.compose(LATER);
    world.desktop.remove_window(A);
    world.composer.close(A, ResolvedRule::default(), LATER);
    assert!(world.compose(LATER + 0.08));
    assert!(world.window(A).is_none());
    let border = world
        .items
        .iter()
        .find(|i| i.kind == SceneKind::Border)
        .expect("ghost");
    assert!(border.alpha > 0.0 && border.alpha < 1.0);
    assert!(!world.compose(LATER + 5.0));
    assert!(world.items.iter().all(|i| i.kind != SceneKind::Border));
}

#[test]
fn blur_fades_with_the_window_but_not_with_its_rule_opacity() {
    let mut world = World::new("");
    world.host.rule = ResolvedRule {
        opacity: Some(0.5),
        ..ResolvedRule::default()
    };
    world.add(A);
    world.compose(0.0);
    assert_eq!(world.window(A).expect("drawn").blur_opacity, 0.0);
    world.compose(0.08);
    let fading = world.window(A).expect("drawn").blur_opacity;
    assert!(fading > 0.0 && fading < 1.0);
    world.compose(LATER);
    let item = world.window(A).expect("drawn");
    assert_eq!((item.alpha, item.blur_opacity), (0.5, 1.0));
}

#[test]
fn a_window_closing_out_of_sight_leaves_nothing() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    world.desktop.focus_workspace(WorkspaceTarget::Down);
    world.compose(LATER);
    world.compose(LATER * 2.0);
    world.desktop.remove_window(A);
    world
        .composer
        .close(A, ResolvedRule::default(), LATER * 2.0);
    assert!(!world.compose(LATER * 2.0 + 0.01));
}

#[test]
fn workspaces_slide_past_each_other() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    world.compose(LATER);
    world.desktop.focus_workspace(WorkspaceTarget::Down);
    world.add(B);
    world.compose(LATER);
    world.compose(LATER + 0.03);
    let (a, b) = (
        *world.window(A).expect("leaving"),
        *world.window(B).expect("arriving"),
    );
    assert!(a.rect.y < 12, "A slides up: {}", a.rect.y);
    assert!(
        b.rect.y > 12 && b.rect.y < 612,
        "B comes from below: {}",
        b.rect.y
    );
    world.compose(LATER * 2.0);
    assert!(world.window(A).is_none(), "out of sight, out of the scene");
    assert_eq!(world.window(B).expect("arrived").rect.y, 12);
}

#[test]
fn removing_a_workspace_above_does_not_jump_the_view() {
    let mut world = World::new("");
    world.add(A);
    world.desktop.focus_workspace(WorkspaceTarget::Down);
    world.add(B);
    world.compose(0.0);
    world.compose(LATER);
    world.desktop.focus_window(A);
    world.compose(LATER);
    world.compose(LATER * 2.0);
    world.desktop.remove_window(A);
    world.desktop.focus_workspace(WorkspaceTarget::Down);
    assert!(world.compose(LATER * 2.0));
    assert!(world.window(B).is_none(), "still exactly one screen away");
    world.compose(LATER * 2.0 + 0.01);
    let y = world.window(B).expect("drawn").rect.y;
    assert!(
        y > 500,
        "B slides in from where it was, it doesn't appear: {y}"
    );
}

#[test]
fn fullscreen_has_no_decoration_and_covers_the_top_layer() {
    let mut world = World::new("");
    let top = 100 + Layer::Top as u64;
    world.add(A);
    world.compose(0.0);
    world.compose(LATER);
    assert!(world.items.iter().any(|i| i.tag == top));
    world.desktop.toggle_fullscreen();
    world.compose(LATER);
    world.compose(LATER * 2.0);
    let item = world.window(A).expect("drawn");
    assert_eq!(item.corner_radius, 0.0);
    assert!(!item.blur_enabled);
    assert_eq!(item.rect, PixelRect::new(0, 0, 1000, 600));
    assert!(world.items.iter().all(|i| i.kind != SceneKind::Border));
    assert!(world.items.iter().all(|i| i.tag != top));
}

#[test]
fn layers_stack_around_the_windows() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    let index = |tag: u64| {
        world
            .items
            .iter()
            .position(|i| i.tag == tag)
            .expect("present")
    };
    let window = world
        .items
        .iter()
        .position(|i| i.kind == SceneKind::Surface)
        .expect("drawn");
    let layer = |l: Layer| index(100 + l as u64);
    assert!(layer(Layer::Background) < layer(Layer::Bottom));
    assert!(layer(Layer::Bottom) < window);
    assert!(window < layer(Layer::Top));
    assert!(layer(Layer::Top) < layer(Layer::Overlay));
}

#[test]
fn windows_are_clipped_at_an_edge_shared_with_another_monitor() {
    let mut world = World::new("[layout]\ndefault-width = 0.75");
    world.desktop.add_monitor(2, RIGHT);
    world.add(A);
    world.add(B);
    world.compose(0.0);
    world.compose(LATER);
    world.compose(LATER * 2.0);
    let a = world.window(A).expect("drawn");
    assert!(
        a.rect.x < 0,
        "off the left, where there is nothing: {:?}",
        a.rect
    );
    assert!(a.rect.x + a.rect.width <= 1000);
    let b = world.window(B).expect("drawn");
    assert_eq!(b.rect.x + b.rect.width, 988, "inside its window box");
}

#[test]
fn a_window_never_shows_past_its_box_while_the_buffer_is_too_big() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    world.compose(LATER);
    let item = world.window(A).expect("drawn");
    assert_eq!((item.rect.width, item.rect.height), (482, 576));
}

#[test]
fn rules_reach_the_window() {
    let mut world = World::new("");
    world.host.rule = ResolvedRule {
        opacity: Some(0.5),
        blur: Some(false),
        default_width: None,
    };
    world.add(A);
    world.compose(0.0);
    world.compose(LATER);
    let item = world.window(A).expect("drawn");
    assert_eq!(item.alpha, 0.5);
    assert!(!item.blur_enabled);
}

#[test]
fn opaque_defaults_still_blur_through_the_pipeline() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    let item = world.window(A).expect("drawn");
    assert!(item.blur_enabled && item.blur_xray);
    assert_eq!(world.tagged(SceneKind::Border), 1);
}

#[test]
fn turning_animation_off_settles_at_once() {
    let mut world = World::new("[animation]\nenabled = false");
    world.add(A);
    assert!(!world.compose(0.0));
    assert_eq!(world.window(A).expect("drawn").alpha, 1.0);
}

#[test]
fn a_reloaded_config_applies_to_windows_already_open() {
    let mut world = World::new("");
    world.add(A);
    world.compose(0.0);
    world.compose(LATER);
    let config = Config::parse("[window]\ncorner-radius = 3.0").expect("config");
    world.composer.reconfigure(&config);
    world.compose(LATER);
    assert_eq!(world.window(A).expect("drawn").corner_radius, 3.0);
}

/// An effect from outside the crate: the point of the trait.
struct Dim;

impl Effect for Dim {
    fn style(&self, _window: &WindowFrame, item: &mut SceneItem) {
        item.alpha *= 0.5;
    }

    fn below(&self, window: &WindowFrame, items: &mut Vec<SceneItem>) {
        items.push(
            SceneItem::rect(PixelRect::new(1, 2, 3, 4), Color::rgb(0.0, 0.0, 0.0))
                .with_tag(window.id.0),
        );
    }
}

#[test]
fn the_pipeline_runs_custom_effects_in_order() {
    let pipeline = Pipeline::new(vec![Box::new(Dim), Box::new(Dim)]);
    let frame = WindowFrame {
        id: A,
        rect: SCREEN,
        alpha: 1.0,
        focus: 1.0,
        rule: ResolvedRule::default(),
        fullscreen: false,
    };
    let mut items = Vec::new();
    pipeline.below(&frame, &mut items);
    assert_eq!(items.len(), 2);
    let mut item = SceneItem::rect(PixelRect::new(0, 0, 1, 1), Color::rgb(0.0, 0.0, 0.0));
    pipeline.style(&frame, &mut item);
    assert_eq!(item.alpha, 0.25);
}
