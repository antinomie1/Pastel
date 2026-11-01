//! What a key can do. Written in the config as kebab-case strings, with arguments
//! after a space: `"focus-column-left"`, `"spawn foot -e htop"`, `"focus-workspace 3"`.

use std::str::FromStr;

use pastel_layout::{Direction, LayoutOp, WorkspaceTarget};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// A key that does nothing; binding to this removes a default.
    None,
    Quit,
    Spawn(Vec<String>),
    CloseWindow,
    ToggleFullscreen,
    ToggleOverlay,
    ReloadConfig,
    Layout(LayoutOp),
    FocusWorkspace(WorkspaceTarget),
    MoveToWorkspace(WorkspaceTarget),
    FocusMonitor(Direction),
    MoveToMonitor(Direction),
}

/// The heading an action is listed under in the hotkey overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Windows,
    Columns,
    Workspaces,
    Session,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Self::Windows,
        Self::Columns,
        Self::Workspaces,
        Self::Session,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Columns => "Columns",
            Self::Workspaces => "Workspaces",
            Self::Session => "Session",
        }
    }
}

const WIDTH_STEP: f64 = 0.1;

/// The actions that take no argument, in one table so names can't drift between
/// parsing and the error message listing them.
const SIMPLE: &[(&str, Action)] = {
    use Action::*;
    use Direction::{Down, Left, Right, Up};
    use LayoutOp as Op;
    &[
        ("none", None),
        ("quit", Quit),
        ("close-window", CloseWindow),
        ("toggle-fullscreen", ToggleFullscreen),
        ("toggle-overlay", ToggleOverlay),
        ("reload-config", ReloadConfig),
        ("focus-column-left", Layout(Op::Focus(Left))),
        ("focus-column-right", Layout(Op::Focus(Right))),
        ("focus-window-up", Layout(Op::Focus(Up))),
        ("focus-window-down", Layout(Op::Focus(Down))),
        ("move-column-left", Layout(Op::Move(Left))),
        ("move-column-right", Layout(Op::Move(Right))),
        ("move-window-up", Layout(Op::Move(Up))),
        ("move-window-down", Layout(Op::Move(Down))),
        ("focus-column-first", Layout(Op::FocusFirst)),
        ("focus-column-last", Layout(Op::FocusLast)),
        ("move-column-first", Layout(Op::MoveFirst)),
        ("move-column-last", Layout(Op::MoveLast)),
        ("consume-or-expel-left", Layout(Op::ConsumeOrExpel(Left))),
        ("consume-or-expel-right", Layout(Op::ConsumeOrExpel(Right))),
        ("cycle-column-width", Layout(Op::CycleWidth)),
        ("grow-column", Layout(Op::AdjustWidth(WIDTH_STEP))),
        ("shrink-column", Layout(Op::AdjustWidth(-WIDTH_STEP))),
        ("maximize-column", Layout(Op::ToggleMaximize)),
        ("center-column", Layout(Op::Center)),
        ("focus-workspace-up", FocusWorkspace(WorkspaceTarget::Up)),
        (
            "focus-workspace-down",
            FocusWorkspace(WorkspaceTarget::Down),
        ),
        ("move-to-workspace-up", MoveToWorkspace(WorkspaceTarget::Up)),
        (
            "move-to-workspace-down",
            MoveToWorkspace(WorkspaceTarget::Down),
        ),
        ("focus-monitor-left", FocusMonitor(Left)),
        ("focus-monitor-right", FocusMonitor(Right)),
        ("focus-monitor-up", FocusMonitor(Up)),
        ("focus-monitor-down", FocusMonitor(Down)),
        ("move-to-monitor-left", MoveToMonitor(Left)),
        ("move-to-monitor-right", MoveToMonitor(Right)),
        ("move-to-monitor-up", MoveToMonitor(Up)),
        ("move-to-monitor-down", MoveToMonitor(Down)),
    ]
};

/// Workspaces are numbered from 1 in the config, from 0 everywhere else.
fn workspace_index(argument: &str) -> Result<WorkspaceTarget, String> {
    match argument.parse::<usize>() {
        Ok(n) if n >= 1 => Ok(WorkspaceTarget::Index(n - 1)),
        _ => Err(format!(
            "workspace must be a number from 1, got `{argument}`"
        )),
    }
}

impl FromStr for Action {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (name, argument) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        let argument = argument.trim();
        match name {
            "spawn" if argument.is_empty() => Err("spawn needs a command".into()),
            "spawn" => Ok(Action::Spawn(
                argument.split_whitespace().map(String::from).collect(),
            )),
            "focus-workspace" => workspace_index(argument).map(Action::FocusWorkspace),
            "move-to-workspace" => workspace_index(argument).map(Action::MoveToWorkspace),
            _ => {
                let found = SIMPLE
                    .iter()
                    .find(|(n, _)| *n == name && argument.is_empty());
                found
                    .map(|(_, action)| action.clone())
                    .ok_or_else(|| format!("unknown action `{text}`; see `pastel --list-actions`"))
            }
        }
    }
}

impl Action {
    /// Every argument-less action name, plus the ones that take one.
    pub fn names() -> impl Iterator<Item = &'static str> {
        let with_argument = [
            "spawn <command>",
            "focus-workspace <n>",
            "move-to-workspace <n>",
        ];
        SIMPLE.iter().map(|(name, _)| *name).chain(with_argument)
    }

    /// How the hotkey overlay describes this, and under what heading; `None` keeps
    /// it out of the overlay. Curated: the overlay is for the bindings people need
    /// reminding of, not a dump of the config.
    pub fn title(&self) -> Option<(Section, String)> {
        use Action as A;
        use Direction::{Left, Right};
        use LayoutOp as Op;
        use Section::*;
        let (section, text) = match self {
            A::Spawn(command) => {
                let name = command.first()?.rsplit('/').next()?;
                (Session, format!("Run {name}"))
            }
            A::CloseWindow => (Windows, "Close window".into()),
            A::ToggleFullscreen => (Windows, "Fullscreen".into()),
            A::Layout(Op::Focus(Left | Right)) => (Windows, "Focus column".into()),
            A::Layout(Op::Move(Left | Right)) => (Windows, "Move column".into()),
            A::Layout(Op::Focus(_)) => (Windows, "Focus in column".into()),
            A::Layout(Op::Move(_)) => (Windows, "Move in column".into()),
            A::Layout(Op::ConsumeOrExpel(_)) => (Columns, "Join or leave column".into()),
            A::Layout(Op::CycleWidth) => (Columns, "Cycle width".into()),
            A::Layout(Op::ToggleMaximize) => (Columns, "Maximize column".into()),
            A::Layout(Op::Center) => (Columns, "Center column".into()),
            A::Layout(Op::AdjustWidth(_)) => (Columns, "Resize column".into()),
            A::FocusWorkspace(WorkspaceTarget::Index(_)) => (Workspaces, "Go to workspace".into()),
            A::MoveToWorkspace(WorkspaceTarget::Index(_)) => {
                (Workspaces, "Send to workspace".into())
            }
            A::FocusWorkspace(_) => (Workspaces, "Next / previous workspace".into()),
            A::MoveToWorkspace(_) => (Workspaces, "Send up / down".into()),
            A::FocusMonitor(_) => (Workspaces, "Focus monitor".into()),
            A::MoveToMonitor(_) => (Workspaces, "Send to monitor".into()),
            A::ToggleOverlay => (Session, "Show this help".into()),
            A::ReloadConfig => (Session, "Reload config".into()),
            A::Quit => (Session, "Quit Pastel".into()),
            _ => return Option::None,
        };
        Some((section, text))
    }
}
