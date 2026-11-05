//! The hotkey overlay: a blurred panel listing what's bound, as niri's. Any key
//! dismisses it. Text is rasterized once and kept as an image until the binds change.

use std::rc::Rc;

use luminaria::shell::SceneItem;
use luminaria::text::Font;
use luminaria::util::{Color, Keysym};
use pastel_config::{Config, OverlayRow, ResolvedRule, Rgba, Section, WindowConfig};
use pastel_core::{Anim, Curve, Rect};
use pastel_layout::WindowId;

use crate::effect::{Pipeline, WindowFrame, color};
use crate::overlay::{Overlay, OverlayContext};
use crate::to_pixels;

const FONT_SIZE: f32 = 11.0;
const TITLE_SIZE: f32 = 15.0;
const PADDING: i32 = 28;
const COLUMN_GAP: i32 = 44;
const KEY_GAP: i32 = 20;
const SECTION_GAP: i32 = 14;
const LINE_GAP: i32 = 5;
const SLIDE: f64 = 12.0;
/// Premultiplied RGBA8 of the panel, baked into the image with the text so the two
/// are one item.
const PANEL: [u8; 4] = [19, 21, 27, 209];
const TEXT: Color = Color::rgb(0.92, 0.93, 0.95);
const DIM: Color = Color::rgb(0.60, 0.63, 0.70);
/// Luminaria keeps images by this number; Pastel has only the one.
const IMAGE_SERIAL: u64 = 1;

struct Raster {
    width: i32,
    height: i32,
}

struct Style {
    font: String,
    accent: Rgba,
}

pub struct HotkeyOverlay {
    style: Style,
    /// The same one windows go through: border, corners, shadow and blur all
    /// follow `[window]`, so the panel looks like it belongs.
    pipeline: Pipeline,
    rows: Vec<OverlayRow>,
    raster: Option<Raster>,
    /// What it was rasterized for: another monitor can want it sharper.
    raster_scale: i32,
    /// Text can't be drawn at all; say so once rather than every frame.
    failed: bool,
    fade: Anim,
    open: bool,
}

impl HotkeyOverlay {
    pub fn new(config: &Config, mod_key: u32) -> Self {
        let curve: Rc<dyn Curve> = config.animation.overlay.clone();
        Self {
            style: Style {
                font: config.overlay.font.clone(),
                accent: config.window.active_color,
            },
            pipeline: pipeline(&config.window),
            rows: config.binds.overlay_rows(mod_key),
            raster: None,
            raster_scale: 1,
            failed: false,
            fade: Anim::new(0.0, curve),
            open: false,
        }
    }

    pub fn reconfigure(&mut self, config: &Config, mod_key: u32) {
        self.style = Style {
            font: config.overlay.font.clone(),
            accent: config.window.active_color,
        };
        self.pipeline = pipeline(&config.window);
        self.rows = config.binds.overlay_rows(mod_key);
        self.raster = None;
        self.failed = false;
        self.fade.set_curve(config.animation.overlay.clone());
    }

    pub fn set_open(&mut self, open: bool, now: f64) {
        self.open = open;
        self.fade.set(f64::from(u8::from(open)), now);
    }

    pub fn toggle(&mut self, now: f64) {
        self.set_open(!self.open, now);
    }

    /// In image pixels: everything is `scale` times its logical size, and the
    /// whole divides back to a whole number of logical pixels.
    fn rasterize(&self, scale: i32) -> Option<(Raster, Vec<u8>)> {
        let times = scale as f32;
        let open = |size: f32| Font::open(&format!("{}:size={size}", self.style.font));
        let (mut body, mut heading) = (open(FONT_SIZE * times)?, open(TITLE_SIZE * times)?);
        let padding = PADDING * scale;
        let (key_gap, column_gap) = (KEY_GAP * scale, COLUMN_GAP * scale);
        let section_gap = SECTION_GAP * scale;
        let line = body.height() + LINE_GAP * scale;
        let columns = columns(&self.rows);

        // Measure first: the canvas has to fit what's written on it.
        let mut key_widths = Vec::new();
        let mut text_widths = Vec::new();
        for column in &columns {
            let key = column.iter().flat_map(|(_, rows)| *rows);
            key_widths.push(key.map(|r| body.measure(&r.keys)).max().unwrap_or(0));
            let text = column.iter().flat_map(|(_, rows)| *rows);
            text_widths.push(text.map(|r| body.measure(&r.title)).max().unwrap_or(0));
        }
        let inner: i32 = key_widths
            .iter()
            .zip(&text_widths)
            .map(|(k, t)| k + key_gap + t)
            .sum();
        let gaps = column_gap * (columns.len() as i32 - 1).max(0);
        let width = whole(inner + gaps + 2 * padding, scale);
        let header = heading.height() + section_gap;
        let tallest = columns
            .iter()
            .map(|c| column_height(c, line, section_gap))
            .max()
            .unwrap_or(0);
        let height = whole(header + tallest + 2 * padding, scale);

        let mut canvas: Vec<u8> = PANEL
            .iter()
            .copied()
            .cycle()
            .take(width as usize * height as usize * 4)
            .collect();
        let (w, h) = (width, height);
        let mut title_pen = |font: &mut Font, x, y, text: &str, ink| {
            let _ = font.draw(&mut canvas, w, h, x, y, text, ink, w - padding);
        };
        let baseline = padding + heading.ascent();
        title_pen(&mut heading, padding, baseline, "Hotkeys", TEXT);

        let accent = color(self.style.accent);
        let mut left = padding;
        for (i, column) in columns.iter().enumerate() {
            let mut y = padding + header + body.ascent();
            for (section, rows) in column {
                title_pen(&mut body, left, y, section.title(), DIM);
                y += line;
                for row in *rows {
                    title_pen(&mut body, left, y, &row.keys, accent);
                    let x = left + key_widths[i] + key_gap;
                    title_pen(&mut body, x, y, &row.title, TEXT);
                    y += line;
                }
                y += section_gap;
            }
            left += key_widths[i] + key_gap + text_widths[i] + column_gap;
        }
        Some((Raster { width, height }, canvas))
    }
}

/// The windows' look, except that the blur is always real: x-ray samples only the
/// wallpaper, which is right for tiles that never overlap but not for a panel
/// floating over them.
fn pipeline(window: &WindowConfig) -> Pipeline {
    Pipeline::from_config(&WindowConfig {
        blur_xray: false,
        ..*window
    })
}

type Group<'a> = (Section, &'a [OverlayRow]);

/// Sections divided into two columns, as evenly as whole sections allow.
fn columns(rows: &[OverlayRow]) -> Vec<Vec<Group<'_>>> {
    let groups: Vec<Group<'_>> = Section::ALL
        .iter()
        .filter_map(|&section| {
            let start = rows.iter().position(|r| r.section == section)?;
            let len = rows[start..]
                .iter()
                .take_while(|r| r.section == section)
                .count();
            Some((section, &rows[start..start + len]))
        })
        .collect();
    let half = rows.len() / 2;
    let mut seen = 0;
    let split = groups
        .iter()
        .position(|(_, rows)| {
            seen += rows.len();
            seen > half
        })
        .map_or(groups.len(), |i| i + 1)
        .min(groups.len().saturating_sub(1).max(1));
    let (a, b) = groups.split_at(split.min(groups.len()));
    let mut out = vec![a.to_vec()];
    if !b.is_empty() {
        out.push(b.to_vec());
    }
    out
}

fn column_height(column: &[Group<'_>], line: i32, section_gap: i32) -> i32 {
    let rows: usize = column.iter().map(|(_, rows)| rows.len() + 1).sum();
    rows as i32 * line + section_gap * (column.len() as i32 - 1).max(0)
}

/// `value` rounded up to a multiple of `scale`.
fn whole(value: i32, scale: i32) -> i32 {
    (value + scale - 1) / scale * scale
}

impl Overlay for HotkeyOverlay {
    fn is_modal(&self) -> bool {
        self.open
    }

    fn on_key(&mut self, _keysym: Keysym, now: f64) {
        self.set_open(false, now);
    }

    fn draw(&mut self, context: &OverlayContext<'_>, items: &mut Vec<SceneItem>) -> bool {
        let now = context.now;
        let animating = self.fade.tick(now);
        let alpha = self.fade.value(now).clamp(0.0, 1.0);
        if alpha <= 0.0 && !self.open {
            return animating;
        }
        let scale = context.scale.max(1);
        if scale != self.raster_scale {
            self.raster = None;
            self.raster_scale = scale;
        }
        if self.raster.is_none() && !self.failed {
            match self.rasterize(scale) {
                Some((raster, canvas)) => {
                    let (w, h) = (raster.width, raster.height);
                    match context.renderer.set_image(IMAGE_SERIAL, w, h, &canvas) {
                        Ok(()) => self.raster = Some(raster),
                        Err(e) => {
                            eprintln!("pastel: hotkey overlay: {e}");
                            self.failed = true;
                        }
                    }
                }
                None => {
                    eprintln!("pastel: hotkey overlay: no font for `{}`", self.style.font);
                    self.failed = true;
                }
            }
        }
        let Some(raster) = &self.raster else {
            return animating;
        };
        let (w, h) = (
            f64::from(raster.width / scale),
            f64::from(raster.height / scale),
        );
        let (cx, cy) = context.monitor.center();
        let x = (cx - w / 2.0).round();
        let y = (cy - h / 2.0 + (1.0 - alpha) * SLIDE).round();
        let frame = WindowFrame {
            id: WindowId(0),
            rect: Rect::new(x, y, w, h),
            alpha: alpha as f32,
            focus: 1.0,
            // Whatever opacity windows have, the text has to stay readable.
            rule: ResolvedRule {
                opacity: Some(1.0),
                ..ResolvedRule::default()
            },
            fullscreen: false,
        };
        let start = items.len();
        self.pipeline.below(&frame, items);
        let mut panel =
            SceneItem::image(to_pixels(frame.rect), IMAGE_SERIAL).with_alpha(frame.alpha);
        self.pipeline.style(&frame, &mut panel);
        items.push(panel);
        for item in &mut items[start..] {
            item.accepts_input = false;
        }
        animating
    }
}
