//! Monitors, each with a vertical stack of workspaces, each with a [`Layout`].
//!
//! Workspaces are dynamic as in niri: there is always exactly one empty workspace
//! at the bottom, and an empty workspace disappears once it stops being looked at.

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

pub struct Workspace {
    /// Stable across reordering, unlike its index.
    pub id: u64,
    pub layout: Box<dyn Layout>,
    /// At most one, and always a window of this workspace.
    pub fullscreen: Option<WindowId>,
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
    /// unless it is the one being looked at.
    fn normalize(&mut self, new_workspace: &mut impl FnMut() -> Workspace) {
        let mut index = 0;
        while index + 1 < self.workspaces.len() {
            if index != self.active && self.workspaces[index].layout.is_empty() {
                self.workspaces.remove(index);
                if index < self.active {
                    self.active -= 1;
                    self.view_shift -= 1.0;
                }
            } else {
                index += 1;
            }
        }
        // All that can be left of a double empty: the active one above the spare.
        let n = self.workspaces.len();
        if n >= 2
            && self.active == n - 2
            && self.workspaces[n - 2].layout.is_empty()
            && self.workspaces[n - 1].layout.is_empty()
        {
            self.workspaces.pop();
        }
        if !self.workspaces.last().is_some_and(|w| w.layout.is_empty()) {
            self.workspaces.push(new_workspace());
        }
    }
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
        Some(Self { options, monitors: Vec::new(), focused: 0, next_workspace: 0, parked: Vec::new() })
    }

    /// Takes effect for workspaces created from now on.
    pub fn set_options(&mut self, options: LayoutOptions) -> bool {
        if create(&options).is_none() {
            return false;
        }
        self.options = options;
        true
    }

    fn new_workspace(options: &LayoutOptions, next: &mut u64, area: Rect) -> Workspace {
        let mut layout = create(options).expect("options were validated");
        layout.set_area(area);
        *next += 1;
        Workspace { id: *next, layout, fullscreen: None }
    }

    fn normalize(&mut self, index: usize) {
        let Self { options, monitors, next_workspace, .. } = self;
        let monitor = &mut monitors[index];
        let area = monitor.usable;
        monitor.normalize(&mut || Self::new_workspace(options, next_workspace, area));
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
        self.monitors.push(Monitor { id, rect, usable: rect, workspaces, active: 0, view_shift: 0.0 });
        self.normalize(self.monitors.len() - 1);
    }

    /// An output left. Its windows move to the first remaining monitor.
    pub fn remove_monitor(&mut self, id: u64) {
        let Some(index) = self.monitors.iter().position(|m| m.id == id) else { return };
        let monitor = self.monitors.remove(index);
        let occupied = monitor.workspaces.into_iter().filter(|w| !w.layout.is_empty());
        match self.monitors.first_mut() {
            Some(first) => {
                let at = first.workspaces.len() - 1; // above the empty one
                let area = first.usable;
                let mut adopted: Vec<Workspace> = occupied.collect();
                for workspace in &mut adopted {
                    workspace.layout.set_area(area);
                }
                first.workspaces.splice(at..at, adopted);
            }
            None => self.parked.extend(occupied),
        }
        self.focused = self.focused.min(self.monitors.len().saturating_sub(1));
    }

    /// The output moved or resized, or panels changed what is left of it.
    pub fn set_monitor_geometry(&mut self, id: u64, rect: Rect, usable: Rect) {
        let Some(monitor) = self.monitors.iter_mut().find(|m| m.id == id) else { return };
        monitor.rect = rect;
        monitor.usable = usable;
        for workspace in &mut monitor.workspaces {
            workspace.layout.set_area(usable);
        }
    }

    fn active_workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.monitors.get_mut(self.focused).map(Monitor::active_workspace_mut)
    }

    /// The window with the keyboard, as far as layouts are concerned.
    pub fn focused_window(&self) -> Option<WindowId> {
        let workspace = self.focused_monitor()?.active_workspace();
        workspace.fullscreen.or_else(|| workspace.layout.focused())
    }

    /// Put a window on the focused workspace and focus it. With no monitor yet it
    /// waits on a parked workspace.
    pub fn add_window(&mut self, id: WindowId, hint: WindowHint) {
        if let Some(workspace) = self.active_workspace_mut() {
            workspace.fullscreen = None;
            workspace.layout.add(id, hint);
            self.normalize(self.focused);
            return;
        }
        if self.parked.is_empty() {
            let workspace = Self::new_workspace(&self.options, &mut self.next_workspace, Rect::default());
            self.parked.push(workspace);
        }
        self.parked[0].layout.add(id, hint);
    }

    pub fn remove_window(&mut self, id: WindowId) -> bool {
        for workspace in &mut self.parked {
            if workspace.layout.remove(id) {
                return true;
            }
        }
        for m in 0..self.monitors.len() {
            for workspace in &mut self.monitors[m].workspaces {
                if workspace.layout.remove(id) {
                    if workspace.fullscreen == Some(id) {
                        workspace.fullscreen = None;
                    }
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
            let found = monitor.workspaces.iter().position(|w| w.layout.contains(id));
            let Some(w) = found else { continue };
            monitor.active = w;
            let workspace = monitor.active_workspace_mut();
            workspace.layout.focus(id);
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
        let Some(workspace) = self.active_workspace_mut() else { return false };
        let changed = workspace.layout.apply(op);
        if workspace.fullscreen.is_some() && workspace.fullscreen != workspace.layout.focused() {
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
        if let Some(workspace) = spaces.chain(&mut self.parked).find(|w| w.layout.contains(id)) {
            match (on, workspace.fullscreen) {
                (true, _) => workspace.fullscreen = Some(id),
                (false, Some(current)) if current == id => workspace.fullscreen = None,
                _ => {}
            }
        }
    }

    pub fn toggle_fullscreen(&mut self) -> bool {
        let Some(id) = self.focused_window() else { return false };
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
        let Some(monitor) = self.monitors.get_mut(self.focused) else { return false };
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
        let Some(id) = self.focused_window() else { return false };
        let monitor = &mut self.monitors[self.focused];
        let index = Self::resolve(monitor, target);
        if index == monitor.active {
            return false;
        }
        let workspace = monitor.active_workspace_mut();
        workspace.layout.remove(id);
        workspace.fullscreen = None;
        monitor.active = index;
        monitor.active_workspace_mut().layout.add(id, WindowHint::default());
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
        let Some(index) = self.neighbour(direction) else { return false };
        self.focused = index;
        true
    }

    /// Send the focused window to the neighbouring monitor and go with it.
    pub fn move_to_monitor(&mut self, direction: Direction) -> bool {
        let Some(index) = self.neighbour(direction) else { return false };
        let Some(id) = self.focused_window() else { return false };
        let from = self.focused;
        let workspace = self.monitors[from].active_workspace_mut();
        workspace.layout.remove(id);
        workspace.fullscreen = None;
        self.normalize(from);
        self.focused = index;
        self.monitors[index].active_workspace_mut().layout.add(id, WindowHint::default());
        self.normalize(index);
        true
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
        let Some(fullscreen) = workspace.fullscreen else { return };
        if let Some(p) = out[start..].iter_mut().find(|p| p.id == fullscreen) {
            p.rect = monitor.rect;
            p.fullscreen = true;
        }
    }
}
