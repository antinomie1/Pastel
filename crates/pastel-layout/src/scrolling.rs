//! Scrolling columns, after niri: an infinite strip of columns, each holding
//! windows stacked vertically, panned past the screen. Opening a window never
//! resizes the others.

use pastel_core::{Direction, Rect};

use crate::{CenterFocus, Layout, LayoutOp, LayoutOptions, Placement, WindowHint, WindowId};

const MIN_WIDTH: f64 = 0.1;
const MAX_WIDTH: f64 = 1.0;

struct Column {
    windows: Vec<WindowId>,
    active: usize,
    /// Fraction of the working area.
    width: f64,
    maximized: bool,
}

impl Column {
    fn new(id: WindowId, width: f64) -> Self {
        Self { windows: vec![id], active: 0, width, maximized: false }
    }

    fn focused(&self) -> WindowId {
        self.windows[self.active]
    }
}

pub struct Scrolling {
    options: LayoutOptions,
    area: Rect,
    columns: Vec<Column>,
    active: usize,
    /// The strip coordinate at the left edge of the area.
    view: f64,
}

impl Scrolling {
    pub fn new(options: LayoutOptions) -> Self {
        let view = -options.gap;
        Self { options, area: Rect::default(), columns: Vec::new(), active: 0, view }
    }

    fn gap(&self) -> f64 {
        self.options.gap
    }

    fn column_width(&self, column: &Column) -> f64 {
        let (w, gap) = (self.area.w, self.gap());
        let px = if column.maximized { w - 2.0 * gap } else { (w - gap) * column.width - gap };
        px.round().max(1.0)
    }

    /// Where a column starts on the strip.
    fn column_x(&self, index: usize) -> f64 {
        let widths = self.columns[..index].iter().map(|c| self.column_width(c) + self.gap());
        widths.sum()
    }

    fn strip_width(&self) -> f64 {
        match self.columns.len() {
            0 => 0.0,
            n => self.column_x(n - 1) + self.column_width(&self.columns[n - 1]),
        }
    }

    fn centered_view(&self, index: usize) -> f64 {
        let (x, w) = (self.column_x(index), self.column_width(&self.columns[index]));
        (x + w / 2.0 - self.area.w / 2.0).round()
    }

    /// Scroll the least that shows the column; the left edge wins if it can't fit.
    fn nearest_view(&self, index: usize) -> f64 {
        let (x, w) = (self.column_x(index), self.column_width(&self.columns[index]));
        let gap = self.gap();
        let right_aligned = x + w + gap - self.area.w;
        self.view.max(right_aligned).min(x - gap)
    }

    /// Keep the view from wandering past either end of the strip.
    fn clamped(&self, view: f64) -> f64 {
        let gap = self.gap();
        let last = self.strip_width() + gap - self.area.w;
        view.min(last.max(-gap)).max(-gap)
    }

    /// Make the focused column visible, as the policy says.
    fn reveal(&mut self) {
        if self.columns.is_empty() {
            self.view = -self.gap();
            return;
        }
        let index = self.active;
        self.view = match self.options.center_focus {
            CenterFocus::Always => self.centered_view(index),
            CenterFocus::Never => self.clamped(self.nearest_view(index)),
            CenterFocus::OnOverflow => {
                let nearest = self.clamped(self.nearest_view(index));
                if nearest == self.view { nearest } else { self.centered_view(index) }
            }
        };
    }

    fn find(&self, id: WindowId) -> Option<(usize, usize)> {
        self.columns.iter().enumerate().find_map(|(c, column)| {
            column.windows.iter().position(|&w| w == id).map(|w| (c, w))
        })
    }

    fn focus_column(&mut self, index: usize) -> bool {
        if index >= self.columns.len() || index == self.active {
            return false;
        }
        self.active = index;
        self.reveal();
        true
    }

    fn move_column(&mut self, index: usize) -> bool {
        if index >= self.columns.len() || index == self.active {
            return false;
        }
        let column = self.columns.remove(self.active);
        self.columns.insert(index, column);
        self.active = index;
        self.reveal();
        true
    }

    fn shift_within_column(&mut self, up: bool, swap: bool) -> bool {
        let Some(column) = self.columns.get_mut(self.active) else { return false };
        let target = if up { column.active.checked_sub(1) } else { Some(column.active + 1) };
        let Some(target) = target.filter(|&t| t < column.windows.len()) else { return false };
        if swap {
            column.windows.swap(column.active, target);
        }
        column.active = target;
        true
    }

    fn consume_or_expel(&mut self, left: bool) -> bool {
        let index = self.active;
        let Some(column) = self.columns.get_mut(index) else { return false };
        if column.windows.len() > 1 {
            let id = column.windows.remove(column.active);
            column.active = column.active.min(column.windows.len() - 1);
            let width = column.width;
            let at = if left { index } else { index + 1 };
            self.columns.insert(at, Column::new(id, width));
            self.active = at;
        } else {
            let neighbour = if left { index.checked_sub(1) } else { Some(index + 1) };
            let Some(neighbour) = neighbour.filter(|&n| n < self.columns.len()) else {
                return false;
            };
            let id = self.columns.remove(index).focused();
            let into = if neighbour > index { neighbour - 1 } else { neighbour };
            let column = &mut self.columns[into];
            column.windows.push(id);
            column.active = column.windows.len() - 1;
            self.active = into;
        }
        self.reveal();
        true
    }

    fn cycle_width(&mut self) -> bool {
        let presets = self.options.preset_widths.clone();
        let Some(column) = self.columns.get_mut(self.active) else { return false };
        let Some(&first) = presets.first() else { return false };
        let next = presets.iter().copied().find(|&p| p > column.width + 1e-3).unwrap_or(first);
        column.width = next;
        column.maximized = false;
        self.reveal();
        true
    }

    fn adjust_width(&mut self, delta: f64) -> bool {
        let Some(column) = self.columns.get_mut(self.active) else { return false };
        let width = (column.width + delta).clamp(MIN_WIDTH, MAX_WIDTH);
        let changed = width != column.width;
        column.width = width;
        column.maximized = false;
        self.reveal();
        changed
    }
}

impl Layout for Scrolling {
    fn name(&self) -> &'static str {
        "scrolling"
    }

    fn set_area(&mut self, area: Rect) {
        self.area = area;
        self.reveal();
    }

    fn add(&mut self, id: WindowId, hint: WindowHint) {
        let width = hint.width.unwrap_or(self.options.default_width);
        let at = if self.columns.is_empty() { 0 } else { self.active + 1 };
        self.columns.insert(at, Column::new(id, width.clamp(MIN_WIDTH, MAX_WIDTH)));
        self.active = at;
        self.reveal();
    }

    fn remove(&mut self, id: WindowId) -> bool {
        let Some((c, w)) = self.find(id) else { return false };
        let column = &mut self.columns[c];
        column.windows.remove(w);
        if column.windows.is_empty() {
            self.columns.remove(c);
            if c < self.active {
                self.active -= 1;
            }
            self.active = self.active.min(self.columns.len().saturating_sub(1));
        } else if w < column.active || column.active == column.windows.len() {
            column.active -= 1;
        }
        self.reveal();
        true
    }

    fn contains(&self, id: WindowId) -> bool {
        self.find(id).is_some()
    }

    fn len(&self) -> usize {
        self.columns.iter().map(|c| c.windows.len()).sum()
    }

    fn focused(&self) -> Option<WindowId> {
        self.columns.get(self.active).map(Column::focused)
    }

    fn focus(&mut self, id: WindowId) -> bool {
        let Some((c, w)) = self.find(id) else { return false };
        self.columns[c].active = w;
        self.active = c;
        self.reveal();
        true
    }

    fn apply(&mut self, op: LayoutOp) -> bool {
        use Direction::{Down, Left, Right, Up};
        let last = self.columns.len().saturating_sub(1);
        match op {
            LayoutOp::Focus(Left) => self.active > 0 && self.focus_column(self.active - 1),
            LayoutOp::Focus(Right) => self.focus_column(self.active + 1),
            LayoutOp::Focus(Up) => self.shift_within_column(true, false),
            LayoutOp::Focus(Down) => self.shift_within_column(false, false),
            LayoutOp::Move(Left) => self.active > 0 && self.move_column(self.active - 1),
            LayoutOp::Move(Right) => self.move_column(self.active + 1),
            LayoutOp::Move(Up) => self.shift_within_column(true, true),
            LayoutOp::Move(Down) => self.shift_within_column(false, true),
            LayoutOp::FocusFirst => self.focus_column(0),
            LayoutOp::FocusLast => self.focus_column(last),
            LayoutOp::MoveFirst => self.move_column(0),
            LayoutOp::MoveLast => self.move_column(last),
            LayoutOp::ConsumeOrExpel(direction) => match direction {
                Left | Up => self.consume_or_expel(true),
                Right | Down => self.consume_or_expel(false),
            },
            LayoutOp::CycleWidth => self.cycle_width(),
            LayoutOp::AdjustWidth(delta) => self.adjust_width(delta),
            LayoutOp::ToggleMaximize => {
                let Some(column) = self.columns.get_mut(self.active) else { return false };
                column.maximized = !column.maximized;
                self.reveal();
                true
            }
            LayoutOp::Center => {
                if self.columns.is_empty() {
                    return false;
                }
                self.view = self.centered_view(self.active);
                true
            }
            LayoutOp::Scroll(dx) => {
                let view = self.clamped((self.view + dx).round());
                let moved = view != self.view;
                self.view = view;
                moved
            }
        }
    }

    fn place(&self, out: &mut Vec<Placement>) {
        let gap = self.gap();
        let inner = (self.area.h - 2.0 * gap).max(1.0);
        let mut x = self.area.x - self.view;
        for column in &self.columns {
            let w = self.column_width(column);
            let n = column.windows.len() as f64;
            let h = ((inner - gap * (n - 1.0)) / n).floor().max(1.0);
            let mut y = self.area.y + gap;
            for (i, &id) in column.windows.iter().enumerate() {
                // The last window takes what rounding left over.
                let last = i + 1 == column.windows.len();
                let height = if last { self.area.y + gap + inner - y } else { h };
                out.push(Placement { id, rect: Rect::new(x, y, w, height), fullscreen: false });
                y += h + gap;
            }
            x += w + gap;
        }
    }
}
