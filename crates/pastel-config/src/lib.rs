//! The config file, what keys do, and the defaults for both.
//!
//! An empty config is a complete desktop: every field has a default and `[binds]`
//! merges over the default bindings.

pub mod action;
mod defaults;
pub mod keys;
mod schema;

use std::path::{Path, PathBuf};

pub use action::{Action, Section};
pub use keys::{Binds, Combo, OverlayRow};
pub use schema::*;

impl Config {
    /// Parse a config document. Unknown keys are errors: a typo that is silently
    /// ignored is worse than one that is reported.
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| e.to_string())?;
        raw.resolve()
    }

    /// `$XDG_CONFIG_HOME/pastel/config.toml`, or under `~/.config`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?;
        Some(base.join("pastel/config.toml"))
    }

    /// A missing file is the defaults; an unreadable or invalid one is an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::parse(""),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}
