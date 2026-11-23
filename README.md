# Pastel

English | [简体中文](README.zh-CN.md)

A lightweight, high-performance scrolling-tiling Wayland compositor (niri-style) written in [Mica](https://github.com/antinomie1/mica) on [luminaria](https://github.com/antinomie1/luminaria).

## Highlights

- **Scrolling tiling & Overview**: Infinite horizontal strip of columns per workspace, vertical stacking & lightweight GPU-pill tabbed columns (occluding inactive tabs), directional consume/expel, whole-column workspace/monitor moves, preset column widths, expand-to-available-width, single-column centering, dynamic (niri-style) or fixed (dwm-tag-style) workspaces, floating windows, multi-monitor support, and zero-VRAM animated **Overview** thumbnail mode (`Mod+O` / 4-finger swipe).
- **Analytical animation system**: Refresh-rate-independent exact ODE spring solver and easing curves with momentum-preserving in-flight retargeting for window movement, camera scrolling, workspace switching, window open/close, focus-ring & bell pulses, hotkey overlay, and Overview transitions.
- **Visual effects & HDR**: Square corners by default (`corner-radius = 0.0`, configurable), GPU SDF hollow borders, soft shadows, per-window transparency, GPU Dual-Kawase frosted-glass blur (zero-steady-state-cost X-Ray mode + live backdrop blur), **10-bit/FP16 HDR** (`wp-color-management-v1` + DRM `HDR_OUTPUT_METADATA` / `BT2020_RGB`), **on-demand VRR** (`vrr = "on-demand"`), **gamma LUT night light** (`wlr-gamma-control-unstable-v1`), and **screencast privacy redaction** (`block-out-from-screencast`).
- **Session security & input**: Lock-screen shortcut isolation (`ext-session-lock-v1`, with explicit `allow-when-locked = true` opt-in), DPMS power control (`power-off-monitors` / auto-wake on input), split `[input.touchpad]` / `[input.mouse]` with `accel-profile = "flat"` / `"adaptive"` and `dwt`, full XKB keysyms (media/brightness keys), configurable mouse & wheel bindings, `[cursor]` auto-hide, and virtual pointer support (`wlr-virtual-pointer-unstable-v1`).
- **Config-consistent hotkey overlay**: Clean niri-style keybinding overlay shown at startup (configurable) and toggled via `Mod+Shift+/`, rendered with smooth system TrueType/OpenType fonts (`[overlay].font = "sans-serif"`, loaded dynamically via FreeType2/Fontconfig) and automatically styled to match `[window]` borders, corner radius, accent color, shadow, live blur, and animations.
- **Pluggable & modular**: Monomorphized structural traits (`Curve`, `LayoutPolicy`, `luminaria.plug.Output`, `luminaria.plug.Renderer`) with zero dynamic dispatch overhead.
- **Edge-device friendly**: Minimal binary footprint, low RSS, zero extra link-time C dependencies beyond `luminaria` + Vulkan 1.3, lazy blur/HDR pipeline allocation, and **zero wakeups when idle**.
- **TOML configuration with live reload**: Strict schema validation (`pastel --check`), `[env]` session environment injection, an empty file yields a full working desktop, and saving/reloading applies changes live.
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
| `Mod+O` | Toggle Overview thumbnail mode |
| `Mod+Return` / `Mod+D` | Spawn terminal / application launcher |
| `Mod+Q` | Close focused window |
| `Mod+H` / `Mod+L` | Focus column left / right |
| `Mod+J` / `Mod+K` | Focus window down / up in column (or switch tabs) |
| `Mod+Shift+H` / `Mod+Shift+L` | Move column left / right |
| `Mod+Shift+J` / `Mod+Shift+K` | Move window down / up |
| `Mod+Comma` / `Mod+Period` | Consume or expel window left / right (`Mod+[` / `Mod+]` also supported) |
| `Mod+W` | Toggle tabbed column display |
| `Mod+R` / `Mod+F` / `Mod+C` | Cycle preset width / maximize column / center column |
| `Mod+Ctrl+F` / `Mod+Ctrl+R` | Expand column to available width / reset window height |
| `Mod+Minus` / `Mod+Equal` | Shrink / grow column width (`Mod+Shift+-/=` adjusts row height) |
| `Mod+Shift+F` | Toggle fullscreen |
| `Mod+V` / `Mod+Shift+V` | Toggle floating / switch focus between tiled and floating |
| `Mod+U` / `Mod+I` / `Mod+Tab` | Focus previous / next / previously visited workspace |
| `Mod+1` .. `Mod+9` | Focus workspace 1..9 (`Mod+Shift+1..9` moves window) |
| `Mod+Ctrl+H/J/K/L` | Focus monitor left / down / up / right (`+Shift` moves window) |
| `Mod+Shift+P` | Power off monitors (DPMS; any input wakes) |
| `Mod+Shift+R` | Reload configuration live |
| `Mod+Shift+E` | Quit Pastel |
| `Mod+WheelUp` / `Mod+WheelDown` | Focus column left / right (`Mod+Shift+Wheel*` switches workspaces) |
| `Mod+Left Drag` / `Mod+Right Drag` | Move floating or pan tiled strip / resize floating window |
| 3-finger / 4-finger touchpad swipe | Column pan & workspace switch / toggle Overview |

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
