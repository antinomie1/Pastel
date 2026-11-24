# Configuration & IPC (`pastel_config` & `pastel_ipc`)

## 1. TOML Configuration (`pastel_config`)

Configuration is loaded from `--config <path>`, or `$XDG_CONFIG_HOME/pastel/config.toml` (falling back to `~/.config/pastel/config.toml`).

- **Zero-config default**: Missing or empty config files produce a full working desktop with sensible defaults.
- **Strict validation**: Every table and key is checked against the schema via `std.toml`; unknown keys or out-of-range values return actionable error messages (`pastel --check` validates without launching).
- **Composable keybindings & lock security**: `[binds]` merges on top of defaults; setting a key to `"none"` removes a default binding. Key specs accept `Mod`, `Super`, `Alt`, `Ctrl`, `Shift`, full XKB keysym names (resolved lazily via `libxkbcommon.so.0` for media/brightness/F13+ keys), mouse buttons (`MouseLeft`/`MouseRight`/`MouseMiddle`/`MouseBack`/`MouseForward`), and scroll wheel triggers (`WheelUp`/`WheelDown`/`WheelLeft`/`WheelRight`). Inline table syntax `{ action = "...", allow-when-locked = true }` opts specific bindings in while the session is locked (`ext-session-lock-v1`).
- **Split input & cursor**: `[input.touchpad]` and `[input.mouse]` configure pointer speed, `accel-profile` (`default` / `flat` / `adaptive`), `natural-scroll`, `left-handed`, `tap-to-click`, and `dwt` independently per device type. `[cursor]` configures cursor `size`, `hide-when-typing`, and zero-churn `hide-after-inactive-ms`.
- **Outputs (`[[output]]`)**: Configure `mode`, `scale`, `position`, `enabled`, `vrr` (`false` / `true` / `"on-demand"`), and HDR (`hdr`, `sdr-nits`, `max-nits`).
- **Session environment (`[env]`)**: Injects key/value environment variables into all spawned commands, `autostart`, and `dbus-update-activation-environment`.
- **Window rules (`[[window-rule]]`)**: Match windows by substring or `*` wildcard glob on `app-id` and/or `title`. Rules are evaluated in order (later rules override earlier ones), apply both at map time and on late-arriving metadata, and can set `opacity`, `blur`, `corner-radius`, `border-width`, `default-width`, `default-floating-size`, `open-focused`, `block-out-from-screencast`, `variable-refresh-rate`, `floating`, `fullscreen`, `maximized`, `workspace`, and `monitor`.
- **XWayland (`[xwayland]`)**: Optional out-of-process X11 support via `xwayland-satellite` (`enabled`, `path`, `display`, `ready-timeout-ms`), keeping `luminaria` pure Wayland while waiting briefly for X11 socket readiness and supporting live reload toggling.
- **Live reload**: Triggered automatically on file save via Linux `inotify` (watching the config directory or nearest existing ancestor with zero wakeups when idle), or manually via `Mod+Shift+R`, `pastel msg reload-config`, or zero-wakeup `signalfd` `SIGHUP`. Re-parses the config safely: invalid edits log an error and keep the previous working configuration intact; valid edits update visuals, keybinds, keymap, keyboard repeat rate/delay, split touchpad/mouse settings, `[env]`, XWayland lifecycle, HDR/VRR, and output modes immediately without restarting.

## 2. Unix JSON-Lines IPC (`pastel_ipc`)

Pastel opens a non-blocking Unix domain socket at:

```
$XDG_RUNTIME_DIR/pastel-<WAYLAND_DISPLAY>.sock
```

and exports `PASTEL_SOCKET` to child processes spawned by Pastel.

- **Single-loop integration**: Both the listener fd and connected IPC client fds are registered with `Display.watch(fd, token)` — zero extra threads, zero polling.
- **Protocol**: Newline-delimited JSON objects:
  - Action request: `{"cmd":"action","action":"focus-workspace 2"}` -> `{"ok":true}`
  - State query (`pastel msg --state`): `{"cmd":"state"}` -> JSON snapshot of outputs, workspaces, focused window, and window list.
  - Event subscription (`pastel msg --subscribe`): `{"cmd":"subscribe"}` -> streams newline-delimited JSON events (`workspace-changed`, `focus-changed`, `window-opened`, `window-closed`, `window-changed`, `outputs-changed`, `overview-changed`, `locked`, `bell`, `layout-changed`, `config-reloaded`) whenever desktop state changes.
