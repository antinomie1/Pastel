//! The bindings an empty config gets, close to niri's so muscle memory carries over.

use crate::action::Action;
use crate::keys::{Binds, Combo};

const FIXED: &[(&str, &str)] = &[
    ("Mod+Return", "spawn foot"),
    ("Mod+D", "spawn fuzzel"),
    ("Mod+Q", "close-window"),
    ("Mod+F", "maximize-column"),
    ("Mod+Shift+F", "toggle-fullscreen"),
    ("Mod+C", "center-column"),
    ("Mod+R", "cycle-column-width"),
    ("Mod+minus", "shrink-column"),
    ("Mod+equal", "grow-column"),
    ("Mod+H", "focus-column-left"),
    ("Mod+L", "focus-column-right"),
    ("Mod+J", "focus-window-down"),
    ("Mod+K", "focus-window-up"),
    ("Mod+Left", "focus-column-left"),
    ("Mod+Right", "focus-column-right"),
    ("Mod+Down", "focus-window-down"),
    ("Mod+Up", "focus-window-up"),
    ("Mod+Shift+H", "move-column-left"),
    ("Mod+Shift+L", "move-column-right"),
    ("Mod+Shift+J", "move-window-down"),
    ("Mod+Shift+K", "move-window-up"),
    ("Mod+Shift+Left", "move-column-left"),
    ("Mod+Shift+Right", "move-column-right"),
    ("Mod+Shift+Down", "move-window-down"),
    ("Mod+Shift+Up", "move-window-up"),
    ("Mod+Home", "focus-column-first"),
    ("Mod+End", "focus-column-last"),
    ("Mod+Shift+Home", "move-column-first"),
    ("Mod+Shift+End", "move-column-last"),
    ("Mod+bracketleft", "consume-or-expel-left"),
    ("Mod+bracketright", "consume-or-expel-right"),
    ("Mod+U", "focus-workspace-down"),
    ("Mod+I", "focus-workspace-up"),
    ("Mod+Shift+U", "move-to-workspace-down"),
    ("Mod+Shift+I", "move-to-workspace-up"),
    ("Mod+Ctrl+H", "focus-monitor-left"),
    ("Mod+Ctrl+L", "focus-monitor-right"),
    ("Mod+Ctrl+Shift+H", "move-to-monitor-left"),
    ("Mod+Ctrl+Shift+L", "move-to-monitor-right"),
    ("Mod+Shift+Slash", "toggle-overlay"),
    ("Mod+Shift+R", "reload-config"),
    ("Mod+Shift+E", "quit"),
];

pub fn binds() -> Binds {
    let mut binds = Binds::default();
    let mut bind = |combo: &str, action: &str| {
        let combo: Combo = combo.parse().expect("default combos parse");
        binds.set(combo, action.parse::<Action>().expect("default actions parse"));
    };
    for (combo, action) in FIXED {
        bind(combo, action);
    }
    for n in 1..=9 {
        bind(&format!("Mod+{n}"), &format!("focus-workspace {n}"));
        bind(&format!("Mod+Shift+{n}"), &format!("move-to-workspace {n}"));
    }
    binds
}
