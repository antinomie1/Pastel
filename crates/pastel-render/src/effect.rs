//! Window effects. An [`Effect`] sees a window once a frame and either draws
//! underneath it or restyles its scene item. The [`Pipeline`] is what's enabled.

use luminaria::shell::SceneItem;
use luminaria::util::Color;
use pastel_config::{ResolvedRule, Rgba, WindowConfig};
use pastel_core::Rect;
use pastel_layout::WindowId;

use crate::to_pixels;

/// One window's state for this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowFrame {
    pub id: WindowId,
    /// Where it is right now, animation included, in whole pixels.
    pub rect: Rect,
    /// The open or close fade, 0 to 1. Rule opacity is separate.
    pub alpha: f32,
    /// How focused, 0 to 1, animated.
    pub focus: f32,
    pub rule: ResolvedRule,
    pub fullscreen: bool,
    /// Above the layout, over other windows.
    pub floating: bool,
}

/// The extension point for window appearance. Implement either or both, then add
/// it to [`Pipeline::from_config`].
pub trait Effect {
    /// Push items that go under the window, bottom first.
    fn below(&self, _window: &WindowFrame, _items: &mut Vec<SceneItem>) {}

    /// Adjust the window's own item.
    fn style(&self, _window: &WindowFrame, _item: &mut SceneItem) {}
}

pub fn color(c: Rgba) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a)
}

/// Cast by an empty rect so it lands under the border as well as the window.
struct Shadow {
    color: Rgba,
    size: f32,
    radius: f32,
    grow: f64,
}

impl Effect for Shadow {
    fn below(&self, window: &WindowFrame, items: &mut Vec<SceneItem>) {
        if window.fullscreen || self.size <= 0.0 {
            return;
        }
        let shadow = Rgba {
            a: self.color.a * window.alpha,
            ..self.color
        };
        let caster = to_pixels(window.rect.inflated(self.grow));
        let item = SceneItem::rect(caster, Color::rgba(0.0, 0.0, 0.0, 0.0))
            .with_corner_radius(self.radius)
            .with_shadow(color(shadow), self.size)
            .with_accepts_input(false);
        items.push(item);
    }
}

/// Hollow, because anything solid behind a translucent window shows through it and
/// ends up in its blur.
struct Border {
    active: Rgba,
    inactive: Rgba,
    width: f64,
    outer_radius: f32,
}

impl Effect for Border {
    fn below(&self, window: &WindowFrame, items: &mut Vec<SceneItem>) {
        if window.fullscreen || self.width <= 0.0 {
            return;
        }
        let tint = self.inactive.lerp(self.active, window.focus);
        let outer = to_pixels(window.rect.inflated(self.width));
        let item = SceneItem::border(outer, self.width.round() as i32, color(tint))
            .with_corner_radius(self.outer_radius)
            .with_alpha(window.alpha)
            .with_tag(window.id.0);
        items.push(item);
    }
}

struct Rounding(f32);

impl Effect for Rounding {
    fn style(&self, window: &WindowFrame, item: &mut SceneItem) {
        if !window.fullscreen {
            item.corner_radius = self.0;
        }
    }
}

struct Opacity(f32);

impl Effect for Opacity {
    fn style(&self, window: &WindowFrame, item: &mut SceneItem) {
        item.alpha *= window.rule.opacity.unwrap_or(self.0).clamp(0.0, 1.0);
    }
}

/// X-ray samples only the wallpaper: right for tiles, which never overlap, wrong
/// for anything floating over them.
struct Blur {
    enabled: bool,
    xray: bool,
}

impl Effect for Blur {
    fn style(&self, window: &WindowFrame, item: &mut SceneItem) {
        if !window.fullscreen && window.rule.blur.unwrap_or(self.enabled) {
            let xray = self.xray && !window.floating;
            *item = item.with_blur(xray).with_blur_opacity(window.alpha);
        }
    }
}

/// The enabled effects, in order. Order matters: what's pushed first is lowest.
#[derive(Default)]
pub struct Pipeline {
    effects: Vec<Box<dyn Effect>>,
}

impl Pipeline {
    pub fn new(effects: Vec<Box<dyn Effect>>) -> Self {
        Self { effects }
    }

    pub fn from_config(config: &WindowConfig) -> Self {
        let radius = config.corner_radius;
        // Around a rounded window, wider by the border. Around a square one, square:
        // adding the width would round a corner nothing asked to round.
        let outer_radius = if radius > 0.0 {
            radius + config.border_width as f32
        } else {
            0.0
        };
        Self::new(vec![
            Box::new(Shadow {
                color: config.shadow_color,
                size: config.shadow_size,
                radius: outer_radius,
                grow: config.border_width,
            }),
            Box::new(Border {
                active: config.active_color,
                inactive: config.inactive_color,
                width: config.border_width,
                outer_radius,
            }),
            Box::new(Rounding(radius)),
            Box::new(Opacity(config.opacity)),
            Box::new(Blur {
                enabled: config.blur,
                xray: config.blur_xray,
            }),
        ])
    }

    pub fn below(&self, window: &WindowFrame, items: &mut Vec<SceneItem>) {
        self.effects.iter().for_each(|e| e.below(window, items));
    }

    pub fn style(&self, window: &WindowFrame, item: &mut SceneItem) {
        self.effects.iter().for_each(|e| e.style(window, item));
    }
}
