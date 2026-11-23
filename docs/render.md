# Rendering, Blur & Hotkey Overlay (`pastel_render`)

## 1. `PastelRenderer` (`luminaria.plug.Renderer`)

`PastelRenderer` plugs directly into `luminaria.display.Display[O, PastelRenderer]` via structural trait monomorphization. It reuses `luminaria_compose.format`, `vk_gpu`, `vk_draw2d`, and `vk_compose` — avoiding any duplication of buffer import, shm upload, or capture logic — while adding DWM-style visual effects:

- **Shared `EffectSheet`**: Pastel updates a compact `EffectSheet` (background color, shadow color, blur parameters, per-output decoration list) before calling `d.layout(...)`.
- **Zero-copy fast paths**:
  - **Direct scanout**: When a fullscreen opaque dmabuf window covers the output with no active blur or overlay on top, `scanout(o)` delegates directly to `vk_compose.Scene.scanout()`, bypassing the GPU completely.
  - **Hardware MPO overlays**: When no live blur sits above eligible dmabuf placements, `overlay_candidates` / `set_overlays` promote surfaces to KMS overlay planes.
  - **GPU TDR recovery**: `recover()` rebuilds the Vulkan device, `vk_draw2d.Canvas`, and Dual-Kawase blur pipelines cleanly on `VK_ERROR_DEVICE_LOST`.

## 2. GPU SDF Hollow Borders & Square-Corner Default

Windows default to crisp square corners (`corner-radius = 0.0`). Borders are drawn as GPU signed-distance-field hollow rings using `vk_draw2d.Canvas.border(rect, color, radius, width)` inside the same instanced pass as window textures — zero CPU border bitmaps, zero extra render passes, zero overdraw inside the window interior.

## 3. Dual-Kawase Backdrop Blur

Two Vulkan fragment shaders (`blur_down.frag.spv` 5-tap downsample, `blur_up.frag.spv` 8-tap upsample) and a masked SDF composite shader (`blur_comp.frag.spv`) provide smooth frosted glass at minimal GPU cost:

- **Lazy allocation**: Offscreen blur pyramid textures (1/2, 1/4, 1/8 resolution) are allocated only when a frame actually contains an active blur region, and freed when outputs close or resize.
- **X-Ray mode (`blur-xray = true`, default)**: Blurs the desktop background / wallpaper layer once into a cached half-resolution texture (recomputed only when the background/wallpaper changes). Translucent tiled windows sample that cached texture in a single draw call — **zero extra GPU blur passes per frame** in steady state, ideal for laptops and edge devices.
- **Live mode (`blur-xray = false`, and always for floating windows & hotkey overlay)**: Flushes the scene below the blurred element, runs the Dual-Kawase downsample -> upsample chain on the backdrop, and composites the blurred result clipped to the element's exact SDF shape (square or rounded) before drawing the translucent element above it.
- **Damage expansion**: Active blur regions automatically expand frame damage by the blur radius so partial redraw remains seamless.

## 4. Visually Consistent Hotkey Overlay

The niri-style hotkey overlay (`Mod+Shift+/`, optionally shown on startup) renders clean, anti-aliased TrueType/OpenType typography onto a premultiplied RGBA8 panel (`[19, 21, 27, 209]`) via dynamically loaded `libfontconfig.so.1` and `libfreetype.so.6` (`pastel_render.font`):

- **Zero extra link-time dependencies**: Fontconfig and FreeType2 are loaded lazily at runtime via Mica `extern table` only when rasterizing the overlay, keeping the binary footprint minimal (`~2.1 MB`, 20 shared libraries).
- **Clean typography & layout**: Title `"Hotkeys"`, 4 balanced sections (`Windows`, `Columns`, `Workspaces`, `Session`) split cleanly across two columns, folded keybindings (e.g., `Super + H / Left`) in `[window].active-color`, descriptions in crisp off-white, and `[overlay].font` configurable (default `"sans-serif"`).
- **Visual unity**: Outer hollow border width, active accent color, drop shadow, corner radius (square `0.0` by default), live Dual-Kawase backdrop blur, and show/hide fade+slide animation (`[animation].overlay`) are all inherited directly from the active `Config`.
- **Dynamic content**: Rebuilt straight from the active `[binds]` map whenever configuration reloads live.
