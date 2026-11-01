//! Key combos such as `Mod+Shift+H`. The key stays a name here: turning it into a
//! keysym is xkb's job, which belongs to the compositor. The modifier bits match
//! xkb's, so they compare directly against what the seat reports.

use std::collections::HashMap;
use std::str::FromStr;

use crate::action::Action;

pub const SHIFT: u32 = 1 << 0;
pub const CTRL: u32 = 1 << 2;
pub const ALT: u32 = 1 << 3;
pub const LOGO: u32 = 1 << 6;
/// Stands for whichever modifier `input.mod` resolves to. Never reaches the seat.
const MOD: u32 = 1 << 31;

/// A key and the modifiers held with it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Combo {
    pub mods: u32,
    /// An xkb keysym name, in lower case for single letters.
    pub key: String,
}

impl FromStr for Combo {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let mut mods = 0;
        let mut parts = text.split('+').map(str::trim).peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                if part.is_empty() {
                    return Err(format!("`{text}` has no key"));
                }
                return Ok(Combo { mods, key: canonical_key(part) });
            }
            mods |= match part.to_ascii_lowercase().as_str() {
                "mod" => MOD,
                "shift" => SHIFT,
                "ctrl" | "control" => CTRL,
                "alt" => ALT,
                "super" | "logo" | "win" => LOGO,
                other => return Err(format!("unknown modifier `{other}` in `{text}`")),
            };
        }
        Err(format!("`{text}` has no key"))
    }
}

/// xkb names are case-sensitive and `Return` is not `return`, but people write `Q`
/// to mean the Q key: only single letters fold.
fn canonical_key(key: &str) -> String {
    if key.chars().count() == 1 { key.to_lowercase() } else { key.to_string() }
}

impl Combo {
    /// With `Mod` replaced by the real modifier.
    pub fn resolved(&self, mod_key: u32) -> Combo {
        let mut mods = self.mods;
        if mods & MOD != 0 {
            mods = (mods & !MOD) | mod_key;
        }
        Combo { mods, key: self.key.clone() }
    }

    /// As shown in the overlay: `Mod + Shift + H`.
    pub fn label(&self, mod_key: u32) -> String {
        let mut parts = Vec::new();
        if self.mods & MOD != 0 {
            parts.push("Mod");
        }
        for (bit, name) in [(CTRL, "Ctrl"), (ALT, "Alt"), (LOGO, "Super"), (SHIFT, "Shift")] {
            if self.mods & bit != 0 && !(self.mods & MOD != 0 && bit == mod_key) {
                parts.push(name);
            }
        }
        let key = self.key.to_uppercase();
        let key = match self.key.as_str() {
            "Return" | "Tab" | "Escape" | "Left" | "Right" | "Up" | "Down" | "Home" | "End" => {
                &self.key
            }
            "bracketleft" => "[",
            "bracketright" => "]",
            "minus" => "-",
            "equal" => "=",
            "slash" => "/",
            "comma" => ",",
            "period" => ".",
            "space" => "Space",
            _ => &key,
        };
        parts.push(key);
        parts.join(" + ")
    }
}

/// `input.mod` and `input.nested-mod` values.
pub fn parse_modifier(text: &str) -> Result<u32, String> {
    match text.to_ascii_lowercase().as_str() {
        "super" | "logo" | "win" => Ok(LOGO),
        "alt" => Ok(ALT),
        "ctrl" | "control" => Ok(CTRL),
        "shift" => Ok(SHIFT),
        other => Err(format!("unknown modifier `{other}`")),
    }
}

/// All bindings, in config order. A later binding of the same combo replaces an
/// earlier one; binding to [`Action::None`] removes it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Binds {
    list: Vec<(Combo, Action)>,
}

impl Binds {
    pub fn set(&mut self, combo: Combo, action: Action) {
        self.list.retain(|(c, _)| *c != combo);
        if action != Action::None {
            self.list.push((combo, action));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Combo, Action)> {
        self.list.iter()
    }

    /// The lookup used on every key press, with `Mod` resolved.
    pub fn table(&self, mod_key: u32) -> HashMap<Combo, Action> {
        let resolved = self.list.iter().map(|(c, a)| (c.resolved(mod_key), a.clone()));
        resolved.collect()
    }
}
