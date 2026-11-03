# Pastel

English | [简体中文](README.zh-CN.md)

A scrolling-tiling Wayland compositor (niri-style) built on [luminaria](https://github.com/antinomie1/luminaria), written in Rust 2024.

- **Native Vulkan 1.3 backend**: Built on luminaria's native Vulkan 1.3 backend
- **Scrolling tiling**: Columns, stacking within columns, preset widths, maximize, center, dynamic workspaces, and multi-monitor support
- **Animations**: Spring and ease curves with in-flight retargeting preserving momentum; covers window open/close, movement, workspace switching, focus color transitions, and overlay
- **Visual effects**: Optional rounded corners (disabled by default), hollow borders, shadows, frosted glass blur, and window rules
- **Floating windows**: Dialogs float automatically, toggled manually or assigned via window rules; drag by titlebar, `Mod+Left Click` to move, `Mod+Right Click` to resize
- **Desktop protocols**: Screen lock (swaylock), screenshot/screen recording (grim, wf-recorder), idle notification (swayidle), status bars/panels, workspaces for pagers (ext-workspace-v1), taskbars and window lists (wlr-foreign-toplevel, ext-foreign-toplevel-list), output management (wlr-randr, kanshi), clipboard managers (wlr-data-control), window icons (xdg-toplevel-icon), color-management negotiation (sRGB), and xdg-activation
- **Input methods**: fcitx5 and IBus through text-input-v3 and input-method-v2, with the candidate window under the text cursor; virtual keyboards and pointers (wayvnc, KDE Connect)
- **X11 applications**: Via [xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite), starts on demand when the first X client connects; can be disabled with `[xwayland] enabled`
- **Displays**: Fractional scaling (`scale = 1.5`), rotation, multi-output; clients render at their true scale
- **Input**: Touchscreens (rotates with display, `touch-output` binds to a specific display); touchpad 3-finger horizontal swipe to pan, vertical swipe to switch workspaces, passes other gestures through to applications
- **Hotkey overlay**: Shown on startup, reopened with `Mod+Shift+/`, dismissed by any key
- **Live config reload**: Takes effect immediately upon saving; keyboard layouts, background color, and monitors (`[[output]]`) require no restarts
- **IPC**: `pastel msg focus-workspace 2`, `pastel msg --state`, `pastel msg --subscribe`; JSON lines over a Unix socket (`pastel-ipc` crate) for bars and shells such as [Palettes](https://github.com/antinomie1/Palettes)
- **Zero idle overhead**: No timers and no frame renders when no animations are running

## Building

Requires Rust ≥ 1.88, xmake, clang ≥ 22, and wayland, wayland-protocols, vulkan-headers, libinput, seatd, libdrm, gbm, fcft, glslang.

luminaria is fetched from GitHub by Cargo, pinned to a specific revision in the root `Cargo.toml`:

```sh
cargo build --release -j4      # First build compiles C++; please limit parallelism
```

## Running

```sh
cargo run --release            # Run nested in the current Wayland session (Mod = Alt)
LUMINARIA_BACKEND=drm cargo run --release   # Bare metal on a free VT (Mod = Super)
```

Default terminal is `foot`, and default application launcher is `fuzzel`.

| Key | Action |
|---|---|
| `Mod+Shift+/` | Hotkey overlay |
| `Mod+Return` / `Mod+D` | Terminal / Launcher |
| `Mod+Q` | Close window |
| `Mod+H/L`, `Mod+J/K` | Focus column, focus within column |
| `Mod+Shift+H/J/K/L` | Move window |
| `Mod+[` / `Mod+]` | Join into adjacent column / expel to new column |
| `Mod+R`, `Mod+F`, `Mod+C` | Cycle width, maximize, center |
| `Mod+Shift+F` | Fullscreen |
| `Mod+V` / `Mod+Shift+V` | Toggle floating / switch focus between floating and tiled |
| `Mod+U/I`, `Mod+1..9` | Switch workspace |
| `Mod+Shift+R` / `Mod+Shift+E` | Reload config / quit |
| `Mod+Scroll`, `Mod+Left Click Drag` | Switch column, pan view (moves window if floating) |
| `Mod+Right Click Drag` | Resize window |

## Configuration

`~/.config/pastel/config.toml`. An empty file provides a fully working desktop, so only specify options you want to change; see [config.example.toml](config.example.toml) for all available options.

```toml
[layout]
gap = 8
center-focus = "on-overflow"

[animation]
slowdown = 1.5

[binds]
"Mod+B" = "spawn firefox"
"Mod+D" = "none"             # Remove default binding

[[window-rule]]              # Blur is visible through translucent windows
app-id = "foot"
opacity = 0.85
```

Run `pastel --check` to validate your configuration, or `pastel --list-actions` to list all available actions.

## Extending

Code is organized into layered crates, with dependencies pointing strictly downward:

| Crate | Responsibility | Extension Points |
|---|---|---|
| `pastel-core` | Geometry, animation | `Curve` |
| `pastel-layout` | Layouts, desktop model | `Layout` |
| `pastel-config` | Configuration, actions, keybindings | `Action` |
| `pastel-render` | Animation state, effects, overlays | `Effect`, `Overlay` |
| `pastel-ipc` | IPC protocol, socket path, client | `Request` |
| `pastel` | Event loop, clients, input, IPC | — |

Layouts only answer "where should windows be" and know nothing about time, so any new layout automatically gets all animations for free. See [AGENTS.md](AGENTS.md) for development guidelines, and [TODO.md](TODO.md) for the roadmap.

## License

BSD 2-Clause, see [LICENSE](LICENSE).
