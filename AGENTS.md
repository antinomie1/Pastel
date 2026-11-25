# AGENTS.md — Pastel Development Guide

## 1. Design Goals & Engineering Constraints

| Goal | Engineering Constraint |
|---|---|
| **Lightweight & Edge-Friendly** | Zero extra link-time C dependencies beyond `luminaria` (`sys/*` + Vulkan 1.3); FreeType2/Fontconfig loaded lazily via `extern table` only for hotkey overlay text; lazy blur pyramid allocation; zero wakeups when idle |
| **WDDM + DWM Separation** | Hardware/protocol mechanism (`luminaria`: KMS atomic commits, explicit sync, MPO overlays, direct scanout, GPU TDR recovery) strictly separated from visual composition (`pastel_render`: Dual-Kawase blur, SDF borders, overlays) and windowing policy (`pastel_layout` + `pastel`) |
| **Pluggable & Modular** | Structural traits without dynamic dispatch: `pastel_core.Curve`, `pastel_layout.LayoutPolicy`, `luminaria.plug.Output`, `luminaria.plug.Renderer`; strict downward crate layering |
| **High Performance & Low Latency** | Flat contiguous `List[T]` storage + O(1) window reverse index; analytical ODE spring solver (O(1), refresh-rate independent); steady-state zero allocation on hot paths via reusable scratch buffers; X-Ray blur caches backdrop once |
| **Visual Consistency** | Square corners (`corner-radius = 0.0`) by default; the hotkey overlay inherits all visual styling (`corner-radius`, `border-width`, `active-color`, `inactive-color`, `shadow-*`, `background`, `blur`, `overlay` animation curve) directly from config |
| **No Duplication** | Never reimplement what `luminaria`, `luminaria_compose.format`, `vk_gpu`, `vk_draw2d`, `vk_compose`, `std.toml`, or `std.json` already provide |

---

## 2. Crate Layering & Navigation

Dependencies point strictly downward:

```
pastel (binary: CLI, event loop, input, windows, outputs, reload, spawn)
  ├── pastel_ipc    (Unix JSON-lines IPC server & `pastel msg` client)
  ├── pastel_render (pluggable Vulkan Renderer: Dual-Kawase blur, SDF borders, hotkey overlay, anim state)
  ├── pastel_layout (pluggable LayoutPolicy trait, scrolling-tiling strip, workspaces, multi-monitor)
  ├── pastel_config (TOML parser, schema, strict validation, actions, keybindings, window rules)
  └── pastel_core   (geometry, color, structural Curve trait, analytical Spring/Ease, retargetable Anim)
        └── Foundation: luminaria, vk_gpu, vk_draw2d, vk_compose
```

### Quick Modification Map

| Target Area | Primary Files |
|---|---|
| Geometry, colors, curves, spring physics | `pastel_core/src/{geom,color,curve,anim}.mica` |
| TOML schema, validation, actions, keybinds | `pastel_config/src/{schema,action,keys,parse,lib}.mica` |
| Scrolling strip, columns, workspaces, floating, multi-monitor | `pastel_layout/src/{traits,column,scrolling,workspace,desktop}.mica` |
| Dual-Kawase blur shaders, `PastelRenderer`, SDF borders, hotkey overlay | `pastel_render/shaders/*`, `pastel_render/src/{effects,blur,renderer,font,overlay,anim_state}.mica` |
| IPC protocol, Unix socket server, CLI client | `pastel_ipc/src/{lib,server,client}.mica` |
| Compositor loop, input/gestures, window rules, output config, reload | `pastel/src/{main,compositor,input,windows,outputs,reload,spawn}.mica` |

Architecture docs live in [docs/README.md](docs/README.md).

---

## 3. Cross-Layer Invariants

1. **No time in `pastel_layout`**: Layouts are pure spatial functions (state -> target rects). Time, springs, and transitions live exclusively in `pastel_core` and `pastel_render.anim_state`, so any custom `LayoutPolicy` gets full animations automatically.
2. **No double animation**: Pastel drives all window/camera/workspace/overlay animations at `Event.Frame(o, vblank_ns)` and passes `transition_ms = 0` to `luminaria` for real windows/images. While Pastel animations are active, `sync_layout` calls `d.want_frame()` for the next `Frame`; once they settle no frame is asked for (0 wakeups/sec).
3. **Preserve hardware fast paths**: Fullscreen opaque windows without active blur or overlays above them must continue to pass through `PastelRenderer.scanout()` (zero-copy direct scanout) and `overlay_candidates()` (KMS MPO planes).
4. **Blur damage expansion**: Whenever a region uses live backdrop blur, damage rectangles touching that region must expand by the blur kernel radius so partial redraw never shows seams.
5. **Visual parity**: Hotkey overlay styling must always derive from the active `Config` (`[window]` + `background` + `[animation].overlay`), never hardcode diverging corner radii or palette colors.
6. **Zero local absolute paths**: Never commit local filesystem paths in `mica.toml`, docs, comments, or commit messages.
7. **Strict TOML validation**: Unknown config keys must produce clear `line:col` / key-path errors; an empty `config.toml` must yield a complete, functional desktop.

---

## 4. Build, Format, Test & Performance Gates

- **Format**: Run `mica fmt <file>` after editing; verify with `sheaf fmt --check` before committing.
- **Shaders**: Recompile GLSL shaders in `pastel_render/shaders/` with `glslc -I <vk_gpu/shaders>` when modified, and commit updated `.spv` binaries.
- **Bottom-up test order**:
  1. `sheaf test -p pastel_core`
  2. `sheaf test -p pastel_config`
  3. `sheaf test -p pastel_layout`
  4. `sheaf test -p pastel_render`
  5. `sheaf test -p pastel_ipc`
  6. `sheaf test -p pastel`
- **Release build & benchmarks**:
  - `sheaf build --release -p pastel`
  - `sheaf test --release -p pastel` (includes `bench_test.mica`: layout throughput, animation step latency, headless frame time P50/P95/P99, RSS footprint)
- **Vulkan validation**: Run `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation sheaf test -p pastel_render` with zero validation errors when touching shaders or GPU passes.
- **Documentation**: Keep `README.md`, `README.zh-CN.md`, `config.example.toml`, and `docs/` synchronized whenever config options, actions, or architecture change.

---

## 5. Mica Language Reminders

- No `;`, `&`/`*` (in types or unary expressions), `::`, `?`, macros, `impl`, `as`, or `let mut`. Borrowed views are `Slice[T]`, `MutSlice[T]`, and `StrSlice`.
- Item directives use prefix `@attr` (`@derive(...)`, `@packed`, `@align(N)`, `@bits(T)`, `@inline`, ...); byte literals use `b'a'` (`u8`).
- Use `var` for mutable locals, `mut p: T` for mutable parameters, `take p: T` for ownership transfer.
- Explicit `return` in every function; lossless integer widening is implicit, while narrowing, sign-changing, and float/int casts are explicit (`f64(x)`, `i32(x)`).
- Move semantics for `str`, `List`, `Box`, and structs containing them; use `.clone()` when reading fields/elements or `mem.replace(mut self.field, empty)` to reuse scratch buffers without allocating.
- Discard unused `Result` values explicitly (`discard r`).
- Methods live in the same module as their type; structural traits require no `impl` block.
