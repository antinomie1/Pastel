# Configuration & IPC (`pastel_config` & `pastel_ipc`)

## 1. TOML Configuration (`pastel_config`)

Configuration is loaded from `--config <path>`, or `$XDG_CONFIG_HOME/pastel/config.toml` (falling back to `~/.config/pastel/config.toml`).

- **Zero-config default**: Missing or empty config files produce a full working desktop with sensible defaults.
- **Strict validation**: Every table and key is checked against the schema via `std.toml`; unknown keys or out-of-range values return actionable error messages (`pastel --check` validates without launching).
- **Composable keybindings**: `[binds]` merges on top of defaults; setting a key to `"none"` removes a default binding. Key specs accept `Mod`, `Super`, `Alt`, `Ctrl`, `Shift`, and XKB keysym names (e.g. `Mod+Return`, `Mod+Shift+/`, `Mod+1`). `Mod` resolves to `[input].mod` on DRM and `[input].nested-mod` when nested.
- **Window rules (`[[window-rule]]`)**: Match windows by substring or `*` wildcard glob on `app-id` and/or `title`. Rules are evaluated in order (later rules override earlier ones), apply both at map time and on late-arriving metadata, and can set `opacity`, `blur`, `corner-radius`, `border-width`, `default-width`, `floating`, `fullscreen`, `maximized`, `workspace`, and `monitor`.
- **XWayland (`[xwayland]`)**: Optional out-of-process X11 support via `xwayland-satellite` (`enabled`, `path`, `display`, `ready-timeout-ms`), keeping `luminaria` pure Wayland while waiting briefly for X11 socket readiness and supporting live reload toggling.
- **Live reload**: Triggered automatically on file save via Linux `inotify` (watching the config directory or nearest existing ancestor with zero wakeups when idle), or manually via `Mod+Shift+R`, `pastel msg reload-config`, or zero-wakeup `signalfd` `SIGHUP`. Re-parses the config safely: invalid edits log an error and keep the previous working configuration intact; valid edits update visuals, keybinds, keymap, keyboard repeat rate/delay, input device settings, XWayland lifecycle, and output modes immediately without restarting.

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
  - Event subscription (`pastel msg --subscribe`): `{"cmd":"subscribe"}` -> streams newline-delimited JSON events (`workspace-changed`, `focus-changed`, `window-opened`, `window-closed`, `config-reloaded`) whenever desktop state changes.
