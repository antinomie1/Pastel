//! The TOML document: `Raw*` mirrors the file, [`Config`] is what the rest of the
//! program uses, with strings validated into real types.

use std::collections::BTreeMap;
use std::rc::Rc;

use pastel_core::{Curve, Ease, Easing, Slowed, Snap, Spring};
use pastel_layout::{CenterFocus, LAYOUTS, LayoutOptions};
use serde::Deserialize;

use crate::action::Action;
use crate::defaults;
use crate::keys::{self, Binds, Combo};

/// An sRGB colour, each channel 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = || format!("invalid colour `{text}`, expected #rrggbb or #rrggbbaa");
        let hex = text
            .strip_prefix('#')
            .filter(|h| h.is_ascii())
            .ok_or_else(bad)?;
        let (step, scale) = match hex.len() {
            3 | 4 => (1, 15.0),
            6 | 8 => (2, 255.0),
            _ => return Err(bad()),
        };
        let channel = |i: usize| match hex.get(i * step..(i + 1) * step) {
            Some(digits) => u8::from_str_radix(digits, 16).map(|v| f32::from(v) / scale),
            None => Ok(1.0),
        };
        let parsed = (channel(0), channel(1), channel(2), channel(3));
        match parsed {
            (Ok(r), Ok(g), Ok(b), Ok(a)) => Ok(Self { r, g, b, a }),
            _ => Err(bad()),
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            r: mix(self.r, other.r),
            g: mix(self.g, other.g),
            b: mix(self.b, other.b),
            a: mix(self.a, other.a),
        }
    }
}

/// One animation's curve, as written in the file.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum CurveSpec {
    Spring { stiffness: f64, damping: f64 },
    Ease { ms: f64, easing: EasingName },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EasingName {
    Linear,
    OutQuad,
    OutCubic,
    OutQuint,
    OutExpo,
    InOutCubic,
}

impl CurveSpec {
    /// The extension point for new kinds of curve.
    fn build(self, slowdown: f64) -> Rc<dyn Curve> {
        match self {
            Self::Spring { stiffness, damping } => Rc::new(Slowed {
                inner: Spring { stiffness, damping },
                factor: slowdown,
            }),
            Self::Ease { ms, easing } => {
                let easing = match easing {
                    EasingName::Linear => Easing::Linear,
                    EasingName::OutQuad => Easing::OutQuad,
                    EasingName::OutCubic => Easing::OutCubic,
                    EasingName::OutQuint => Easing::OutQuint,
                    EasingName::OutExpo => Easing::OutExpo,
                    EasingName::InOutCubic => Easing::InOutCubic,
                };
                Rc::new(Slowed {
                    inner: Ease {
                        seconds: ms / 1000.0,
                        easing,
                    },
                    factor: slowdown,
                })
            }
        }
    }
}

/// One monitor, by the name `wlr-randr` shows. Unset fields stay as they are.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputConfig {
    pub name: String,
    pub enabled: bool,
    /// Width, height and millihertz; 0 for the highest refresh at that size.
    pub mode: Option<(i32, i32, i32)>,
    /// Device pixels per logical pixel; 1.5 is fine.
    pub scale: Option<f64>,
    pub position: Option<(i32, i32)>,
    /// A `wl_output.transform` value.
    pub transform: Option<u32>,
}

fn parse_mode(text: &str) -> Result<(i32, i32, i32), String> {
    let bad = || format!("mode `{text}` isn't WIDTHxHEIGHT or WIDTHxHEIGHT@HZ");
    let (size, hz) = text
        .split_once('@')
        .map_or((text, None), |(s, r)| (s, Some(r)));
    let (w, h) = size.split_once('x').ok_or_else(bad)?;
    let refresh: f64 = hz.map_or(Ok(0.0), str::parse).map_err(|_| bad())?;
    Ok((
        w.parse().map_err(|_| bad())?,
        h.parse().map_err(|_| bad())?,
        (refresh * 1000.0).round() as i32,
    ))
}

fn parse_transform(text: &str) -> Result<u32, String> {
    const NAMES: [&str; 8] = [
        "normal",
        "90",
        "180",
        "270",
        "flipped",
        "flipped-90",
        "flipped-180",
        "flipped-270",
    ];
    NAMES
        .iter()
        .position(|name| *name == text)
        .map(|index| index as u32)
        .ok_or_else(|| format!("unknown transform `{text}`"))
}

impl RawOutput {
    fn resolve(self) -> Result<OutputConfig, String> {
        if self.scale.is_some_and(|scale| scale <= 0.0) {
            return Err(format!("output {}: scale must be above 0", self.name));
        }
        Ok(OutputConfig {
            mode: self.mode.as_deref().map(parse_mode).transpose()?,
            transform: self.transform.as_deref().map(parse_transform).transpose()?,
            position: self.position.map(|[x, y]| (x, y)),
            scale: self.scale,
            enabled: self.enabled,
            name: self.name,
        })
    }
}

/// X11 through xwayland-satellite, started by the first X client that connects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XwaylandConfig {
    pub enabled: bool,
    pub path: String,
}

/// An xkb keymap by name. Empty fields follow `XKB_DEFAULT_*`, so an empty config
/// keeps the keyboard the session already has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeymapNames {
    pub layout: String,
    pub variant: String,
    pub options: String,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct RawConfig {
    input: RawInput,
    layout: RawLayout,
    window: RawWindow,
    animation: RawAnimation,
    overlay: RawOverlay,
    xwayland: RawXwayland,
    output: Vec<RawOutput>,
    background: Option<String>,
    autostart: Vec<String>,
    binds: BTreeMap<String, String>,
    window_rule: Vec<RawRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct RawOutput {
    name: String,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    mode: Option<String>,
    scale: Option<f64>,
    position: Option<[i32; 2]>,
    transform: Option<String>,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct RawXwayland {
    enabled: bool,
    path: String,
}

impl Default for RawXwayland {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "xwayland-satellite".into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct RawInput {
    r#mod: String,
    nested_mod: String,
    focus_follows_mouse: bool,
    touch_output: Option<String>,
    layout: String,
    variant: String,
    options: String,
}

impl Default for RawInput {
    fn default() -> Self {
        Self {
            r#mod: "super".into(),
            nested_mod: "alt".into(),
            focus_follows_mouse: false,
            touch_output: None,
            layout: String::new(),
            variant: String::new(),
            options: String::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct RawLayout {
    r#type: String,
    gap: f64,
    default_width: f64,
    preset_widths: Vec<f64>,
    center_focus: String,
}

impl Default for RawLayout {
    fn default() -> Self {
        let d = LayoutOptions::default();
        Self {
            r#type: d.kind,
            gap: d.gap,
            default_width: d.default_width,
            preset_widths: d.preset_widths,
            center_focus: "never".into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct RawWindow {
    corner_radius: f32,
    border_width: f64,
    active_color: String,
    inactive_color: String,
    shadow_color: String,
    shadow_size: f32,
    blur: bool,
    blur_xray: bool,
    opacity: f32,
}

impl Default for RawWindow {
    fn default() -> Self {
        Self {
            corner_radius: 0.0,
            border_width: 2.0,
            active_color: "#a6c8ff".into(),
            inactive_color: "#4a4f5c".into(),
            shadow_color: "#00000060".into(),
            shadow_size: 20.0,
            blur: true,
            blur_xray: true,
            opacity: 1.0,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct RawAnimation {
    enabled: bool,
    slowdown: f64,
    window_movement: CurveSpec,
    window_open: CurveSpec,
    window_close: CurveSpec,
    workspace_switch: CurveSpec,
    focus_ring: CurveSpec,
    overlay: CurveSpec,
}

impl Default for RawAnimation {
    fn default() -> Self {
        let ease = |ms, easing| CurveSpec::Ease { ms, easing };
        Self {
            enabled: true,
            slowdown: 1.0,
            window_movement: CurveSpec::Spring {
                stiffness: 800.0,
                damping: 1.0,
            },
            window_open: ease(180.0, EasingName::OutCubic),
            window_close: ease(160.0, EasingName::OutQuad),
            workspace_switch: CurveSpec::Spring {
                stiffness: 900.0,
                damping: 1.0,
            },
            focus_ring: ease(120.0, EasingName::OutCubic),
            overlay: ease(140.0, EasingName::OutCubic),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct RawOverlay {
    at_startup: bool,
    font: String,
}

impl Default for RawOverlay {
    fn default() -> Self {
        Self {
            at_startup: true,
            font: "sans-serif".into(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct RawRule {
    app_id: Option<String>,
    title: Option<String>,
    default_width: Option<f64>,
    opacity: Option<f32>,
    blur: Option<bool>,
    floating: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputConfig {
    /// What `Mod` means on bare metal and when nested, as xkb modifier bits.
    pub mod_key: u32,
    pub nested_mod_key: u32,
    pub focus_follows_mouse: bool,
    /// The monitor a touchscreen is glued to. Unset is a laptop's own panel if
    /// there is one, or else the focused monitor.
    pub touch_output: Option<String>,
    pub keymap: KeymapNames,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowConfig {
    pub corner_radius: f32,
    pub border_width: f64,
    pub active_color: Rgba,
    pub inactive_color: Rgba,
    pub shadow_color: Rgba,
    pub shadow_size: f32,
    pub blur: bool,
    /// Blur only the wallpaper and layers, shared between windows. Much cheaper.
    pub blur_xray: bool,
    pub opacity: f32,
}

/// Built curves, ready to hand to animations.
#[derive(Clone)]
pub struct AnimationConfig {
    pub window_movement: Rc<dyn Curve>,
    pub window_open: Rc<dyn Curve>,
    pub window_close: Rc<dyn Curve>,
    pub workspace_switch: Rc<dyn Curve>,
    pub focus_ring: Rc<dyn Curve>,
    pub overlay: Rc<dyn Curve>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayConfig {
    pub at_startup: bool,
    /// A fontconfig family.
    pub font: String,
}

/// A window rule. Patterns are case-sensitive substrings; a rule with neither
/// matches everything. Later rules win field by field.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WindowRule {
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub default_width: Option<f64>,
    pub opacity: Option<f32>,
    pub blur: Option<bool>,
    /// Out of the layout. Dialogs are, unless this says otherwise.
    pub floating: Option<bool>,
}

impl WindowRule {
    pub fn matches(&self, app_id: &str, title: &str) -> bool {
        let hit = |pattern: &Option<String>, text: &str| {
            pattern.as_ref().is_none_or(|p| text.contains(p.as_str()))
        };
        hit(&self.app_id, app_id) && hit(&self.title, title)
    }
}

/// What the matching rules add up to for one window.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResolvedRule {
    pub default_width: Option<f64>,
    pub opacity: Option<f32>,
    pub blur: Option<bool>,
    /// Out of the layout. Dialogs are, unless this says otherwise.
    pub floating: Option<bool>,
}

#[derive(Clone)]
pub struct Config {
    pub input: InputConfig,
    pub layout: LayoutOptions,
    pub window: WindowConfig,
    pub animation: AnimationConfig,
    pub overlay: OverlayConfig,
    pub xwayland: XwaylandConfig,
    pub outputs: Vec<OutputConfig>,
    pub background: Rgba,
    /// Shell commands run once, at startup.
    pub autostart: Vec<String>,
    pub binds: Binds,
    pub rules: Vec<WindowRule>,
}

impl Config {
    pub fn rule_for(&self, app_id: &str, title: &str) -> ResolvedRule {
        let mut resolved = ResolvedRule::default();
        for rule in self.rules.iter().filter(|r| r.matches(app_id, title)) {
            resolved.default_width = rule.default_width.or(resolved.default_width);
            resolved.opacity = rule.opacity.or(resolved.opacity);
            resolved.blur = rule.blur.or(resolved.blur);
            resolved.floating = rule.floating.or(resolved.floating);
        }
        resolved
    }
}

fn fraction(name: &str, value: f64) -> Result<f64, String> {
    if value > 0.0 && value <= 1.0 {
        Ok(value)
    } else {
        Err(format!("{name} must be above 0 and at most 1, got {value}"))
    }
}

impl RawConfig {
    pub(crate) fn resolve(self) -> Result<Config, String> {
        let center_focus = match self.layout.center_focus.as_str() {
            "never" => CenterFocus::Never,
            "always" => CenterFocus::Always,
            "on-overflow" => CenterFocus::OnOverflow,
            other => return Err(format!("center-focus: unknown value `{other}`")),
        };
        if !LAYOUTS.contains(&self.layout.r#type.as_str()) {
            let known = LAYOUTS.join(", ");
            return Err(format!(
                "layout type `{}` is not one of: {known}",
                self.layout.r#type
            ));
        }
        if self.layout.gap < 0.0 {
            return Err("gap can't be negative".into());
        }
        fraction("default-width", self.layout.default_width)?;
        for &width in &self.layout.preset_widths {
            fraction("preset-widths", width)?;
        }
        let mut preset_widths = self.layout.preset_widths;
        preset_widths.sort_by(f64::total_cmp);

        let mut binds = defaults::binds();
        for (combo, action) in &self.binds {
            let parsed: Combo = combo.parse()?;
            let action: Action = action
                .parse()
                .map_err(|e| format!("binds.\"{combo}\": {e}"))?;
            binds.set(parsed, action);
        }

        let w = self.window;
        let a = self.animation;
        if a.slowdown <= 0.0 {
            return Err("slowdown must be above 0".into());
        }
        let curve = |spec: CurveSpec| -> Rc<dyn Curve> {
            if a.enabled {
                spec.build(a.slowdown)
            } else {
                Rc::new(Snap)
            }
        };
        let rules = self.window_rule.into_iter().map(|r| WindowRule {
            app_id: r.app_id,
            title: r.title,
            default_width: r.default_width,
            opacity: r.opacity,
            blur: r.blur,
            floating: r.floating,
        });
        Ok(Config {
            input: InputConfig {
                mod_key: keys::parse_modifier(&self.input.r#mod)?,
                nested_mod_key: keys::parse_modifier(&self.input.nested_mod)?,
                focus_follows_mouse: self.input.focus_follows_mouse,
                touch_output: self.input.touch_output,
                keymap: KeymapNames {
                    layout: self.input.layout,
                    variant: self.input.variant,
                    options: self.input.options,
                },
            },
            layout: LayoutOptions {
                kind: self.layout.r#type,
                gap: self.layout.gap,
                default_width: self.layout.default_width,
                preset_widths,
                center_focus,
            },
            window: WindowConfig {
                corner_radius: w.corner_radius,
                border_width: w.border_width,
                active_color: Rgba::parse(&w.active_color)?,
                inactive_color: Rgba::parse(&w.inactive_color)?,
                shadow_color: Rgba::parse(&w.shadow_color)?,
                shadow_size: w.shadow_size,
                blur: w.blur,
                blur_xray: w.blur_xray,
                opacity: w.opacity.clamp(0.0, 1.0),
            },
            animation: AnimationConfig {
                window_movement: curve(a.window_movement),
                window_open: curve(a.window_open),
                window_close: curve(a.window_close),
                workspace_switch: curve(a.workspace_switch),
                focus_ring: curve(a.focus_ring),
                overlay: curve(a.overlay),
            },
            overlay: OverlayConfig {
                at_startup: self.overlay.at_startup,
                font: self.overlay.font,
            },
            outputs: self
                .output
                .into_iter()
                .map(RawOutput::resolve)
                .collect::<Result<_, _>>()?,
            xwayland: XwaylandConfig {
                enabled: self.xwayland.enabled,
                path: self.xwayland.path,
            },
            background: Rgba::parse(self.background.as_deref().unwrap_or("#1e2028"))?,
            autostart: self.autostart,
            binds,
            rules: rules.collect(),
        })
    }
}
