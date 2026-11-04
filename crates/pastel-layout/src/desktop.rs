//! Monitors, each with a vertical stack of workspaces, each with a [`Layout`].
//!
//! Workspaces are dynamic as in niri: there is always exactly one empty workspace
//! at the bottom, and an empty workspace disappears once it stops being looked at.
//! With [`LayoutOptions::workspaces`] set, every monitor instead keeps that many,
//! empty or not, as dwm's tags; only workspaces past them come and go.

use pastel_core::{Direction, Rect};

use crate::{Layout, LayoutOp, LayoutOptions, Placement, WindowHint, WindowId, create};

/// Which workspace an action means, on the focused monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTarget {
    Up,
    Down,
    /// Zero-based. Past the end means the empty one at the bottom.
    Index(usize),
}

/// A window out of the layout, somewhere of its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Floating {
    pub id: WindowId,
    pub rect: Rect,
    /// False until the client has said what size it wants.
    pub sized: bool,
    /// Under the pointer's control: follows it exactly, with no animation.
    pub interactive: bool,
}

pub struct Workspace {
    /// Stable across reordering, unlike its index.
    pub id: u64,
    pub layout: Box<dyn Layout>,
    /// Bottom to top. The layout never hears about these, so any `Layout` has them.
    pub floating: Vec<Floating>,
    /// The keyboard is with the top floating window, not the layout.
    pub focus_floating: bool,
    /// At most one, and always a window of this workspace.
    pub fullscreen: Option<WindowId>,
}

impl Workspace {
    pub fn is_empty(&self) -> bool {
        self.layout.is_empty() && self.floating.is_empty()
    }

    pub fn contains(&self, id: WindowId) -> bool {
        self.layout.contains(id) || self.floating.iter().any(|f| f.id == id)
    }

    fn focused(&self) -> Option<WindowId> {
        let floating = self.floating.last().filter(|_| self.focus_floating);
        self.fullscreen
            .or(floating.map(|f| f.id))
            .or_else(|| self.layout.focused())
    }

    /// Out of the workspace, with where it floated if it did.
    fn take(&mut self, id: WindowId) -> Option<Option<Floating>> {
        if self.fullscreen == Some(id) {
            self.fullscreen = None;
        }
        if let Some(index) = self.floating.iter().position(|f| f.id == id) {
            let taken = self.floating.remove(index);
            self.focus_floating &= !self.floating.is_empty();
            return Some(Some(taken));
        }
        self.layout.remove(id).then_some(None)
    }

    fn put(&mut self, id: WindowId, floating: Option<Floating>) {
        match floating {
            Some(floating) => {
                self.floating.push(floating);
                self.focus_floating = true;
            }
            None => {
                self.layout.add(id, WindowHint::default());
                self.focus_floating = false;
            }
        }
    }

    fn shift_floating(&mut self, dx: f64, dy: f64) {
        for floating in &mut self.floating {
            floating.rect = floating.rect.translated(dx, dy);
        }
    }
}

pub struct Monitor {
    pub id: u64,
    /// The whole output, in layout coordinates.
    pub rect: Rect,
    /// `rect` minus panels' exclusive zones.
    pub usable: Rect,
    pub workspaces: Vec<Workspace>,
    pub active: usize,
    view_shift: f64,
}

impl Monitor {
    pub fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active]
    }

    fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active]
    }

    /// How far workspace indices moved since last asked, because one above the
    /// active one was removed. The renderer applies this to its animated view so
    /// the removal doesn't read as a jump.
    pub fn take_view_shift(&mut self) -> f64 {
        std::mem::take(&mut self.view_shift)
    }

    /// Hold the invariants: the last workspace, and only the last, is empty,
    /// unless it is the one being looked at. With `fixed` workspaces, the first
    /// `fixed` always exist and only empty ones past them, unlooked at, go.
    fn normalize(&mut self, fixed: usize, new_workspace: &mut impl FnMut() -> Workspace) {
        if fixed > 0 {
            self.drop_empty(fixed, self.workspaces.len());
            while self.workspaces.len() < fixed {
                self.workspaces.push(new_workspace());
            }
            return;
        }
        self.drop_empty(0, self.workspaces.len().saturating_sub(1));
        // All that can be left of a double empty: the active one above the spare.
        let n = self.workspaces.len();
        if n >= 2
            && self.active == n - 2
            && self.workspaces[n - 2].is_empty()
            && self.workspaces[n - 1].is_empty()
        {
            self.workspaces.pop();
        }
        if !self.workspaces.last().is_some_and(Workspace::is_empty) {
            self.workspaces.push(new_workspace());
        }
    }

    /// Removes the empty workspaces from `from` up to before `to` (counted
    /// before removing), except the one being looked at.
    fn drop_empty(&mut self, from: usize, to: usize) {
        let (mut index, mut left) = (from, to.saturating_sub(from));
        while left > 0 && index < self.workspaces.len() {
            left -= 1;
            if index != self.active && self.workspaces[index].is_empty() {
                self.workspaces.remove(index);
                if index < self.active {
                    self.active -= 1;
                    self.view_shift -= 1.0;
                }
            } else {
                index += 1;
            }
        }
    }
}

/// How far `move-column-*` carries a floating window.
const NUDGE: f64 = 40.0;

fn nudge(direction: Direction) -> (f64, f64) {
    match direction {
        Direction::Left => (-NUDGE, 0.0),
        Direction::Right => (NUDGE, 0.0),
        Direction::Up => (0.0, -NUDGE),
        Direction::Down => (0.0, NUDGE),
    }
}

/// A box this size in the middle of `area`.
fn centered(area: Rect, width: f64, height: f64) -> Rect {
    let (cx, cy) = area.center();
    Rect::new(
        (cx - width / 2.0).round(),
        (cy - height / 2.0).round(),
        width,
        height,
    )
}

pub struct Desktop {
    options: LayoutOptions,
    monitors: Vec<Monitor>,
    focused: usize,
    next_workspace: u64,
    /// Workspaces with no monitor to live on: at startup, or all unplugged.
    parked: Vec<Workspace>,
}

impl Desktop {
    /// `None` if `options` names no known layout.
    pub fn new(options: LayoutOptions) -> Option<Self> {
        create(&options)?;
        Some(Self {
            options,
            monitors: Vec::new(),
            focused: 0,
            next_workspace: 0,
            parked: Vec::new(),
        })
    }

    /// Takes effect for workspaces created from now on.
    pub fn set_options(&mut self, options: LayoutOptions) -> bool {
        if create(&options).is_none() {
            return false;
        }
        let recount = options.workspaces != self.options.workspaces;
        self.options = options;
        if recount {
            for index in 0..self.monitors.len() {
                self.normalize(index);
            }
        }
        true
    }

    fn new_workspace(options: &LayoutOptions, next: &mut u64, area: Rect) -> Workspace {
        let mut layout = create(options).expect("options were validated");
        layout.set_area(area);
        *next += 1;
        Workspace {
            id: *next,
            layout,
            floating: Vec::new(),
            focus_floating: false,
            fullscreen: None,
        }
    }

    fn normalize(&mut self, index: usize) {
        let Self {
            options,
            monitors,
            next_workspace,
            ..
        } = self;
        let monitor = &mut monitors[index];
        let area = monitor.usable;
        let fixed = options.workspaces;
        monitor.normalize(fixed, &mut || Self::new_workspace(options, next_workspace, area));
    }

    pub fn monitors(&self) -> &[Monitor] {
        &self.monitors
    }

    pub fn monitors_mut(&mut self) -> &mut [Monitor] {
        &mut self.monitors
    }

    pub fn monitor(&self, id: u64) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.id == id)
    }

    pub fn focused_monitor(&self) -> Option<&Monitor> {
        self.monitors.get(self.focused)
    }

    /// A new output arrived. Adopts any windows that had nowhere to be.
    pub fn add_monitor(&mut self, id: u64, rect: Rect) {
        let mut workspaces = std::mem::take(&mut self.parked);
        for workspace in &mut workspaces {
            workspace.layout.set_area(rect);
        }
        self.monitors.push(Monitor {
            id,
            rect,
            usable: rect,
            workspaces,
            active: 0,
            view_shift: 0.0,
        });
        self.normalize(self.monitors.len() - 1);
    }

    /// An output left. Its windows move to the first remaining monitor.
    pub fn remove_monitor(&mut self, id: u64) {
        let Some(index) = self.monitors.iter().position(|m| m.id == id) else {
            return;
        };
        let monitor = self.monitors.remove(index);
        let origin = (monitor.rect.x, monitor.rect.y);
        let occupied = monitor.workspaces.into_iter().filter(|w| !w.is_empty());
        match self.monitors.first_mut() {
            Some(first) => {
                // Above the spare empty one; after the fixed ones, which keep
                // their numbers.
                let spare = usize::from(self.options.workspaces == 0);
                let at = first.workspaces.len() - spare;
                let area = first.usable;
                let (dx, dy) = (first.rect.x - origin.0, first.rect.y - origin.1);
                let mut adopted: Vec<Workspace> = occupied.collect();
                for workspace in &mut adopted {
                    workspace.layout.set_area(area);
                    workspace.shift_floating(dx, dy);
                }
                first.workspaces.splice(at..at, adopted);
            }
            None => self.parked.extend(occupied),
        }
        self.focused = self.focused.min(self.monitors.len().saturating_sub(1));
    }

    /// The output moved or resized, or panels changed what is left of it.
    pub fn set_monitor_geometry(&mut self, id: u64, rect: Rect, usable: Rect) {
        let Some(monitor) = self.monitors.iter_mut().find(|m| m.id == id) else {
            return;
        };
        let (dx, dy) = (rect.x - monitor.rect.x, rect.y - monitor.rect.y);
        monitor.rect = rect;
        monitor.usable = usable;
        for workspace in &mut monitor.workspaces {
            workspace.layout.set_area(usable);
            workspace.shift_floating(dx, dy);
        }
    }

    fn active_workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.monitors
            .get_mut(self.focused)
            .map(Monitor::active_workspace_mut)
    }

    /// The window with the keyboard, as far as layouts are concerned.
    pub fn focused_window(&self) -> Option<WindowId> {
        self.focused_monitor()?.active_workspace().focused()
    }

    /// Put a window on the focused workspace and focus it. With no monitor yet it
    /// waits on a parked workspace.
    pub fn add_window(&mut self, id: WindowId, hint: WindowHint) {
        if let Some(monitor) = self.monitors.get_mut(self.focused) {
            let usable = monitor.usable;
            let workspace = monitor.active_workspace_mut();
            workspace.fullscreen = None;
            if hint.floating {
                workspace.floating.push(Floating {
                    id,
                    rect: centered(usable, usable.w / 2.0, usable.h / 2.0),
                    sized: false,
                    interactive: false,
                });
                workspace.focus_floating = true;
            } else {
                workspace.layout.add(id, hint);
                workspace.focus_floating = false;
            }
            self.normalize(self.focused);
            return;
        }
        if self.parked.is_empty() {
            let workspace =
                Self::new_workspace(&self.options, &mut self.next_workspace, Rect::default());
            self.parked.push(workspace);
        }
        self.parked[0].layout.add(id, hint);
    }

    pub fn remove_window(&mut self, id: WindowId) -> bool {
        for workspace in &mut self.parked {
            if workspace.take(id).is_some() {
                return true;
            }
        }
        for m in 0..self.monitors.len() {
            for workspace in &mut self.monitors[m].workspaces {
                if workspace.take(id).is_some() {
                    self.normalize(m);
                    return true;
                }
            }
        }
        false
    }

    /// Focus a window wherever it is, switching monitor and workspace to it.
    pub fn focus_window(&mut self, id: WindowId) -> bool {
        for (m, monitor) in self.monitors.iter_mut().enumerate() {
            let found = monitor.workspaces.iter().position(|w| w.contains(id));
            let Some(w) = found else { continue };
            monitor.active = w;
            let workspace = monitor.active_workspace_mut();
            if let Some(index) = workspace.floating.iter().position(|f| f.id == id) {
                let raised = workspace.floating.remove(index);
                workspace.floating.push(raised);
                workspace.focus_floating = true;
            } else {
                workspace.layout.focus(id);
                workspace.focus_floating = false;
            }
            if workspace.fullscreen != Some(id) {
                workspace.fullscreen = None;
            }
            self.focused = m;
            self.normalize(m);
            return true;
        }
        false
    }

    /// Run a layout command on the focused workspace.
    pub fn apply(&mut self, op: LayoutOp) -> bool {
        let Some(workspace) = self.active_workspace_mut() else {
            return false;
        };
        if workspace.focus_floating {
            match op {
                // Back to the layout; `focus-floating` comes the other way.
                LayoutOp::Focus(_) | LayoutOp::FocusFirst | LayoutOp::FocusLast => {
                    workspace.focus_floating = false;
                }
                LayoutOp::Move(direction) => {
                    if let Some(top) = workspace.floating.last_mut() {
                        let (dx, dy) = nudge(direction);
                        top.rect = top.rect.translated(dx, dy);
                    }
                    return true;
                }
                LayoutOp::Scroll(_) => {}
                _ => return false,
            }
        }
        let changed = workspace.layout.apply(op);
        if let Some(fullscreen) = workspace.fullscreen
            && workspace.layout.contains(fullscreen)
            && workspace.layout.focused() != Some(fullscreen)
        {
            workspace.fullscreen = None;
        }
        changed
    }

    pub fn is_fullscreen(&self, id: WindowId) -> bool {
        let spaces = self.monitors.iter().flat_map(|m| &m.workspaces);
        spaces.chain(&self.parked).any(|w| w.fullscreen == Some(id))
    }

    pub fn set_fullscreen(&mut self, id: WindowId, on: bool) {
        let spaces = self.monitors.iter_mut().flat_map(|m| &mut m.workspaces);
        if let Some(workspace) = spaces.chain(&mut self.parked).find(|w| w.contains(id)) {
            match (on, workspace.fullscreen) {
                (true, _) => workspace.fullscreen = Some(id),
                (false, Some(current)) if current == id => workspace.fullscreen = None,
                _ => {}
            }
        }
    }

    pub fn toggle_fullscreen(&mut self) -> bool {
        let Some(id) = self.focused_window() else {
            return false;
        };
        let on = !self.is_fullscreen(id);
        self.set_fullscreen(id, on);
        true
    }

    fn resolve(monitor: &Monitor, target: WorkspaceTarget) -> usize {
        let last = monitor.workspaces.len() - 1;
        match target {
            WorkspaceTarget::Up => monitor.active.saturating_sub(1),
            WorkspaceTarget::Down => (monitor.active + 1).min(last),
            WorkspaceTarget::Index(index) => index.min(last),
        }
    }

    pub fn focus_workspace(&mut self, target: WorkspaceTarget) -> bool {
        let Some(monitor) = self.monitors.get_mut(self.focused) else {
            return false;
        };
        let index = Self::resolve(monitor, target);
        if index == monitor.active {
            return false;
        }
        monitor.active = index;
        self.normalize(self.focused);
        true
    }

    /// Send the focused window to a workspace and go with it.
    pub fn move_to_workspace(&mut self, target: WorkspaceTarget) -> bool {
        let Some(id) = self.focused_window() else {
            return false;
        };
        let monitor = &mut self.monitors[self.focused];
        let index = Self::resolve(monitor, target);
        if index == monitor.active {
            return false;
        }
        let floating = monitor.active_workspace_mut().take(id).flatten();
        monitor.active = index;
        monitor.active_workspace_mut().put(id, floating);
        self.normalize(self.focused);
        true
    }

    /// The monitor whose centre lies furthest along `direction` from the focused
    /// one, nearest first.
    fn neighbour(&self, direction: Direction) -> Option<usize> {
        let (cx, cy) = self.focused_monitor()?.rect.center();
        let score = |rect: &Rect| {
            let (x, y) = rect.center();
            let along = match direction {
                Direction::Left => cx - x,
                Direction::Right => x - cx,
                Direction::Up => cy - y,
                Direction::Down => y - cy,
            };
            (along > 0.0).then_some(along)
        };
        let scored = self.monitors.iter().enumerate();
        scored
            .filter_map(|(i, m)| score(&m.rect).map(|s| (i, s)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    pub fn focus_monitor(&mut self, direction: Direction) -> bool {
        let Some(index) = self.neighbour(direction) else {
            return false;
        };
        self.focused = index;
        true
    }

    /// Send the focused window to the neighbouring monitor and go with it.
    pub fn move_to_monitor(&mut self, direction: Direction) -> bool {
        let Some(index) = self.neighbour(direction) else {
            return false;
        };
        let Some(id) = self.focused_window() else {
            return false;
        };
        let from = self.focused;
        let origin = self.monitors[from].rect;
        let target = self.monitors[index].rect;
        let mut floating = self.monitors[from]
            .active_workspace_mut()
            .take(id)
            .flatten();
        if let Some(floating) = &mut floating {
            floating.rect = floating
                .rect
                .translated(target.x - origin.x, target.y - origin.y);
        }
        self.normalize(from);
        self.focused = index;
        self.monitors[index]
            .active_workspace_mut()
            .put(id, floating);
        self.normalize(index);
        true
    }

    fn floating_mut(&mut self, id: WindowId) -> Option<&mut Floating> {
        let spaces = self.monitors.iter_mut().flat_map(|m| &mut m.workspaces);
        let all = spaces.chain(&mut self.parked).flat_map(|w| &mut w.floating);
        all.into_iter().find(|f| f.id == id)
    }

    pub fn floating(&self, id: WindowId) -> Option<&Floating> {
        let spaces = self.monitors.iter().flat_map(|m| &m.workspaces);
        let all = spaces.chain(&self.parked).flat_map(|w| &w.floating);
        all.into_iter().find(|f| f.id == id)
    }

    /// Where a window on screen is right now, floating or not.
    pub fn rect_of(&self, id: WindowId) -> Option<Rect> {
        let mut placements = Vec::new();
        for monitor in &self.monitors {
            for workspace in &monitor.workspaces {
                Self::place(monitor, workspace, &mut placements);
            }
        }
        placements.iter().find(|p| p.id == id).map(|p| p.rect)
    }

    /// Between the layout and floating, wherever it is, in place. A window whose
    /// size isn't known yet, because it asked before mapping, goes in the middle.
    pub fn set_floating(&mut self, id: WindowId, on: bool) -> bool {
        if on == self.floating(id).is_some() {
            return false;
        }
        let rect = self.rect_of(id);
        for m in 0..self.monitors.len() {
            let usable = self.monitors[m].usable;
            let Some(workspace) = self.monitors[m]
                .workspaces
                .iter_mut()
                .find(|w| w.contains(id))
            else {
                continue;
            };
            workspace.take(id);
            let floating = on.then(|| Floating {
                id,
                rect: rect.unwrap_or(centered(usable, usable.w / 2.0, usable.h / 2.0)),
                sized: rect.is_some(),
                interactive: false,
            });
            workspace.put(id, floating);
            self.normalize(m);
            return true;
        }
        false
    }

    pub fn toggle_floating(&mut self) -> bool {
        let Some(id) = self.focused_window() else {
            return false;
        };
        self.set_floating(id, self.floating(id).is_none())
    }

    /// Between the floating windows and the layout.
    pub fn switch_floating_focus(&mut self) -> bool {
        let Some(workspace) = self.active_workspace_mut() else {
            return false;
        };
        let on = !workspace.focus_floating && !workspace.floating.is_empty();
        let changed = on != workspace.focus_floating;
        workspace.focus_floating = on;
        changed
    }

    pub fn set_floating_rect(&mut self, id: WindowId, rect: Rect) {
        if let Some(floating) = self.floating_mut(id) {
            floating.rect = rect;
        }
    }

    /// The size the client chose, the first time it says: in the middle of its
    /// monitor, or of the window it belongs to.
    pub fn fit_floating(&mut self, id: WindowId, width: f64, height: f64, over: Option<WindowId>) {
        let area = over.and_then(|parent| self.rect_of(parent)).or_else(|| {
            self.monitors.iter().find_map(|m| {
                m.workspaces
                    .iter()
                    .any(|w| w.contains(id))
                    .then_some(m.usable)
            })
        });
        if let (Some(area), Some(floating)) = (area, self.floating_mut(id))
            && !floating.sized
        {
            floating.rect = centered(area, width, height);
            floating.sized = true;
        }
    }

    pub fn set_interactive(&mut self, id: WindowId, on: bool) {
        if let Some(floating) = self.floating_mut(id) {
            floating.interactive = on;
        }
    }

    /// A drag ended with `id` over another monitor: it lives there now.
    pub fn rehome_floating(&mut self, id: WindowId) {
        let Some((cx, cy)) = self.floating(id).map(|f| f.rect.center()) else {
            return;
        };
        let target = self.monitors.iter().position(|m| m.rect.contains(cx, cy));
        let from = self
            .monitors
            .iter()
            .position(|m| m.workspaces.iter().any(|w| w.contains(id)));
        let (Some(target), Some(from)) = (target, from) else {
            return;
        };
        if target == from {
            return;
        }
        let floating = self.monitors[from]
            .workspaces
            .iter_mut()
            .find(|w| w.contains(id))
            .and_then(|w| w.take(id).flatten());
        self.normalize(from);
        self.monitors[target]
            .active_workspace_mut()
            .put(id, floating);
        self.focused = target;
        self.normalize(target);
    }

    /// Focus the monitor containing a point, as the pointer crosses over.
    pub fn focus_monitor_at(&mut self, x: f64, y: f64) {
        if let Some(index) = self.monitors.iter().position(|m| m.rect.contains(x, y)) {
            self.focused = index;
        }
    }

    /// Where a workspace's windows should be, fullscreen included.
    pub fn place(monitor: &Monitor, workspace: &Workspace, out: &mut Vec<Placement>) {
        let start = out.len();
        workspace.layout.place(out);
        out.extend(workspace.floating.iter().map(|f| Placement {
            id: f.id,
            rect: f.rect,
            fullscreen: false,
            floating: true,
            sized: f.sized,
            interactive: f.interactive,
        }));
        let Some(fullscreen) = workspace.fullscreen else {
            return;
        };
        if let Some(p) = out[start..].iter_mut().find(|p| p.id == fullscreen) {
            p.rect = monitor.rect;
            p.fullscreen = true;
        }
    }
}
