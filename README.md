# Pastel

English | [简体中文](README.zh-CN.md)

A lightweight, high-performance scrolling-tiling Wayland compositor (niri-style) written in [Mica](https://github.com/antinomie1/mica) on [luminaria](https://github.com/antinomie1/luminaria).

## Highlights

- **Scrolling tiling**: Infinite horizontal strip of columns per workspace, vertical stacking within columns, preset column widths, maximize/center column, dynamic (niri-style) or fixed (dwm-tag-style) workspaces, floating windows, and multi-monitor support.
- **Analytical animation system**: Refresh-rate-independent exact ODE spring solver and easing curves with momentum-preserving in-flight retargeting for window movement, camera scrolling, workspace switching, window open/close, focus-ring color transitions, and overlay transitions.
- **Visual effects**: Square corners by default (`corner-radius = 0.0`, configurable), GPU SDF hollow borders, soft shadows, per-window transparency, and GPU Dual-Kawase frosted-glass blur (both zero-steady-state-cost X-Ray mode and live backdrop blur).
- **Config-consistent hotkey overlay**: Clean niri-style keybinding overlay shown at startup (configurable) and toggled via `Mod+Shift+/`, rendered with smooth system TrueType/OpenType fonts (`[overlay].font = "sans-serif"`, loaded dynamically via FreeType2/Fontconfig) and automatically styled to match `[window]` borders, corner radius, accent color, shadow, live blur, and animations.
- **Pluggable & modular**: Monomorphized structural traits (`Curve`, `LayoutPolicy`, `luminaria.plug.Output`, `luminaria.plug.Renderer`) with zero dynamic dispatch overhead.
- **Edge-device friendly**: Minimal binary footprint, low RSS, zero extra C font/UI libraries, lazy GPU blur allocation, and **zero wakeups when idle**.
- **TOML configuration with live reload**: Strict schema validation (`pastel --check`), an empty file yields a full working desktop, and saving/reloading applies changes live.
- **JSON-lines IPC**: `pastel msg <action>`, `pastel msg --state`, and `pastel msg --subscribe` over a Unix socket for status bars, launchers, and scripts.

## Building

Requires the `mica` compiler, `sheaf` package manager, Vulkan 1.3 headers/runtime, and `luminaria`'s system libraries (`libdrm`, `libinput`, `libseat`, `libudev`, `xkbcommon`).

```sh
sheaf build --release -p pastel
```

Run tests and performance benchmarks:

```sh
sheaf test
sheaf test --release -p pastel
```

## Running

```sh
# Nested inside an existing Wayland session (Mod = Alt)
./target/release/pastel

# Bare metal DRM/KMS from a free VT (Mod = Super)
./target/release/pastel --backend drm

# Validate configuration without starting
./target/release/pastel --check

# Print every bindable action
./target/release/pastel --list-actions
```

### Default Keybindings

Default terminal is `foot` and default launcher is `fuzzel` (override in `[binds]`).

| Key | Action |
|---|---|
| `Mod+Shift+/` | Toggle hotkey overlay |
| `Mod+Return` / `Mod+D` | Spawn terminal / application launcher |
| `Mod+Q` | Close focused window |
| `Mod+H` / `Mod+L` | Focus column left / right |
| `Mod+J` / `Mod+K` | Focus window down / up in column |
| `Mod+Shift+H` / `Mod+Shift+L` | Move column left / right |
| `Mod+Shift+J` / `Mod+Shift+K` | Move window down / up |
| `Mod+[` / `Mod+]` | Consume window into column / expel window out |
| `Mod+R` / `Mod+F` / `Mod+C` | Cycle preset width / maximize column / center column |
| `Mod+Shift+F` | Toggle fullscreen |
| `Mod+V` / `Mod+Shift+V` | Toggle floating / switch focus between tiled and floating |
| `Mod+U` / `Mod+I` | Focus previous / next workspace |
| `Mod+1` .. `Mod+9` | Focus workspace 1..9 |
| `Mod+Shift+1` .. `Mod+Shift+9` | Move window to workspace 1..9 |
| `Mod+Shift+R` | Reload configuration live |
| `Mod+Shift+E` | Quit Pastel |
| `Mod+Scroll` | Scroll columns horizontally |
| `Mod+Left Drag` / `Mod+Right Drag` | Move / resize floating window |
| 3-finger touchpad swipe | Horizontal column pan / vertical workspace switch |

## Configuration

Configuration lives at `~/.config/pastel/config.toml` (or pass `--config <path>`). An empty file produces a complete default desktop. See [config.example.toml](config.example.toml) for every option and default.

```toml
background = "#1e2028"

[layout]
gap = 12.0
center-focus = "on-overflow"

[window]
corner-radius = 0.0        # Square corners by default
border-width = 2.0
active-color = "#a6c8ff"
inactive-color = "#4a4f5c"
blur = true
blur-xray = true

[animation]
slowdown = 1.0

[binds]
"Mod+B" = "spawn firefox"
"Mod+D" = "none"           # Unbind a default shortcut

[[window-rule]]
app-id = "foot"
opacity = 0.88
blur = true
```

## IPC

```sh
pastel msg focus-column-right
pastel msg focus-workspace 2
pastel msg reload-config
pastel msg --state
pastel msg --subscribe
```

## Architecture & Documentation

| Package | Responsibility | Extension Points |
|---|---|---|
| `pastel_core` | Geometry, colors, analytical spring/ease animations | `Curve` |
| `pastel_config` | Strict TOML config, actions, keybinds, window rules | `Action` |
| `pastel_layout` | Scrolling-tiling strip, workspaces, floating, multi-monitor | `LayoutPolicy` |
| `pastel_render` | Vulkan renderer (Dual-Kawase blur, SDF borders), hotkey overlay | `luminaria.plug.Renderer` |
| `pastel_ipc` | Unix JSON-lines IPC server & CLI client | `Request` / `Event` |
| `pastel` | Compositor loop, input/gestures, window rules, reload, spawn | `Pastel[O, R]` |

See [docs/README.md](docs/README.md) for architecture and design documentation, and [AGENTS.md](AGENTS.md) for contributor guidelines.

## License

BSD 2-Clause License. See [LICENSE](LICENSE).
